use libp2p::PeerId;
use tokio::sync::mpsc;

use crate::chat::LogEntry;
use crate::network::event::NetworkCommand;
use crate::protocol::gossip::FileAnnounce;
use crate::room::RoomLifetime;
use crate::transfer::DownloadStatus;

// ── AppCommand (TUI → App) ────────────────────────────────────────────────────

/// TUI/CLI에서 앱 레이어로 보내는 명령.
#[derive(Debug)]
pub enum AppCommand {
    // ── 인증 ─────────────────────────────────────────────────────────────────
    Login { id: String, password: String },
    Register { id: String, nickname: String, password: String },
    DeleteAccount { id: String, password: String },

    // ── 방 ───────────────────────────────────────────────────────────────────
    CreateRoom { name: String, lifetime: RoomLifetime },
    JoinRoom { room_id: [u8; 32] },
    DeleteRoom { room_id: [u8; 32] },
    LeaveRoom,
    ListRooms,

    // ── 채팅 ─────────────────────────────────────────────────────────────────
    SendMessage { text: String },
    ListPeers,

    // ── 초대 ─────────────────────────────────────────────────────────────────
    GenerateInviteCode,
    /// URL(초대자의 user_id)로 DHT 조회 → 방 목록 수신.
    LookupRoomUrl { url: String },
    /// 초대 코드 입력 → DHT 조회 → InviteRequest 전송.
    EnterInviteCode { code: String },
    AcceptInvite { number: Option<u32> },
    DeclineInvite { number: Option<u32> },
    /// mDNS 탐색 목록에서 피어를 직접 초대 (인트라넷 모드).
    InviteMdnsPeer { peer_id_bytes: Vec<u8> },
    /// 친구를 PeerId로 초대 (DHT 조회 → 직접 연결).
    InviteFriend { peer_id_bytes: Vec<u8> },

    // ── 파일 ─────────────────────────────────────────────────────────────────
    ShareFile { path: String },
    DownloadFileByNumber { number: u32 },
    StartDownload { file_hash: [u8; 32], file_name: String, chunk_count: u32 },
    /// 선택적 다운로드에서 여러 파일 동시 다운로드 (12-tui.md: 파일 선택 화면).
    StartDownloads { files: Vec<([ u8; 32], String, u32)> }, // (file_hash, file_name, chunk_count)
    PauseDownload { number: u32 },
    ResumeDownload { number: u32 },
    CancelDownload { number: u32 },
    MoveDownloadTop { number: u32 },
    MoveDownloadUp { number: u32 },
    MoveDownloadDown { number: u32 },
    SeedPause { number: u32 },
    SeedResume { number: u32 },
    RemoveSeed { number: u32, delete_file: bool },

    // ── 설정 ─────────────────────────────────────────────────────────────────
    ChangeNickname { new_nickname: String },
    ChangePassword { current: String, new_pw: String },

    // ── 친구 ─────────────────────────────────────────────────────────────────
    AddFriend { peer_id_bytes: Vec<u8> },
    RemoveFriend { peer_id_bytes: Vec<u8> },

    // ── 설정 ─────────────────────────────────────────────────────────────────
    /// 설정 화면 진입 시 현재 config 스냅샷을 요청한다.
    EnterSettings,
    /// 설정 항목 변경 (field: 필드명, value: 새 값 문자열).
    UpdateConfigField { field: String, value: String },

    // ── 목록 조회 (채팅 명령어) ───────────────────────────────────────────────
    ListFiles,
    ListDownloads,
    ListSeeds,

    // ── 방 상태 동기화 ───────────────────────────────────────────────────────
    /// 방 전체 상태 재동기화 (피어, 파일, bitfield 등)
    Refresh,

    // ── 시스템 ───────────────────────────────────────────────────────────────
    Shutdown,
}

// ── AppEvent (App → TUI) ──────────────────────────────────────────────────────

/// 앱 레이어에서 TUI/CLI로 보내는 이벤트.
#[derive(Debug)]
pub enum AppEvent {
    // ── 피드 항목 ─────────────────────────────────────────────────────────────
    FeedEntry(LogEntry),

    // ── 피어 상태 ─────────────────────────────────────────────────────────────
    PeerJoined { peer_id: PeerId, nickname: String },
    PeerLeft { peer_id: PeerId },
    PeerList { peers: Vec<(PeerId, String)> },

    // ── 방 이벤트 ─────────────────────────────────────────────────────────────
    RoomList { rooms: Vec<([u8; 32], String, Option<u32>)> }, // (room_id, name, peer_count)
    RoomExpired,
    JoinedRoom { room_id: [u8; 32], name: String },
    LeftRoom,

    // ── 파일 이벤트 ───────────────────────────────────────────────────────────
    FileAnnounced { announce: FileAnnounce },
    FileRemoved { file_hash: [u8; 32] },
    DownloadProgress {
        file_hash: [u8; 32],
        completed_chunks: u32,
        total_chunks: u32,
        status: DownloadStatus,
    },
    DownloadComplete { file_hash: [u8; 32], file_name: String },

    // ── 초대 이벤트 ───────────────────────────────────────────────────────────
    InviteCodeGenerated { code: String, my_id: String },
    InviteReceived { from_peer: PeerId, from_nickname: String, room_name: String, number: u32 },
    InviteDecision { accepted: bool, by_peer: PeerId },
    InviteExpired,
    /// URL DHT 조회 성공 — 방 목록 (room_id, identifier).
    UrlRooms { rooms: Vec<([u8; 32], String)> },
    /// URL DHT 조회 실패 — URL에 해당하는 방 없음.
    UrlNotFound,

    // ── mDNS 피어 목록 (인트라넷 초대용) ─────────────────────────────────────
    /// mDNS로 발견된 로컬 네트워크 피어 목록 갱신.
    MdnsPeersUpdated { peers: Vec<(PeerId, String)> }, // (PeerId, display addr)

    // ── 설정 스냅샷 ───────────────────────────────────────────────────────────
    ConfigSnapshot {
        user_id: String,
        nickname: String,
        network_mode: String,
        port: String,
        max_connections: String,
        download_path: String,
        max_concurrent_dl: String,
        max_upload_kbps: String,
        max_download_kbps: String,
        log_path: String,
        language: String,
    },

    // ── 방 피어 수 갱신 (배경 DHT 조회 완료) ─────────────────────────────────
    /// ListRooms 이후 백그라운드 DHT GetProviders 완료 시 개별 방 피어 수 갱신.
    RoomPeerCount { room_id: [u8; 32], count: u32 },

    // ── 오류 / 알림 ───────────────────────────────────────────────────────────
    Error(String),
    Notice(String),

    // ── 인증 결과 ─────────────────────────────────────────────────────────────
    LoginSuccess { nickname: String },
    LoginFailed(String),
    RegisterSuccess,
    RegisterFailed(String),
}

// ── 채널 타입 별칭 ────────────────────────────────────────────────────────────

pub type AppCommandTx = mpsc::Sender<AppCommand>;
pub type AppCommandRx = mpsc::Receiver<AppCommand>;
pub type AppEventTx = mpsc::Sender<AppEvent>;
pub type AppEventRx = mpsc::Receiver<AppEvent>;
pub type NetworkCommandTx = mpsc::Sender<NetworkCommand>;
