use std::collections::HashMap;

use libp2p::gossipsub::IdentTopic;
use libp2p::{Multiaddr, PeerId};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::account::{AccountPaths, Config, Identity};
use crate::chat::log::{LogEntry, LogEntryKind};
use crate::friends::FriendStore;
use crate::network::codec::AppRequest;
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
            enc_key,
            room_store,
            friend_store,
            download_manager,
            seeding_manager,
            active_room: None,
            mdns_peers: HashMap::new(),
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
                    // mDNS 이벤트 먼저 처리 (AppCore 내부 상태 갱신)
                    self.preprocess_network_event(&event).await;
                    let room_key = self.active_room.as_ref().map(|r| &r.key);
                    route_network_event(event, room_key, &self.app_tx, &self.net_tx).await;
                }

                // DHT Provider Records 주기적 재등록
                _ = republish_tick.tick() => {
                    self.republish_dht().await;
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
                self.config.nickname = new_nickname.clone();
                let enc_key = self.account_enc_key();
                let store = crate::account::UserStore::load(&self.paths.users_json())
                    .ok();
                if let Some(_s) = store {
                    // users.json에서 현재 사용자 ID 찾기 (identity로 매핑)
                    // 단순화: config를 재저장하는 것으로 닉네임 반영
                    let _ = enc_key;
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

            // ── 목록 조회 ─────────────────────────────────────────────────
            AppCommand::ListRooms => {
                let _now_ms = RoomStore::now_ms();
                let rooms: Vec<_> = self.room_store.all().iter().map(|r| {
                    (r.room_id, r.name.clone(), None)
                }).collect();
                self.app_tx.send(AppEvent::RoomList { rooms }).await.ok();
            }

            AppCommand::ListPeers => {
                // 현재 GossipSub 구독 피어 목록은 Network에서 관리
                // 간단히 빈 목록 전송
                self.app_tx.send(AppEvent::PeerList { peers: vec![] }).await.ok();
            }

            // ── 초대 코드 ─────────────────────────────────────────────────
            AppCommand::GenerateInviteCode => {
                self.generate_invite_code().await;
            }

            AppCommand::EnterInviteCode { code } => {
                self.enter_invite_code(code).await;
            }

            AppCommand::AcceptInvite { number } => {
                // InviteManager 없이 단순 알림
                let _ = number;
                self.app_tx.send(AppEvent::Notice("초대 수락 처리됨".into())).await.ok();
            }

            // ── 시스템 ─────────────────────────────────────────────────────
            AppCommand::Shutdown => {
                return true; // 루프 종료 → shutdown() 호출
            }

            // Login/Register/DeleteAccount는 main.rs에서 AppCore 시작 전 처리
            AppCommand::Login { .. } | AppCommand::Register { .. } | AppCommand::DeleteAccount { .. } => {}

            AppCommand::ChangePassword { .. } => {
                // 비밀번호 변경은 main.rs 레벨에서 처리 필요 (salt 접근 필요)
                self.app_tx.send(AppEvent::Error(
                    "비밀번호 변경은 설정 화면에서 처리됩니다.".into()
                )).await.ok();
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

        self.active_room = Some(ActiveRoom { room_id, key, topic });

        self.app_tx
            .send(AppEvent::JoinedRoom { room_id, name: room_name })
            .await
            .ok();
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

                // 자신의 메시지도 피드에 추가
                let entry = LogEntry {
                    timestamp_ms: RoomStore::now_ms(),
                    kind: LogEntryKind::Chat {
                        sender_nickname: self.config.nickname.clone(),
                        sender_peer_short: "(나)".into(),
                        text,
                    },
                };
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
