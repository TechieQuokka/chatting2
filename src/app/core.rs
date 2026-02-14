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
use crate::room::{RoomKey, RoomLifetime, RoomStore};
use crate::transfer::{DownloadManager, SeedingManager};

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

    // ── mDNS 피어 목록 (인트라넷 모드 초대용) ────────────────────────────────
    mdns_peers: HashMap<PeerId, Multiaddr>,

    // ── 초대 관리 ─────────────────────────────────────────────────────────────
    invite_manager: InviteManager,
    /// 수신된 InviteRequest의 응답 채널 (초대 번호 → (발신 PeerId, ResponseChannel)).
    pending_invite_channels: HashMap<u32, (PeerId, request_response::ResponseChannel<AppResponse>)>,
    /// 다음 초대 번호 (단조 증가).
    invite_counter: u32,

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
            mdns_peers: HashMap::new(),
            invite_manager: InviteManager::default(),
            pending_invite_channels: HashMap::new(),
            invite_counter: 0,
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
                    } else {
                        // mDNS 이벤트 먼저 처리 (AppCore 내부 상태 갱신)
                        self.preprocess_network_event(&event).await;
                        let room_key = self.active_room.as_ref().map(|r| &r.key);
                        let chat_log = self.active_room.as_ref().map(|r| &r.chat_log);
                        route_network_event(event, room_key, chat_log, &self.app_tx, &self.net_tx).await;
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
                // 로컬 저장소 기반 목록 — 실시간 피어 수 미확인이므로 Some(0) → Offline
                let rooms: Vec<_> = self.room_store.all().iter().map(|r| {
                    (r.room_id, r.name.clone(), Some(0u32))
                }).collect();
                self.app_tx.send(AppEvent::RoomList { rooms }).await.ok();
            }

            AppCommand::ListPeers => {
                let peers: Vec<(PeerId, String)> = self.mdns_peers
                    .iter()
                    .map(|(id, addr)| (*id, addr.to_string()))
                    .collect();
                self.app_tx.send(AppEvent::PeerList { peers }).await.ok();
            }

            // ── 초대 코드 ─────────────────────────────────────────────────
            AppCommand::GenerateInviteCode => {
                self.generate_invite_code().await;
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
                    let my_peer_id_bytes = self.identity.keypair().public().to_peer_id().to_bytes();
                    crate::invite::handler::approve(
                        &mut self.invite_manager,
                        peer,
                        &room_key,
                        &room_topic,
                        my_peer_id_bytes,
                        [0u8; 32],
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
                if self.seeding_manager.entries.is_empty() {
                    self.app_tx.send(AppEvent::Notice("공유 중인 파일 없음".into())).await.ok();
                } else {
                    self.app_tx.send(AppEvent::Notice(format!(
                        "공유 중인 파일 ({}개):", self.seeding_manager.entries.len()
                    ))).await.ok();
                    for (i, e) in self.seeding_manager.entries.iter().enumerate() {
                        let status = match e.status {
                            crate::transfer::seeding::SeedStatus::Active => "시딩",
                            crate::transfer::seeding::SeedStatus::AutoPaused => "자동정지",
                            crate::transfer::seeding::SeedStatus::ManualPaused => "정지",
                        };
                        self.app_tx.send(AppEvent::Notice(format!(
                            "  [{}] {} ({})", i + 1, e.file_name, status
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
        let download_path = std::path::PathBuf::from(&self.config.download_path).join(&file_name);
        self.download_manager.add(file_hash, file_name.clone(), chunk_count, download_path);

        let msg = format!("[↓] '{}' 다운로드 시작", file_name);
        self.app_tx.send(AppEvent::FeedEntry(crate::chat::LogEntry::file_event(&msg))).await.ok();
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
                        self.app_tx.send(AppEvent::InviteCodeGenerated { code }).await.ok();
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

    async fn enter_invite_code(&mut self, code: String) {
        // DHT에서 초대 레코드 조회
        let dht_key = crate::invite::hash_code(&code).to_vec();
        self.net_tx.send(crate::network::event::NetworkCommand::GetRecord {
            key: dht_key,
        }).await.ok();
        self.app_tx.send(AppEvent::Notice(format!("초대 코드 '{}' 조회 중...", code))).await.ok();
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
                }
                self.emit_mdns_update().await;
            }
            NetworkEvent::MdnsExpired(peers) => {
                for (peer_id, _) in peers {
                    self.mdns_peers.remove(peer_id);
                }
                self.emit_mdns_update().await;
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

    // ── InboundRequest 처리 ───────────────────────────────────────────────────

    /// InboundRequest를 수신해 요청 종류에 따라 처리한다.
    ///
    /// - InviteRequest: 응답 채널을 보관하고 TUI에 승인 팝업 요청.
    /// - ChunkRequest:  SeedingManager에서 청크를 읽어 암호화 후 응답.
    /// - BitfieldRequest: 현재 방의 파일 bitfield를 응답.
    async fn handle_inbound_request(&mut self, event: NetworkEvent) {
        let NetworkEvent::InboundRequest { peer, request, channel } = event else { return };
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
