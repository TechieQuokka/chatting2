use libp2p::PeerId;

use crate::room::RoomLifetime;
use crate::transfer::DownloadStatus;

// ── 화면 정의 ────────────────────────────────────────────────────────────────

/// TUI 현재 화면 상태.
#[derive(Debug)]
pub enum Screen {
    Login(LoginState),
    Register(RegisterState),
    DeleteAccount(DeleteAccountState),
    MainMenu(MainMenuState),
    RoomList(RoomListState),
    CreateRoom(CreateRoomState),
    InviteEntry(InviteEntryState),
    FriendList(FriendListState),
    Settings(SettingsState),
    FileSelect(FileSelectState),
    Chat(ChatState),
}

// ── 로그인 화면 ───────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct LoginState {
    pub id_input: String,
    pub pw_input: String,
    pub focused: LoginField,
    pub error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum LoginField {
    #[default]
    Id,
    Pw,
}

// ── 계정 등록 화면 ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct RegisterState {
    pub id_input: String,
    pub pw_input: String,
    pub pw_confirm: String,
    pub focused: RegisterField,
    pub error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum RegisterField {
    #[default]
    Id,
    Pw,
    PwConfirm,
}

// ── 계정 삭제 화면 ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct DeleteAccountState {
    pub id: String,
    pub confirmed: bool,
    pub error: Option<String>,
}

// ── 메인 메뉴 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct MainMenuState {
    pub nickname: String,
    pub pending_invites: Vec<PendingInviteInfo>,
    pub show_invite_overlay: bool,
    pub invite_cursor: usize,
}

#[derive(Debug, Clone)]
pub struct PendingInviteInfo {
    pub from_peer: PeerId,
    pub from_display: String,
    pub room_name: String,
    pub number: u32,
}

// ── 방 목록 화면 ─────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct RoomListState {
    pub rooms: Vec<RoomListEntry>,
    pub cursor: usize,
    pub expired_cleaned: usize,
    pub confirm_delete: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct RoomListEntry {
    pub room_id: [u8; 32],
    pub name: String,
    pub peer_status: PeerStatus,
    pub lifetime_display: String,
}

#[derive(Debug, Clone)]
pub enum PeerStatus {
    Checking,
    Online(u32),
    Offline,
    Expired,
}

// ── 방 만들기 화면 ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct CreateRoomState {
    pub name_input: String,
    pub lifetime: RoomLifetime,
    pub focused: CreateRoomField,
    pub error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum CreateRoomField {
    #[default]
    Name,
    Lifetime,
}

impl Default for RoomLifetime {
    fn default() -> Self { RoomLifetime::OneDay }
}

// ── 초대 코드 입장 화면 ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct InviteEntryState {
    pub step: InviteStep,
    pub url_input: String,
    pub room_candidates: Vec<([u8; 32], String)>, // (room_id, identifier)
    pub room_cursor: usize,
    pub code_input: String,
    pub selected_room: Option<[u8; 32]>,
    pub ttl_remaining_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum InviteStep {
    #[default]
    UrlInput,
    RoomSelect,
    CodeInput,
    Waiting,
    Failed(String),
}

// ── 친구 목록 화면 ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct FriendListState {
    pub friends: Vec<FriendDisplay>,
    pub cursor: usize,
    pub confirm_delete: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct FriendDisplay {
    pub peer_id_bytes: Vec<u8>,
    pub display_name: String,
    pub connected_date: String,
}

// ── 설정 화면 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct SettingsState {
    pub category: SettingsCategory,
    pub cursor: usize,
    pub editing: bool,
    pub edit_input: String,
    pub error: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum SettingsCategory {
    #[default]
    Select,
    Profile,
    Network,
    Chat,
    File,
    RoomManage,
    FriendManage,
    Language,
}

// ── 파일 선택 화면 (선택적 다운로드) ─────────────────────────────────────────

#[derive(Debug, Default)]
pub struct FileSelectState {
    pub file_hash: [u8; 32],
    pub folder_name: String,
    pub items: Vec<FileSelectItem>,
    pub cursor: usize,
    pub total_size: u64,
    pub selected_size: u64,
}

#[derive(Debug, Clone)]
pub struct FileSelectItem {
    pub name: String,
    pub size: u64,
    pub file_hash: [u8; 32],
    pub selected: bool,
    pub depth: usize,
    pub is_dir: bool,
}

// ── 채팅/파일 화면 ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ChatState {
    pub room_id: [u8; 32],
    pub room_name: String,
    pub peer_count: u32,
    pub upload_bps: u64,
    pub download_bps: u64,
    pub last_sync_ms: Option<u64>,
    pub expired: bool,

    /// 통합 피드 항목.
    pub feed: Vec<FeedItem>,
    pub feed_scroll: usize,

    /// 입력창.
    pub input: String,
    pub input_disabled: bool,

    /// 활성 다운로드 요약 (최대 3개 표시).
    pub active_downloads: Vec<DownloadSummary>,

    /// 피드 내 대기 중인 초대 목록.
    pub pending_invites: Vec<PendingInviteInfo>,
}

/// 피드에 표시되는 항목.
#[derive(Debug, Clone)]
pub struct FeedItem {
    pub timestamp_ms: u64,
    pub content: FeedContent,
}

#[derive(Debug, Clone)]
pub enum FeedContent {
    Chat { peer_display: String, text: String },
    FileEvent(String),
    System(String),
    Invite(String),
    Command(String),
}

/// 상단 다운로드 요약 바.
#[derive(Debug, Clone)]
pub struct DownloadSummary {
    pub file_name: String,
    pub pct: f32,
    pub bps: u64,
    pub status: DownloadStatus,
}
