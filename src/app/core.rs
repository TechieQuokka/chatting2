use std::collections::HashMap;

use libp2p::gossipsub::IdentTopic;
use libp2p::request_response;
use libp2p::{Multiaddr, PeerId};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::account::{AccountPaths, Config, Identity};
use crate::chat::log::{LogEntry, LogEntryKind};
use crate::friends::FriendStore;
use crate::invite::session::InviteManager;
use crate::network::codec::{AppRequest, AppResponse};
use crate::network::event::{NetworkCommand, NetworkEvent};
use crate::protocol::gossip::{self, GossipPayload};
use crate::room::{RoomKey, RoomLifetime, RoomRecord, RoomStore};
use crate::transfer::{DownloadManager, PeerBitfields, SeedingManager};
use crate::tui::render::format_size;

use super::channels::{AppCommand, AppCommandRx, AppEvent, AppEventTx};
use super::router::route_network_event;

// DHT Provider Records를 20분마다 재등록한다.
// Kademlia 레코드 TTL(기본 24h) 내에서 충분히 갱신된다.
const DHT_REPUBLISH_SECS: u64 = 20 * 60;

// ── 방 토픽 헬퍼 ──────────────────────────────────────────────────────────────

/// 방 ID → GossipSub 토픽 이름.
///
/// 형식: `room/<hex32bytes>`
fn room_topic(room_id: &[u8; 32]) -> IdentTopic {
    let hex: String = room_id.iter().map(|b| format!("{b:02x}")).collect();
    IdentTopic::new(format!("room/{hex}"))
}

// ── 현재 방 세션 ──────────────────────────────────────────────────────────────

/// 입장 중인 방의 런타임 상태.
struct ActiveRoom {
    room_id: [u8; 32],
    key: RoomKey,
    topic: IdentTopic,
    /// 방별 채팅 로그 핸들러 (디스크 영속).
    chat_log: crate::chat::log::ChatLog,
}

impl Drop for ActiveRoom {
    fn drop(&mut self) {
        // RoomKey는 ZeroizeOnDrop이므로 drop 시 자동 제로화.
    }
}

// ── AppCore ───────────────────────────────────────────────────────────────────

/// 앱 중심 상태 머신.
///
/// 로그인 후 생성되며, TUI ↔ Network 사이의 모든 로직을 조율한다.
pub struct AppCore {
    // ── 계정 ─────────────────────────────────────────────────────────────────
    pub identity: Identity,
    pub config: Config,
    pub paths: AccountPaths,
    pub user_id: String,
    /// 로그인 시 Argon2id로 파생된 암호화 키 (rooms.enc / friends.enc 용).
    enc_key: [u8; 32],

    // ── 저장소 ───────────────────────────────────────────────────────────────
    pub room_store: RoomStore,
    pub friend_store: FriendStore,
    pub download_manager: DownloadManager,
    pub seeding_manager: SeedingManager,

    // ── 현재 방 ──────────────────────────────────────────────────────────────
    active_room: Option<ActiveRoom>,
    /// 현재 방에서 공유 중인 파일 목록 (모든 피어, file_hash → FileAnnounce).
    available_files: HashMap<[u8; 32], crate::protocol::gossip::FileAnnounce>,

    // ── mDNS 피어 목록 (인트라넷 모드 초대용) ────────────────────────────────
    mdns_peers: HashMap<PeerId, Multiaddr>,

    // ── 현재 방 피어 목록 (DHT Provider 기반) ─────────────────────────────────
    room_peers: HashMap<PeerId, Multiaddr>,

    // ── 초대 관리 ─────────────────────────────────────────────────────────────
    invite_manager: InviteManager,
    /// 수신된 InviteRequest의 응답 채널 (초대 번호 → (발신 PeerId, ResponseChannel)).
    pending_invite_channels: HashMap<u32, (PeerId, request_response::ResponseChannel<AppResponse>)>,
    /// 다음 초대 번호 (단조 증가).
    invite_counter: u32,
    /// DHT 조회 중인 URL (KadGetRecordResult 처리용, URL 방 목록 조회).
    pending_url_lookup: Option<String>,
    /// DHT 조회 중인 초대 코드 (KadGetRecordResult 처리용).
    pending_invite_code: Option<String>,
    /// 피초대자 측: InviteRequest 전송 후 InviteAccepted 대기 중인 방 ID.
    pending_invite_room_id: Option<[u8; 32]>,

    // ── 파일 전송 ─────────────────────────────────────────────────────────────
    /// 피어별 청크 보유 현황 (다운로드 스케줄링용).
    peer_bitfields: PeerBitfields,

    // ── 채널 ─────────────────────────────────────────────────────────────────
    cmd_rx: AppCommandRx,
    app_tx: AppEventTx,
    net_tx: mpsc::Sender<NetworkCommand>,
    net_rx: mpsc::Receiver<NetworkEvent>,
}

impl AppCore {
    pub fn new(
        identity: Identity,
        config: Config,
        paths: AccountPaths,
        user_id: String,
        enc_key: [u8; 32],
        room_store: RoomStore,
        friend_store: FriendStore,
        download_manager: DownloadManager,
        seeding_manager: SeedingManager,
        cmd_rx: AppCommandRx,
        app_tx: AppEventTx,
        net_tx: mpsc::Sender<NetworkCommand>,
        net_rx: mpsc::Receiver<NetworkEvent>,
    ) -> Self {
        Self {
            identity,
            config,
            paths,
            user_id,
            enc_key,
            room_store,
            friend_store,
            download_manager,
            seeding_manager,
            active_room: None,
            available_files: HashMap::new(),
            mdns_peers: HashMap::new(),
            room_peers: HashMap::new(),
            invite_manager: InviteManager::default(),
            pending_invite_channels: HashMap::new(),
            invite_counter: 0,
            pending_url_lookup: None,
            pending_invite_code: None,
            pending_invite_room_id: None,
            peer_bitfields: PeerBitfields::default(),
            cmd_rx,
            app_tx,
            net_tx,
            net_rx,
        }
    }

    /// 메인 이벤트 루프.
    ///
    /// `shutdown_rx`가 신호를 받으면 정리 후 종료한다.
    pub async fn run(mut self, mut shutdown_rx: tokio::sync::oneshot::Receiver<()>) {
        // DHT Provider Records 주기적 재등록 타이머.
        let mut republish_tick = interval(Duration::from_secs(DHT_REPUBLISH_SECS));
        republish_tick.tick().await; // 첫 tick 즉시 소비 (시작 직후 재등록 방지)

        // 05-room.md / D-07: 채팅 화면 입장 중 1분 간격 만료 체크 타이머.
        let mut expiry_check_tick = interval(Duration::from_secs(60));
        expiry_check_tick.tick().await; // 첫 tick 즉시 소비

        loop {
            tokio::select! {
                // 앱 커맨드 처리 (TUI → App)
                Some(cmd) = self.cmd_rx.recv() => {
                    if self.handle_command(cmd).await {
                        // handle_command가 true를 반환하면 종료
                        break;
                    }
                }

                // 네트워크 이벤트 처리 (Network → App)
                Some(event) = self.net_rx.recv() => {
                    // InboundRequest는 ResponseChannel 소유권이 필요하므로 분리 처리
                    if let NetworkEvent::InboundRequest { .. } = event {
                        self.handle_inbound_request(event).await;
                    } else if let NetworkEvent::InboundResponse { peer, response } = event {
                        // InboundResponse: ChunkResponse, BitfieldResponse, InviteAccepted/Rejected
                        self.handle_inbound_response(peer, response).await;
                    } else {
                        // mDNS 이벤트 먼저 처리 (AppCore 내부 상태 갱신)
                        self.preprocess_network_event(&event).await;
                        // KadGetRecordResult: URL 방 목록 조회 또는 초대 코드 조회 처리
                        if let NetworkEvent::KadGetRecordResult { ref result, .. } = event {
                            if self.pending_url_lookup.is_some() {
                                match result {
                                    Ok(bytes) => {
                                        let bytes = bytes.clone();
                                        self.handle_url_dht_result(bytes).await;
                                    }
                                    Err(_) => {
                                        self.pending_url_lookup.take();
                                        self.app_tx.send(AppEvent::UrlNotFound).await.ok();
                                    }
                                }
                            } else if self.pending_invite_code.is_some() {
                                // 초대 코드 DHT 조회 결과 — 성공/실패 모두 명시적으로 처리
                                match result {
                                    Ok(bytes) => {
                                        let bytes = bytes.clone();
                                        self.handle_invite_dht_result(bytes).await;
                                    }
                                    Err(_) => {
                                        self.pending_invite_code = None;
                                        self.app_tx.send(AppEvent::Error(
                                            "초대 코드를 찾을 수 없습니다. 코드가 만료됐거나 잘못됐습니다.".into()
                                        )).await.ok();
                                    }
                                }
                            }
                            // else: pending 없는 stale DHT 응답 → 무시
                        } else if let NetworkEvent::KadGetProvidersResult { ref key, ref providers } = event {
                            // 방 입장 시 동기화: 각 Provider에게 BitfieldRequest 전송
                            // 방 목록 배경 조회: RoomPeerCount 이벤트 emit
                            let key_bytes = key.as_ref().to_vec();
                            let providers = providers.clone();
                            self.handle_providers_for_sync(&key_bytes, providers).await;
                        } else {
                            let room_key = self.active_room.as_ref().map(|r| &r.key);
                            let chat_log = self.active_room.as_ref().map(|r| &r.chat_log);
                            route_network_event(event, room_key, chat_log, &self.app_tx, &self.net_tx).await;
                        }
                    }
                }

                // DHT Provider Records 주기적 재등록
                _ = republish_tick.tick() => {
                    self.republish_dht().await;
                }

                // 채팅 화면 입장 중 1분 간격 방 만료 체크 (D-07)
                _ = expiry_check_tick.tick() => {
                    if let Some(room) = &self.active_room {
                        if self.room_store.is_expired(&room.room_id) {
                            self.app_tx.send(AppEvent::RoomExpired).await.ok();
                        }
                    }
                }

                // 종료 신호 수신
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }

        // ── Graceful shutdown ─────────────────────────────────────────────────
        self.shutdown().await;
    }

    // ── 커맨드 핸들러 ─────────────────────────────────────────────────────────

    /// 커맨드 처리. `true`를 반환하면 루프 종료.
    async fn handle_command(&mut self, cmd: AppCommand) -> bool {
        match cmd {
            // ── 방 ─────────────────────────────────────────────────────────
            AppCommand::JoinRoom { room_id } => {
                self.join_room(room_id).await;
            }

            AppCommand::CreateRoom { name, lifetime } => {
                self.create_room(name, lifetime).await;
            }

            AppCommand::LeaveRoom => {
                self.leave_room().await;
            }

            // ── 채팅 ───────────────────────────────────────────────────────
            AppCommand::SendMessage { text } => {
                self.send_message(text).await;
            }

            // ── 초대 ───────────────────────────────────────────────────────
            AppCommand::InviteMdnsPeer { peer_id_bytes } => {
                self.invite_peer_by_bytes(peer_id_bytes).await;
            }

            AppCommand::InviteFriend { peer_id_bytes } => {
                // 친구의 PeerId로 직접 연결 시도 → InviteRequest 전송.
                // 연결이 이미 있으면 바로 전송, 없으면 dial 후 Identify 완료 시 재시도.
                self.invite_peer_by_bytes(peer_id_bytes).await;
            }

            // ── 파일 전송 ──────────────────────────────────────────────────
            AppCommand::ShareFile { path } => {
                self.share_file(path).await;
            }

            AppCommand::DownloadFileByNumber { number } => {
                // /list에서 표시된 번호로 다운로드
                if number == 0 || number as usize > self.available_files.len() {
                    self.app_tx.send(AppEvent::Error(format!("잘못된 번호: {}", number))).await.ok();
                } else {
                    // available_files의 n번째 항목 가져오기
                    if let Some(announce) = self.available_files.values().nth((number - 1) as usize) {
                        if let Some(first_file) = announce.files.first() {
                            self.start_download(
                                first_file.file_hash,
                                announce.name.clone(),
                                first_file.chunk_count,
                            ).await;
                        }
                    }
                }
            }

            AppCommand::StartDownload { file_hash, file_name, chunk_count } => {
                self.start_download(file_hash, file_name, chunk_count).await;
            }

            // 선택적 다운로드에서 여러 파일 동시 다운로드 (12-tui.md: 파일 선택 화면)
            AppCommand::StartDownloads { files } => {
                for (file_hash, file_name, chunk_count) in files {
                    self.start_download(file_hash, file_name, chunk_count).await;
                }
            }

            AppCommand::PauseDownload { number } => {
                self.download_manager.pause(number);
            }

            AppCommand::ResumeDownload { number } => {
                self.download_manager.resume(number);
            }

            AppCommand::CancelDownload { number } => {
                self.download_manager.cancel(number);
            }

            AppCommand::MoveDownloadTop { number } => {
                self.download_manager.top(number);
            }

            AppCommand::MoveDownloadUp { number } => {
                self.download_manager.up(number);
            }

            AppCommand::MoveDownloadDown { number } => {
                self.download_manager.down(number);
            }

            AppCommand::SeedPause { number } => {
                self.seeding_manager.manual_pause(number);
            }

            AppCommand::SeedResume { number } => {
                self.seeding_manager.resume(number);
            }

            AppCommand::RemoveSeed { number, .. } => {
                self.seeding_manager.remove(number);
            }

            // ── 설정 ───────────────────────────────────────────────────────
            AppCommand::ChangeNickname { new_nickname } => {
                // 03-account.md: 닉네임 변경은 config.enc와 users.json 모두 갱신
                self.config.nickname = new_nickname.clone();
                let config_path = self.paths.config_enc(&self.user_id);
                let enc_key = self.account_enc_key();
                match self.config.save_with_enc_key(&config_path, &enc_key) {
                    Ok(()) => {}
                    Err(e) => {
                        self.app_tx.send(AppEvent::Error(format!("닉네임 저장 실패: {e}"))).await.ok();
                        return false;
                    }
                }
                // users.json의 nickname 필드도 동기화 (로그인 화면 목록 표시용)
                if let Ok(mut store) = crate::account::UserStore::load(&self.paths.users_json()) {
                    let _ = store.update_nickname(&self.user_id, new_nickname.clone());
                }
                self.app_tx.send(AppEvent::Notice(format!("닉네임 변경됨: {new_nickname}"))).await.ok();
            }

            AppCommand::AddFriend { peer_id_bytes } => {
                let enc_key = self.account_enc_key();
                let nickname = String::new(); // Identify 수신 전 빈 닉네임
                let record = crate::friends::FriendRecord::new(peer_id_bytes, nickname);
                let _ = self.friend_store.add(record);
                self.friend_store.save(&enc_key).ok();
                self.app_tx.send(AppEvent::Notice("친구 추가됨".into())).await.ok();
            }

            AppCommand::RemoveFriend { peer_id_bytes } => {
                let enc_key = self.account_enc_key();
                let _ = self.friend_store.remove(&peer_id_bytes);
                self.friend_store.save(&enc_key).ok();
                self.app_tx.send(AppEvent::Notice("친구 삭제됨".into())).await.ok();
            }

            AppCommand::DeleteRoom { room_id } => {
                match self.room_store.remove(&room_id) {
                    Ok(()) => {
                        let enc_key = self.account_enc_key();
                        self.room_store.save(&enc_key).ok();
                        self.app_tx.send(AppEvent::Notice("방이 삭제됐습니다.".into())).await.ok();
                    }
                    Err(e) => {
                        self.app_tx.send(AppEvent::Error(format!("방 삭제 실패: {e}"))).await.ok();
                    }
                }
            }

            // ── 목록 조회 ─────────────────────────────────────────────────
            AppCommand::ListRooms => {
                // 05-room.md: 방 목록 진입 시 만료 방 자동 정리
                let removed = self.room_store.remove_expired();
                if removed > 0 {
                    let enc_key = self.account_enc_key();
                    self.room_store.save(&enc_key).ok();
                }
                // 05-room.md: 캐시 즉시 표시 (None → Checking 상태)
                let rooms: Vec<_> = self.room_store.all().iter().map(|r| {
                    (r.room_id, r.name.clone(), None)
                }).collect();
                // 백그라운드 DHT 조회를 위해 room_id 목록 미리 수집
                let room_ids: Vec<[u8; 32]> = rooms.iter().map(|(id, _, _)| *id).collect();
                self.app_tx.send(AppEvent::RoomList { rooms }).await.ok();
                // 각 방 피어 수 배경 DHT 조회
                for room_id in room_ids {
                    self.net_tx
                        .send(NetworkCommand::GetProviders { key: room_id.to_vec() })
                        .await
                        .ok();
                }
            }

            AppCommand::ListPeers => {
                // 현재 방의 피어만 표시 (DHT Provider 기반)
                let peers: Vec<(PeerId, String)> = self.room_peers
                    .iter()
                    .map(|(id, addr)| (*id, addr.to_string()))
                    .collect();
                self.app_tx.send(AppEvent::PeerList { peers }).await.ok();
            }

            AppCommand::Refresh => {
                // 방 전체 상태 재동기화
                let Some(ref active) = self.active_room else {
                    self.app_tx.send(AppEvent::Error("방에 입장하지 않았습니다.".into())).await.ok();
                    return false;
                };

                let room_id = active.room_id;

                // 피드에 시스템 메시지 추가
                self.app_tx
                    .send(AppEvent::FeedEntry(crate::chat::log::LogEntry::system("방 상태를 새로고침하는 중...")))
                    .await
                    .ok();

                // 1. 피어 목록 재동기화 (DHT GetProviders)
                self.net_tx
                    .send(NetworkCommand::GetProviders { key: room_id.to_vec() })
                    .await
                    .ok();

                // 2. 파일 목록 재동기화 (DHT GetRecord)
                self.net_tx
                    .send(NetworkCommand::GetRecord { key: room_id.to_vec() })
                    .await
                    .ok();

                // 3. 모든 room_peers에게 BitfieldRequest 재전송
                for peer_id in self.room_peers.keys() {
                    self.net_tx
                        .send(NetworkCommand::SendRequest {
                            peer: *peer_id,
                            request: AppRequest::BitfieldRequest { room_id },
                        })
                        .await
                        .ok();
                }

                self.app_tx
                    .send(AppEvent::FeedEntry(crate::chat::log::LogEntry::system("새로고침 완료")))
                    .await
                    .ok();
            }

            // ── 초대 코드 ─────────────────────────────────────────────────
            AppCommand::GenerateInviteCode => {
                self.generate_invite_code().await;
            }

            AppCommand::LookupRoomUrl { url } => {
                self.lookup_room_url(url).await;
            }

            AppCommand::EnterInviteCode { code } => {
                self.enter_invite_code(code).await;
            }

            AppCommand::AcceptInvite { number } => {
                let target_num = if let Some(n) = number {
                    n
                } else {
                    // 번호 미지정 시 가장 오래된 대기 초대
                    match self.pending_invite_channels.keys().copied().next() {
                        Some(n) => n,
                        None => {
                            self.app_tx.send(AppEvent::Error("처리할 초대 요청이 없습니다.".into())).await.ok();
                            return false;
                        }
                    }
                };
                if let Some((peer, channel)) = self.pending_invite_channels.remove(&target_num) {
                    let Some(room) = &self.active_room else {
                        self.app_tx.send(AppEvent::Error("방에 입장한 상태에서만 초대를 수락할 수 있습니다.".into())).await.ok();
                        return false;
                    };
                    let room_key = room.key.clone();
                    let room_topic = room.topic.clone();
                    let room_id = room.room_id;
                    let my_peer_id_bytes = self.identity.keypair().public().to_peer_id().to_bytes();
                    // 방 이름을 rooms.enc에서 조회 (피초대자가 rooms.enc에 저장할 때 사용)
                    let room_name = self.room_store.get(&room_id)
                        .map(|r| r.name.clone())
                        .unwrap_or_default();
                    crate::invite::handler::approve(
                        &mut self.invite_manager,
                        peer,
                        &room_key,
                        &room_topic,
                        my_peer_id_bytes,
                        [0u8; 32],
                        room_name,
                        channel,
                        &self.net_tx,
                    ).await;
                    self.app_tx.send(AppEvent::Notice("초대를 수락했습니다.".into())).await.ok();
                } else {
                    self.app_tx.send(AppEvent::Error("해당 번호의 초대 요청을 찾을 수 없습니다.".into())).await.ok();
                }
            }

            AppCommand::DeclineInvite { number } => {
                let target_num = if let Some(n) = number {
                    n
                } else {
                    match self.pending_invite_channels.keys().copied().next() {
                        Some(n) => n,
                        None => {
                            self.app_tx.send(AppEvent::Error("처리할 초대 요청이 없습니다.".into())).await.ok();
                            return false;
                        }
                    }
                };
                if let Some((peer, channel)) = self.pending_invite_channels.remove(&target_num) {
                    let Some(room) = &self.active_room else {
                        self.app_tx.send(AppEvent::Error("방에 입장한 상태에서만 초대를 거절할 수 있습니다.".into())).await.ok();
                        return false;
                    };
                    let room_key = room.key.clone();
                    let room_topic = room.topic.clone();
                    let my_peer_id_bytes = self.identity.keypair().public().to_peer_id().to_bytes();
                    crate::invite::handler::reject(
                        &mut self.invite_manager,
                        peer,
                        crate::network::codec::RejectReason::Declined,
                        &room_key,
                        &room_topic,
                        my_peer_id_bytes,
                        [0u8; 32],
                        channel,
                        &self.net_tx,
                    ).await;
                    self.app_tx.send(AppEvent::Notice("초대를 거절했습니다.".into())).await.ok();
                } else {
                    self.app_tx.send(AppEvent::Error("해당 번호의 초대 요청을 찾을 수 없습니다.".into())).await.ok();
                }
            }

            // ── 설정 ───────────────────────────────────────────────────────
            AppCommand::EnterSettings => {
                self.send_config_snapshot().await;
            }

            AppCommand::UpdateConfigField { field, value } => {
                match field.as_str() {
                    "nickname" => {
                        self.config.nickname = value.clone();
                    }
                    "network_mode" => {
                        self.config.network_mode = if value.contains("인트라") {
                            crate::account::NetworkMode::Intranet
                        } else {
                            crate::account::NetworkMode::Internet
                        };
                    }
                    "port" => {
                        if let Ok(p) = value.trim().parse::<u16>() {
                            self.config.port = p;
                        } else {
                            self.app_tx.send(AppEvent::Error("포트는 0–65535 숫자여야 합니다.".into())).await.ok();
                            return false;
                        }
                    }
                    "max_connections" => {
                        if let Ok(n) = value.trim().parse::<u32>() {
                            self.config.max_connections = n;
                        } else {
                            self.app_tx.send(AppEvent::Error("숫자를 입력하세요.".into())).await.ok();
                            return false;
                        }
                    }
                    "download_path" => {
                        self.config.download_path = value.clone();
                    }
                    "max_concurrent_dl" => {
                        if let Ok(n) = value.trim().parse::<u32>() {
                            self.config.max_concurrent_downloads = n;
                            self.download_manager.max_concurrent = n as usize;
                        } else {
                            self.app_tx.send(AppEvent::Error("숫자를 입력하세요.".into())).await.ok();
                            return false;
                        }
                    }
                    "max_upload_kbps" => {
                        if let Ok(n) = value.trim().parse::<u32>() {
                            self.config.max_upload_kbps = n;
                            self.seeding_manager.set_upload_limit(n as u64 * 1024);
                        } else {
                            self.app_tx.send(AppEvent::Error("숫자를 입력하세요 (0=무제한).".into())).await.ok();
                            return false;
                        }
                    }
                    "max_download_kbps" => {
                        if let Ok(n) = value.trim().parse::<u32>() {
                            self.config.max_download_kbps = n;
                        } else {
                            self.app_tx.send(AppEvent::Error("숫자를 입력하세요 (0=무제한).".into())).await.ok();
                            return false;
                        }
                    }
                    "log_path" => {
                        self.config.log_path = value.clone();
                    }
                    "language" => {
                        self.config.language = if value.contains("English") {
                            crate::account::Language::English
                        } else {
                            crate::account::Language::Korean
                        };
                    }
                    _ => {}
                }

                // config.enc 저장
                let config_path = self.paths.config_enc(&self.user_id);
                let enc_key = self.account_enc_key();
                match self.config.save_with_enc_key(&config_path, &enc_key) {
                    Ok(()) => {
                        self.send_config_snapshot().await;
                    }
                    Err(e) => {
                        self.app_tx.send(AppEvent::Error(format!("설정 저장 실패: {e}"))).await.ok();
                    }
                }
            }

            // ── 목록 조회 ──────────────────────────────────────────────────
            AppCommand::ListFiles => {
                if self.available_files.is_empty() {
                    self.app_tx.send(AppEvent::Notice("이 방에 공유된 파일 없음".into())).await.ok();
                } else {
                    self.app_tx.send(AppEvent::Notice(format!(
                        "이 방의 공유 파일 ({}개):", self.available_files.len()
                    ))).await.ok();
                    for (i, announce) in self.available_files.values().enumerate() {
                        let size_str = format_size(announce.total_size);
                        self.app_tx.send(AppEvent::Notice(format!(
                            "  [{}] {} ({})", i + 1, announce.name, size_str
                        ))).await.ok();
                    }
                }
            }

            AppCommand::ListDownloads => {
                if self.download_manager.entries.is_empty() {
                    self.app_tx.send(AppEvent::Notice("다운로드 목록 없음".into())).await.ok();
                } else {
                    self.app_tx.send(AppEvent::Notice(format!(
                        "다운로드 목록 ({}개):", self.download_manager.entries.len()
                    ))).await.ok();
                    for (i, e) in self.download_manager.entries.iter().enumerate() {
                        let status = match e.status {
                            crate::transfer::DownloadStatus::Active => "다운로드중",
                            crate::transfer::DownloadStatus::AutoPaused => "자동정지",
                            crate::transfer::DownloadStatus::ManualPaused => "정지",
                            crate::transfer::DownloadStatus::Waiting => "대기중",
                            crate::transfer::DownloadStatus::Completed => "완료",
                            crate::transfer::DownloadStatus::Cancelled => "취소됨",
                        };
                        self.app_tx.send(AppEvent::Notice(format!(
                            "  [{}] {} {:.0}% ({})", i + 1, e.file_name, e.progress_pct(), status
                        ))).await.ok();
                    }
                }
            }

            AppCommand::ListSeeds => {
                if self.seeding_manager.entries.is_empty() {
                    self.app_tx.send(AppEvent::Notice("시딩 목록 없음".into())).await.ok();
                } else {
                    self.app_tx.send(AppEvent::Notice(format!(
                        "시딩 목록 ({}개):", self.seeding_manager.entries.len()
                    ))).await.ok();
                    for (i, e) in self.seeding_manager.entries.iter().enumerate() {
                        let status = match e.status {
                            crate::transfer::seeding::SeedStatus::Active => "시딩중",
                            crate::transfer::seeding::SeedStatus::AutoPaused => "자동정지",
                            crate::transfer::seeding::SeedStatus::ManualPaused => "수동정지",
                        };
                        self.app_tx.send(AppEvent::Notice(format!(
                            "  [{}] {} ({}) — {}", i + 1, e.file_name, status,
                            e.local_path.display()
                        ))).await.ok();
                    }
                }
            }

            // ── 시스템 ─────────────────────────────────────────────────────
            AppCommand::Shutdown => {
                return true; // 루프 종료 → shutdown() 호출
            }

            // Login/Register/DeleteAccount는 main.rs에서 AppCore 시작 전 처리
            AppCommand::Login { .. } | AppCommand::Register { .. } | AppCommand::DeleteAccount { .. } => {}

            AppCommand::ChangePassword { current, new_pw } => {
                match crate::account::session::change_password(
                    &self.paths,
                    &self.user_id,
                    current.as_bytes(),
                    new_pw.as_bytes(),
                ) {
                    Ok(()) => {
                        self.app_tx.send(AppEvent::Notice("비밀번호가 변경되었습니다.".into())).await.ok();
                    }
                    Err(e) => {
                        self.app_tx.send(AppEvent::Error(format!("비밀번호 변경 실패: {e}"))).await.ok();
                    }
                }
            }
        }

        false
    }

    // ── 방 관련 ───────────────────────────────────────────────────────────────

    async fn join_room(&mut self, room_id: [u8; 32]) {
        // 이미 다른 방에 입장 중이면 먼저 퇴장
        if self.active_room.is_some() {
            self.leave_room().await;
        }

        let Some(record) = self.room_store.get(&room_id) else {
            self.app_tx
                .send(AppEvent::Error("방을 찾을 수 없습니다.".into()))
                .await
                .ok();
            return;
        };

        // 만료 체크
        let now_ms = RoomStore::now_ms();
        if record.is_expired(now_ms) {
            self.app_tx
                .send(AppEvent::RoomExpired)
                .await
                .ok();
            return;
        }

        let room_name = record.name.clone();
        let topic = room_topic(&room_id);
        let key = record.key.clone();

        // GossipSub 토픽 구독
        self.net_tx
            .send(NetworkCommand::Subscribe { topic: topic.clone() })
            .await
            .ok();

        // DHT Provider Records 등록 (자신이 이 방의 멤버임을 광고)
        self.net_tx
            .send(NetworkCommand::StartProviding { key: room_id.to_vec() })
            .await
            .ok();

        // URL 레코드 갱신: user_id → 전체 방 목록 PUT
        // 방 입장 시마다 등록해 피초대자가 user_id로 방을 탐색할 수 있게 한다.
        self.put_url_record().await;

        // 05-room.md: 방 입장 시 동기화 — 기존 피어들에게 BitfieldRequest 전송
        self.net_tx
            .send(NetworkCommand::GetProviders { key: room_id.to_vec() })
            .await
            .ok();

        // 시딩 재개 (자동 일시정지 상태만)
        self.seeding_manager.auto_resume_on_rejoin();

        // 다운로드 재개 (자동 일시정지 상태만)
        self.download_manager.auto_resume_on_rejoin();

        // 채팅 로그 핸들러 초기화 및 이전 내역 로드
        let log_dir = std::path::Path::new(&self.config.log_path);
        let chat_log = crate::chat::log::ChatLog::new(log_dir, &room_id)
            .unwrap_or_else(|_| {
                // 설정 경로 실패 시 임시 디렉토리로 fallback
                let tmp = std::env::temp_dir().join("chatapp_logs");
                crate::chat::log::ChatLog::new(&tmp, &room_id)
                    .expect("임시 로그 디렉토리 생성 실패")
            });
        let history = chat_log.load_all().unwrap_or_default();

        self.active_room = Some(ActiveRoom { room_id, key, topic, chat_log });

        // 방 입장 이벤트 먼저 전송 — TUI가 ChatState를 생성한 뒤 피드에 내역을 채움
        self.app_tx
            .send(AppEvent::JoinedRoom { room_id, name: room_name })
            .await
            .ok();

        // 이전 채팅 내역 재생 (최대 500개, P2P 특성상 개인 내역만 표시)
        const MAX_HISTORY: usize = 500;
        let start = history.len().saturating_sub(MAX_HISTORY);
        let history_slice = &history[start..];
        for entry in history_slice.iter() {
            self.app_tx.send(AppEvent::FeedEntry(entry.clone())).await.ok();
        }
        // 이전 내역과 현재 세션 구분선
        if !history_slice.is_empty() {
            self.app_tx
                .send(AppEvent::FeedEntry(
                    crate::chat::log::LogEntry::system("─── 여기서 입장 ─────────────────────────────"),
                ))
                .await
                .ok();
        }
    }

    async fn create_room(&mut self, name: String, lifetime: RoomLifetime) {
        let enc_key = self.account_enc_key();
        match self.room_store.create_room(&name, lifetime) {
            Ok(record) => {
                let room_id = record.room_id;
                let room_name = record.name.clone();

                // rooms.enc 저장
                if let Err(e) = self.room_store.save(&enc_key) {
                    self.app_tx
                        .send(AppEvent::Error(format!("방 저장 실패: {e}")))
                        .await
                        .ok();
                    return;
                }

                // 방 생성 후 즉시 입장
                self.join_room(room_id).await;

                self.app_tx
                    .send(AppEvent::Notice(format!("방 '{room_name}' 생성됨")))
                    .await
                    .ok();
            }
            Err(e) => {
                self.app_tx
                    .send(AppEvent::Error(format!("방 생성 실패: {e}")))
                    .await
                    .ok();
            }
        }
    }

    async fn leave_room(&mut self) {
        let Some(room) = self.active_room.take() else { return };

        // GossipSub 토픽 구독 해제 (topic 클론 후 room drop → 키 자동 제로화)
        self.net_tx
            .send(NetworkCommand::Unsubscribe { topic: room.topic.clone() })
            .await
            .ok();

        // 시딩 자동 일시정지 (수동 일시정지 상태는 유지)
        self.seeding_manager.auto_pause_all();

        // 다운로드 자동 일시정지
        self.download_manager.auto_pause_all();

        // RoomKey는 ActiveRoom이 drop되면서 ZeroizeOnDrop으로 자동 제로화됨

        // 방 퇴장 시 PeerBitfields 초기화 (다음 방 입장에서 재구성)
        self.peer_bitfields = PeerBitfields::default();

        // 방 파일 목록 초기화
        self.available_files.clear();

        // 방 피어 목록 초기화
        self.room_peers.clear();

        self.app_tx.send(AppEvent::LeftRoom).await.ok();
    }

    // ── 채팅 ──────────────────────────────────────────────────────────────────

    async fn send_message(&mut self, text: String) {
        let Some(room) = &self.active_room else { return };

        let msg = crate::protocol::gossip::ChatMessage {
            nickname: self.config.nickname.clone(),
            text: text.clone(),
            timestamp_ms: RoomStore::now_ms(),
        };

        let payload = GossipPayload::Chat(msg);
        match gossip::encode(&payload, &room.key.0) {
            Ok(data) => {
                self.net_tx
                    .send(NetworkCommand::Publish {
                        topic: room.topic.clone(),
                        data,
                    })
                    .await
                    .ok();

                // 자신의 메시지도 피드에 추가하고 로그에 저장
                let entry = LogEntry {
                    timestamp_ms: RoomStore::now_ms(),
                    kind: LogEntryKind::Chat {
                        sender_nickname: self.config.nickname.clone(),
                        sender_peer_short: "(나)".into(),
                        text,
                    },
                };
                if let Some(room) = &self.active_room {
                    room.chat_log.append(&entry).ok();
                }
                self.app_tx.send(AppEvent::FeedEntry(entry)).await.ok();
            }
            Err(e) => {
                self.app_tx
                    .send(AppEvent::Error(format!("메시지 암호화 실패: {e:?}")))
                    .await
                    .ok();
            }
        }
    }

    // ── 파일 공유 ─────────────────────────────────────────────────────────────

    async fn share_file(&mut self, path: String) {
        let Some(room) = &self.active_room else {
            self.app_tx.send(AppEvent::Error("방에 입장하지 않은 상태에서 공유 불가".into())).await.ok();
            return;
        };
        let room_key_bytes = room.key.0;
        let topic = room.topic.clone();

        let path_buf = std::path::PathBuf::from(&path);
        match crate::transfer::build_file_announce(&path_buf) {
            Ok(announce) => {
                // 전체 파일 보유 비트필드로 SeedingManager에 등록
                // FileAnnounce에는 file_hash 직접 필드가 없으므로 files[0]에서 가져옴
                if let Some(first_file) = announce.files.first() {
                    let chunk_count = first_file.chunk_count;
                    let mut bitfield = crate::transfer::Bitfield::new(chunk_count);
                    for i in 0..chunk_count { bitfield.set(i); }
                    self.seeding_manager.add(
                        first_file.file_hash,
                        announce.name.clone(),
                        path_buf,
                        bitfield,
                    );
                }

                // GossipSub로 FileAnnounce 브로드캐스트
                let payload = crate::protocol::gossip::GossipPayload::FileAnnounce(announce.clone());
                match crate::protocol::gossip::encode(&payload, &room_key_bytes) {
                    Ok(data) => {
                        self.net_tx.send(crate::network::event::NetworkCommand::Publish { topic, data }).await.ok();
                        let msg = format!("[파일] '{}' 공유 시작", announce.name);
                        self.app_tx.send(AppEvent::FeedEntry(crate::chat::LogEntry::file_event(&msg))).await.ok();
                    }
                    Err(e) => {
                        self.app_tx.send(AppEvent::Error(format!("파일 공유 암호화 실패: {e:?}"))).await.ok();
                    }
                }
            }
            Err(e) => {
                self.app_tx.send(AppEvent::Error(format!("파일 메타데이터 생성 실패: {e:?}"))).await.ok();
            }
        }
    }

    async fn start_download(&mut self, file_hash: [u8; 32], file_name: String, chunk_count: u32) {
        let download_dir = std::path::PathBuf::from(&self.config.download_path);
        // 다운로드 디렉토리가 없으면 생성
        if let Err(e) = std::fs::create_dir_all(&download_dir) {
            self.app_tx.send(AppEvent::Error(format!("다운로드 디렉토리 생성 실패: {e}"))).await.ok();
            return;
        }
        let download_path = download_dir.join(&file_name);
        self.download_manager.add(file_hash, file_name.clone(), chunk_count, download_path);

        // 새 다운로드를 Active 상태로 변경 (DownloadEntry::new는 AutoPaused로 시작)
        if let Some(entry) = self.download_manager.entries.iter_mut().find(|e| e.file_hash == file_hash) {
            entry.status = crate::transfer::DownloadStatus::Active;
        }

        let msg = format!("[↓] '{}' 다운로드 시작", file_name);
        self.app_tx.send(AppEvent::FeedEntry(crate::chat::LogEntry::file_event(&msg))).await.ok();

        // PeerBitfields가 이미 있으면 즉시 청크 요청 디스패치
        let net_tx = self.net_tx.clone();
        crate::transfer::transfer_loop::dispatch_chunk_requests(
            &mut self.download_manager,
            &self.peer_bitfields,
            &net_tx,
        ).await;
    }

    // ── 초대 코드 생성/입력 ───────────────────────────────────────────────────

    async fn generate_invite_code(&mut self) {
        let Some(room) = &self.active_room else {
            self.app_tx.send(AppEvent::Error("방에 입장한 상태에서만 초대 코드 생성 가능".into())).await.ok();
            return;
        };
        let room_id = room.room_id;
        let keypair = self.identity.keypair().clone();

        // 초대 코드 생성 및 DHT 등록
        let code = crate::invite::generate_code();
        match crate::invite::create_dht_record(&keypair, &code, room_id) {
            Ok(record) => {
                match crate::invite::encode_dht_record(&record) {
                    Ok(bytes) => {
                        let dht_key = crate::invite::hash_code(&code).to_vec();
                        self.net_tx.send(crate::network::event::NetworkCommand::PutRecord {
                            key: dht_key,
                            value: bytes,
                        }).await.ok();

                        // URL 레코드 갱신 (방 목록 등록)
                        self.put_url_record().await;

                        self.app_tx.send(AppEvent::InviteCodeGenerated { code, my_id: self.user_id.clone() }).await.ok();
                    }
                    Err(e) => {
                        self.app_tx.send(AppEvent::Error(format!("초대 코드 직렬화 실패: {e}"))).await.ok();
                    }
                }
            }
            Err(e) => {
                self.app_tx.send(AppEvent::Error(format!("초대 코드 생성 실패: {e}"))).await.ok();
            }
        }
    }

    /// 방 이름 → room_id 를 DHT에 PUT한다.
    ///
    /// 방 입장 시 및 초대 코드 생성 시 호출되어 피초대자가 방 이름으로 room_id를 탐색할 수 있게 한다.
    /// 각 방마다 `hash_url(방이름)` 키로 별도 레코드를 등록한다.
    async fn put_url_record(&mut self) {
        let rooms: Vec<_> = self.room_store.all()
            .iter()
            .map(|r| (r.room_id, r.name.clone()))
            .collect();
        for (room_id, name) in rooms {
            let url_key = crate::invite::hash_url(&name).to_vec();
            let entry = vec![crate::invite::UrlRoomEntry {
                room_id,
                identifier: name,
            }];
            if let Ok(url_bytes) = crate::invite::encode_url_record(&entry) {
                self.net_tx.send(crate::network::event::NetworkCommand::PutRecord {
                    key: url_key,
                    value: url_bytes,
                }).await.ok();
            }
        }
    }

    /// URL(초대자 user_id)로 DHT 방 목록 조회.
    async fn lookup_room_url(&mut self, url: String) {
        let url_key = crate::invite::hash_url(&url).to_vec();
        self.pending_url_lookup = Some(url.clone());
        self.net_tx.send(crate::network::event::NetworkCommand::GetRecord {
            key: url_key,
        }).await.ok();
        self.app_tx.send(AppEvent::Notice(format!("'{}' 조회 중...", url))).await.ok();
    }

    /// URL DHT 조회 결과 처리.
    async fn handle_url_dht_result(&mut self, bytes: Vec<u8>) {
        self.pending_url_lookup.take();
        let rooms = match crate::invite::decode_url_record(&bytes) {
            Ok(r) => r,
            Err(_) => {
                self.app_tx.send(AppEvent::UrlNotFound).await.ok();
                return;
            }
        };
        if rooms.is_empty() {
            self.app_tx.send(AppEvent::UrlNotFound).await.ok();
            return;
        }
        let rooms: Vec<([u8; 32], String)> = rooms.into_iter()
            .map(|r| (r.room_id, r.identifier))
            .collect();
        self.app_tx.send(AppEvent::UrlRooms { rooms }).await.ok();
    }

    /// 초대 코드 DHT 조회 → InviteRequest 전송.
    async fn enter_invite_code(&mut self, code: String) {
        let dht_key = crate::invite::hash_code(&code).to_vec();
        self.pending_invite_code = Some(code.clone());
        self.net_tx.send(crate::network::event::NetworkCommand::GetRecord {
            key: dht_key,
        }).await.ok();
        self.app_tx.send(AppEvent::Notice(format!("초대 코드 '{}' 조회 중...", code))).await.ok();
    }

    /// DHT 초대 레코드 조회 성공 시 처리.
    ///
    /// 1. 레코드 디코드 + 서명 검증
    /// 2. creator_public_key → PeerId 변환
    /// 3. InviteRequest 전송
    ///
    /// 검증 실패 시 pending_invite_code를 보존하여 이전 코드 조회의 지연 응답(stale)이
    /// 현재 조회 중인 코드의 상태를 오염시키지 않도록 한다.
    async fn handle_invite_dht_result(&mut self, bytes: Vec<u8>) {
        // take() 대신 clone() — 검증 실패 시 복원할 수 있도록
        let Some(code) = self.pending_invite_code.clone() else { return };

        let record = match crate::invite::code::decode_dht_record(&bytes) {
            Ok(r) => r,
            Err(e) => {
                // DHT 레코드 손상 — 코드를 클리어하고 오류 보고
                self.pending_invite_code = None;
                self.app_tx.send(AppEvent::Error(format!("초대 레코드 디코드 실패: {e}"))).await.ok();
                return;
            }
        };

        if let Err(_) = crate::invite::code::verify_dht_record(&code, &record) {
            // 서명 불일치 — 이전 코드 조회의 stale 응답일 가능성이 높음
            // pending_invite_code를 그대로 보존하고 무시
            return;
        }

        // 검증 성공 — 이제 코드를 클리어하고 진행
        self.pending_invite_code = None;

        let public_key = match libp2p::identity::PublicKey::try_decode_protobuf(&record.creator_public_key) {
            Ok(pk) => pk,
            Err(_) => {
                self.app_tx.send(AppEvent::Error("초대 레코드의 공개키가 유효하지 않습니다.".into())).await.ok();
                return;
            }
        };
        let host_peer_id = public_key.to_peer_id();
        let room_id = record.room_id;
        let my_peer_id_bytes = self.identity.keypair().public().to_peer_id().to_bytes();

        self.app_tx.send(AppEvent::Notice(format!("호스트에게 입장 요청 전송 중..."))).await.ok();

        // InviteAccepted 수신 시 방 ID를 알 수 있도록 저장
        self.pending_invite_room_id = Some(room_id);

        self.net_tx.send(NetworkCommand::SendRequest {
            peer: host_peer_id,
            request: AppRequest::InviteRequest {
                room_id,
                requester_peer_id: my_peer_id_bytes,
            },
        }).await.ok();
    }

    // ── DHT 재등록 ────────────────────────────────────────────────────────────

    async fn republish_dht(&mut self) {
        let Some(room) = &self.active_room else { return };
        let room_id = room.room_id;

        self.net_tx
            .send(NetworkCommand::StartProviding { key: room_id.to_vec() })
            .await
            .ok();
    }

    // ── Graceful Shutdown ─────────────────────────────────────────────────────

    async fn shutdown(&mut self) {
        // 방 퇴장 처리 (GossipSub 해제 + 시딩/다운로드 일시정지 + 키 제로화)
        if self.active_room.is_some() {
            self.leave_room().await;
        }

        // rooms.enc 저장
        let enc_key = self.account_enc_key();
        self.room_store.save(&enc_key).ok();

        // friends.enc 저장
        self.friend_store.save(&enc_key).ok();
    }

    // ── 헬퍼 ──────────────────────────────────────────────────────────────────

    /// 현재 Config를 ConfigSnapshot 이벤트로 TUI에 전송한다.
    async fn send_config_snapshot(&self) {
        use crate::account::{Language, NetworkMode};
        let network_mode = match self.config.network_mode {
            NetworkMode::Internet => "인터넷".to_string(),
            NetworkMode::Intranet => "인트라넷".to_string(),
        };
        let language = match self.config.language {
            Language::Korean => "Korean".to_string(),
            Language::English => "English".to_string(),
        };
        self.app_tx.send(AppEvent::ConfigSnapshot {
            user_id: self.user_id.clone(),
            nickname: self.config.nickname.clone(),
            network_mode,
            port: self.config.port.to_string(),
            max_connections: self.config.max_connections.to_string(),
            download_path: self.config.download_path.clone(),
            max_concurrent_dl: self.config.max_concurrent_downloads.to_string(),
            max_upload_kbps: self.config.max_upload_kbps.to_string(),
            max_download_kbps: self.config.max_download_kbps.to_string(),
            log_path: self.config.log_path.clone(),
            language,
        }).await.ok();
    }

    /// 계정 암호화 키 (rooms.enc / friends.enc 암호화에 사용).
    ///
    /// 로그인 시 Argon2id로 파생된 키를 사용한다.
    fn account_enc_key(&self) -> [u8; 32] {
        self.enc_key
    }

    /// 현재 방 키 반환 (없으면 None).
    pub fn current_room_key(&self) -> Option<&RoomKey> {
        self.active_room.as_ref().map(|r| &r.key)
    }

    // ── mDNS 이벤트 전처리 ────────────────────────────────────────────────────

    /// NetworkEvent를 route_network_event에 전달하기 전에 AppCore 내부 상태를 갱신.
    async fn preprocess_network_event(&mut self, event: &NetworkEvent) {
        match event {
            NetworkEvent::MdnsDiscovered(peers) => {
                for (peer_id, addr) in peers {
                    self.mdns_peers.insert(*peer_id, addr.clone());
                    // 현재 방 멤버가 연결되면 room_peers도 업데이트
                    if self.room_peers.contains_key(peer_id) {
                        self.room_peers.insert(*peer_id, addr.clone());
                    }
                }
                self.emit_mdns_update().await;
            }
            NetworkEvent::MdnsExpired(peers) => {
                for (peer_id, _) in peers {
                    self.mdns_peers.remove(peer_id);
                }
                self.emit_mdns_update().await;
            }
            NetworkEvent::PeerConnected { peer_id, addr } => {
                // 토렌트 방식: 연결 성공 시 room_peers에 실제 주소 반영
                if self.room_peers.contains_key(peer_id) {
                    // 실제 연결된 주소로 업데이트 (placeholder 교체)
                    self.room_peers.insert(*peer_id, addr.clone());
                }
            }
            NetworkEvent::GossipMessage { source: Some(peer_id), data, .. } => {
                // 토렌트 방식: GossipSub 메시지 수신 = 상대가 방 멤버 (이벤트 기반 갱신)
                if self.active_room.is_some() && !self.room_peers.contains_key(peer_id) {
                    let addr = self.mdns_peers.get(peer_id)
                        .cloned()
                        .unwrap_or_else(|| format!("/p2p/{}", peer_id).parse().unwrap());
                    self.room_peers.insert(*peer_id, addr);

                    // 연결 시도 (PeerConnected 이벤트에서 실제 주소로 교체됨)
                    self.net_tx.send(NetworkCommand::DialPeerId { peer: *peer_id }).await.ok();

                    // 상태바 업데이트를 위해 RoomPeerCount 이벤트 발생
                    if let Some(ref active) = self.active_room {
                        let count = self.room_peers.len() as u32;
                        self.app_tx.send(AppEvent::RoomPeerCount { room_id: active.room_id, count }).await.ok();
                    }
                }

                // FileAnnounce 메시지 처리: available_files에 저장 + BitfieldRequest 전송
                if let Some(ref room) = self.active_room {
                    if let Ok(payload) = gossip::decode(data, &room.key.0) {
                        if let GossipPayload::FileAnnounce(announce) = payload {
                            // file_hash는 files[0]에서 가져옴
                            if let Some(first_file) = announce.files.first() {
                                self.available_files.insert(first_file.file_hash, announce);

                                // 새 파일 공유 시 해당 피어에게 BitfieldRequest 전송
                                // → peer_bitfields 업데이트 → 다운로드 가능
                                let room_id = room.room_id;
                                self.net_tx.send(NetworkCommand::SendRequest {
                                    peer: *peer_id,
                                    request: AppRequest::BitfieldRequest { room_id },
                                }).await.ok();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn emit_mdns_update(&self) {
        let peers: Vec<(PeerId, String)> = self
            .mdns_peers
            .iter()
            .map(|(id, addr)| (*id, addr.to_string()))
            .collect();
        self.app_tx
            .send(AppEvent::MdnsPeersUpdated { peers })
            .await
            .ok();
    }

    // ── InboundResponse 처리 ──────────────────────────────────────────────────

    /// InboundResponse를 수신해 응답 종류에 따라 처리한다.
    ///
    /// - InviteAccepted:   방 키 추출 → rooms.enc 저장 → 방 입장
    /// - InviteRejected:   에러 메시지 표시
    /// - BitfieldResponse: PeerBitfields 갱신 → 청크 요청 디스패치
    /// - ChunkResponse:    복호화 → 검증 → 디스크 기록 → 진행률 갱신
    async fn handle_inbound_response(&mut self, peer: PeerId, response: AppResponse) {
        use crate::network::codec::RejectReason;

        match response {
            AppResponse::InviteAccepted { encrypted_room_key, room_name } => {
                let Some(room_id) = self.pending_invite_room_id.take() else { return };

                let Some(room_key) = crate::invite::handler::on_invite_accepted(encrypted_room_key) else {
                    self.app_tx.send(AppEvent::Error("방 키 추출 실패".into())).await.ok();
                    return;
                };

                // 방 이름: 수락자가 전송한 실제 이름 사용, 비어있으면 임시 이름으로 대체
                let name = if room_name.is_empty() {
                    let room_hex: String = room_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                    format!("room-{room_hex}")
                } else {
                    room_name
                };

                let record = RoomRecord {
                    room_id,
                    name,
                    key: room_key,
                    created_at_ms: RoomStore::now_ms(),
                    lifetime: RoomLifetime::Unlimited, // 정확한 수명 불명 → 무제한 임시 처리
                    files: Vec::new(),
                    last_sync_ms: None,
                };
                self.room_store.insert(record);
                let enc_key = self.account_enc_key();
                self.room_store.save(&enc_key).ok();

                // 방 입장
                self.join_room(room_id).await;
                let _ = peer;
            }

            AppResponse::InviteRejected { reason } => {
                // 거절 시 대기 중인 방 ID 클리어 — 다음 초대 시도가 오염되지 않도록
                self.pending_invite_room_id = None;
                let msg = match reason {
                    RejectReason::Declined => "초대 거절됨".to_string(),
                    RejectReason::Expired => "초대 코드 만료됨".to_string(),
                    RejectReason::TooManyAttempts => "초대 코드 입력 횟수 초과".to_string(),
                };
                self.app_tx.send(AppEvent::Error(msg)).await.ok();
                let _ = peer;
            }

            AppResponse::BitfieldResponse { files } => {
                self.on_bitfield_response(peer, files).await;
            }

            AppResponse::ChunkResponse { chunk_index, encrypted_data } => {
                self.on_chunk_response(peer, chunk_index, encrypted_data).await;
            }
        }
    }

    /// BitfieldResponse 수신: PeerBitfields 갱신 → 청크 요청 디스패치 → rooms.enc 동기화 시각 갱신.
    async fn on_bitfield_response(&mut self, peer: PeerId, files: Vec<([u8; 32], Vec<u8>)>) {
        let Some(room) = &self.active_room else { return };
        let room_id = room.room_id;

        for (file_hash, bitfield_bytes) in files {
            // available_files에서 chunk_count 가져오기 (방의 공유 파일)
            let chunk_count = self.available_files.values()
                .find_map(|announce| {
                    announce.files.iter()
                        .find(|f| f.file_hash == file_hash)
                        .map(|f| f.chunk_count)
                })
                .or_else(|| {
                    // room_store에서도 확인 (자신이 공유한 파일)
                    self.room_store.get(&room_id)
                        .and_then(|r| r.files.iter().find(|f| f.file_hash == file_hash))
                        .map(|f| f.chunk_count)
                })
                .unwrap_or(0);

            if chunk_count > 0 {
                let bf = crate::transfer::Bitfield::from_bytes(bitfield_bytes, chunk_count);
                self.peer_bitfields.update(file_hash, peer, bf);
            }
        }

        // PeerBitfields 갱신 후 청크 요청 디스패치
        let net_tx = self.net_tx.clone();
        crate::transfer::transfer_loop::dispatch_chunk_requests(
            &mut self.download_manager,
            &self.peer_bitfields,
            &net_tx,
        ).await;

        // rooms.enc 마지막 동기화 시각 갱신
        if let Some(r) = self.room_store.get_mut(&room_id) {
            r.last_sync_ms = Some(RoomStore::now_ms());
        }
        let enc_key = self.account_enc_key();
        self.room_store.save(&enc_key).ok();
    }

    /// ChunkResponse 수신: 복호화 → 해시 검증 → 디스크 기록 → 진행률 이벤트 → 다음 청크 디스패치.
    async fn on_chunk_response(&mut self, peer: PeerId, chunk_index: u32, encrypted_data: Vec<u8>) {
        let Some(room) = &self.active_room else { return };
        let room_key = room.key.0;
        let room_id = room.room_id;

        // in_flight에서 해당 chunk_index를 가진 활성 다운로드 엔트리 찾기
        let Some(entry) = self.download_manager.entries.iter()
            .find(|e| {
                e.status == crate::transfer::DownloadStatus::Active
                    && e.in_flight.contains(&chunk_index)
            })
        else { return };

        let file_hash = entry.file_hash;
        let file_name = entry.file_name.clone();
        let chunk_count = entry.chunk_count;

        // chunk_hashes 조회: available_files 우선, room_store는 fallback
        let chunk_hashes: Vec<[u8; 32]> = self.available_files.get(&file_hash)
            .and_then(|announce| announce.files.iter().find(|f| f.file_hash == file_hash))
            .map(|f| f.chunk_hashes.clone())
            .or_else(|| {
                // fallback: room_store에서도 조회 (자신이 공유한 파일)
                self.room_store.get(&room_id)
                    .and_then(|r| r.files.iter().find(|f| f.file_hash == file_hash))
                    .map(|f| f.chunk_hashes.clone())
            })
            .unwrap_or_default();

        match crate::transfer::transfer_loop::handle_chunk_response(
            &mut self.download_manager,
            peer,
            &file_hash,
            chunk_index,
            encrypted_data,
            &room_key,
            &chunk_hashes,
            &file_hash,
        ) {
            Ok(completed) => {
                if let Some(e) = self.download_manager.entries.iter()
                    .find(|e| e.file_hash == file_hash)
                {
                    let done = e.bitfield.completed();
                    self.app_tx.send(AppEvent::DownloadProgress {
                        file_hash,
                        completed_chunks: done,
                        total_chunks: chunk_count,
                        status: e.status.clone(),
                    }).await.ok();
                }
                if completed {
                    // 완료된 다운로드의 .bf 파일 삭제
                    if let Some(entry) = self.download_manager.entries.iter()
                        .find(|e| e.file_hash == file_hash)
                    {
                        let bf_path = entry.bf_path();
                        let _ = std::fs::remove_file(&bf_path);
                    }
                    self.app_tx.send(AppEvent::DownloadComplete { file_hash, file_name }).await.ok();
                }
            }
            Err(e) => {
                self.app_tx.send(AppEvent::Error(format!("청크 처리 오류: {e}"))).await.ok();
            }
        }

        // 다음 청크 요청 디스패치
        let net_tx = self.net_tx.clone();
        crate::transfer::transfer_loop::dispatch_chunk_requests(
            &mut self.download_manager,
            &self.peer_bitfields,
            &net_tx,
        ).await;
    }

    /// KadGetProvidersResult 수신:
    /// 1. 방 목록 배경 조회 — RoomPeerCount 이벤트 emit (항상)
    /// 2. 방 입장 동기화 — 활성 방 일치 시 각 Provider에게 BitfieldRequest 전송
    async fn handle_providers_for_sync(&mut self, key_bytes: &[u8], providers: Vec<PeerId>) {
        let my_peer_id = self.identity.keypair().public().to_peer_id();

        // key_bytes → room_id (32바이트인 경우에만 처리)
        if key_bytes.len() == 32 {
            let mut room_id = [0u8; 32];
            room_id.copy_from_slice(key_bytes);

            // 자신을 제외한 피어 수 계산
            let peer_count = providers.iter().filter(|p| **p != my_peer_id).count() as u32;

            // 알려진 방에 대해서만 RoomPeerCount 이벤트 emit
            if self.room_store.get(&room_id).is_some() {
                self.app_tx
                    .send(AppEvent::RoomPeerCount { room_id, count: peer_count })
                    .await
                    .ok();
            }

            // 활성 방과 일치하면 토렌트 방식 적용: 모든 Provider에게 자동 연결 시도
            let active_room_id = self.active_room.as_ref().map(|r| r.room_id);
            if active_room_id == Some(room_id) {
                // 현재 방의 피어 목록 갱신 (자신 제외)
                self.room_peers.clear();
                for provider in &providers {
                    if *provider == my_peer_id {
                        continue; // 자신 제외
                    }
                    // mdns_peers에서 multiaddr 가져오기, 없으면 PeerId 기반 placeholder 사용
                    let addr = self.mdns_peers.get(provider)
                        .cloned()
                        .unwrap_or_else(|| {
                            // Fallback: PeerId를 문자열로 변환 (실제 연결 가능한 주소는 libp2p가 관리)
                            format!("/p2p/{}", provider).parse().unwrap()
                        });
                    self.room_peers.insert(*provider, addr);
                }

                // 토렌트 방식 (10-file-transfer.md:9): DHT provider 전원에게 즉시 연결 시도
                // max_connections 제한 내에서 Full mesh 구성 → 파일 전송 효율 최대화
                for provider in &providers {
                    if *provider == my_peer_id {
                        continue;
                    }
                    self.net_tx.send(NetworkCommand::DialPeerId { peer: *provider }).await.ok();
                }

                for provider in providers {
                    if provider == my_peer_id {
                        continue; // 자신에게는 요청하지 않음
                    }
                    self.net_tx.send(NetworkCommand::SendRequest {
                        peer: provider,
                        request: AppRequest::BitfieldRequest { room_id },
                    }).await.ok();
                }
            }
        }
    }

    // ── InboundRequest 처리 ───────────────────────────────────────────────────

    /// InboundRequest를 수신해 요청 종류에 따라 처리한다.
    ///
    /// - InviteRequest: 응답 채널을 보관하고 TUI에 승인 팝업 요청.
    /// - ChunkRequest:  SeedingManager에서 청크를 읽어 암호화 후 응답.
    /// - BitfieldRequest: 현재 방의 파일 bitfield를 응답.
    async fn handle_inbound_request(&mut self, event: NetworkEvent) {
        let NetworkEvent::InboundRequest { peer, request, channel } = event else { return };

        // 토렌트 방식: BitfieldRequest/ChunkRequest 수신 = 상대가 방 멤버
        // 이벤트 기반으로 room_peers 실시간 갱신 (주기적 polling 불필요)
        match &request {
            AppRequest::BitfieldRequest { .. } | AppRequest::ChunkRequest { .. } => {
                if self.active_room.is_some() && !self.room_peers.contains_key(&peer) {
                    // mdns_peers에서 주소 가져오기, 없으면 placeholder
                    let addr = self.mdns_peers.get(&peer)
                        .cloned()
                        .unwrap_or_else(|| format!("/p2p/{}", peer).parse().unwrap());
                    self.room_peers.insert(peer, addr);

                    // 연결 시도 (이미 연결되어 있으면 무시됨, PeerConnected에서 실제 주소로 교체)
                    self.net_tx.send(NetworkCommand::DialPeerId { peer }).await.ok();

                    // 상태바 업데이트를 위해 RoomPeerCount 이벤트 발생
                    if let Some(ref active) = self.active_room {
                        let count = self.room_peers.len() as u32;
                        self.app_tx.send(AppEvent::RoomPeerCount { room_id: active.room_id, count }).await.ok();
                    }
                }
            }
            _ => {}
        }

        match request {
            AppRequest::InviteRequest { room_id, .. } => {
                let room_name = self.room_store
                    .get(&room_id)
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| "알 수 없는 방".to_string());

                let invite_num = self.invite_counter;
                self.invite_counter += 1;
                self.pending_invite_channels.insert(invite_num, (peer, channel));

                let from_display = {
                    let s = peer.to_string();
                    s[s.len().saturating_sub(12)..].to_string()
                };

                crate::invite::handler::on_invite_request(
                    &mut self.invite_manager,
                    peer,
                    room_id,
                    vec![],
                    &self.app_tx,
                    invite_num,
                    room_name,
                    from_display,
                ).await;
            }

            // 08-protocol.md: ChunkResponse는 방 키 AES-256-GCM 암호화
            AppRequest::ChunkRequest { file_hash, chunk_index } => {
                let Some(room) = &self.active_room else {
                    // 방 밖에서 온 청크 요청 — 무시
                    let _ = (peer, channel);
                    return;
                };
                let room_key = room.key.0;

                // SeedingManager에서 해당 파일 경로 조회
                let local_path = self.seeding_manager.local_path(&file_hash).cloned();
                let can_serve = self.seeding_manager.try_serve_with_limit(
                    &file_hash,
                    chunk_index,
                    262144, // 256KB 청크 크기 (10-file-transfer.md)
                );

                if !can_serve {
                    // 해당 청크 없거나 속도 제한 — 응답 없이 드롭 (피어가 timeout 처리)
                    let _ = (local_path, channel);
                    return;
                }

                if let Some(path) = local_path {
                    let offset = (chunk_index as u64) * 262144;
                    match read_chunk_from_file(&path, offset, 262144) {
                        Ok(chunk_data) => {
                            // AES-256-GCM으로 청크 암호화 (nonce||ciphertext)
                            match crate::crypto::encrypt(&room_key, &chunk_data) {
                                Ok(encrypted) => {
                                    let response = crate::network::codec::AppResponse::ChunkResponse {
                                        chunk_index,
                                        encrypted_data: encrypted.0,
                                    };
                                    self.net_tx.send(crate::network::event::NetworkCommand::SendResponse {
                                        channel,
                                        response,
                                    }).await.ok();
                                }
                                Err(e) => {
                                    self.app_tx.send(AppEvent::Error(format!("청크 암호화 실패: {e:?}"))).await.ok();
                                }
                            }
                        }
                        Err(e) => {
                            self.app_tx.send(AppEvent::Error(format!("청크 읽기 실패: {e}"))).await.ok();
                        }
                    }
                }
            }

            // 08-protocol.md: BitfieldResponse — 전체 파일 목록 + 청크 보유 현황
            AppRequest::BitfieldRequest { room_id } => {
                // 요청한 방의 파일 bitfield 목록 응답
                let files: Vec<([u8; 32], Vec<u8>)> = self.seeding_manager.entries
                    .iter()
                    .filter(|e| {
                        // 현재 입장 중인 방과 일치하는지 확인
                        self.active_room.as_ref()
                            .map(|r| r.room_id == room_id)
                            .unwrap_or(false)
                            && e.status == crate::transfer::seeding::SeedStatus::Active
                    })
                    .map(|e| (e.file_hash, e.bitfield.as_bytes().to_vec()))
                    .collect();

                let response = crate::network::codec::AppResponse::BitfieldResponse { files };
                self.net_tx.send(crate::network::event::NetworkCommand::SendResponse {
                    channel,
                    response,
                }).await.ok();
                let _ = peer;
            }
        }
    }

    // ── 피어 초대 ─────────────────────────────────────────────────────────────

    /// PeerId bytes로 특정 피어에게 초대 요청을 보낸다.
    ///
    /// mDNS 탐색 목록 초대 및 친구 초대 모두 이 메서드를 사용한다.
    async fn invite_peer_by_bytes(&mut self, peer_id_bytes: Vec<u8>) {
        let Some(room) = &self.active_room else {
            self.app_tx
                .send(AppEvent::Error("방에 입장하지 않은 상태에서 초대 불가".into()))
                .await
                .ok();
            return;
        };

        let Ok(peer_id) = PeerId::from_bytes(&peer_id_bytes) else {
            self.app_tx
                .send(AppEvent::Error("잘못된 PeerId".into()))
                .await
                .ok();
            return;
        };

        let room_id = room.room_id;
        let my_peer_id_bytes = self.identity.keypair().public().to_peer_id().to_bytes();

        // mDNS 피어면 주소를 알고 있으므로 dial 먼저
        if let Some(addr) = self.mdns_peers.get(&peer_id) {
            self.net_tx
                .send(NetworkCommand::DialPeer { addr: addr.clone() })
                .await
                .ok();
        }

        // InviteRequest 전송
        self.net_tx
            .send(NetworkCommand::SendRequest {
                peer: peer_id,
                request: AppRequest::InviteRequest {
                    room_id,
                    requester_peer_id: my_peer_id_bytes,
                },
            })
            .await
            .ok();
    }
}

// ── 파일 청크 읽기 헬퍼 ──────────────────────────────────────────────────────

/// 지정한 파일의 `offset`부터 최대 `max_size` 바이트를 읽어 반환한다.
///
/// 10-file-transfer.md: 청크 크기 256KB, 마지막 청크는 더 작을 수 있음.
fn read_chunk_from_file(
    path: &std::path::Path,
    offset: u64,
    max_size: u64,
) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; max_size as usize];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}
