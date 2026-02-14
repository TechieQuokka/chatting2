//! 국제화 (i18n) 지원.
//!
//! 지원 언어: 한국어(기본), 영어.
//!
//! ## 사용법
//!
//! ```
//! use crate::i18n::{Lang, t};
//!
//! let lang = Lang::Korean;
//! println!("{}", t(lang, Key::Login));
//! ```
//!
//! ## 변경 즉시 적용
//!
//! `Lang`은 `Config`에 저장되므로 설정 화면에서 변경 즉시
//! 다음 프레임 렌더링부터 새 언어로 표시된다.

use serde::{Deserialize, Serialize};

// ── 언어 열거형 ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Korean,
    English,
}

impl Lang {
    pub fn display_name(&self) -> &'static str {
        match self {
            Lang::Korean => "한국어",
            Lang::English => "English",
        }
    }
}

// ── 번역 키 ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // ── 화면 제목 / 메뉴 ──────────────────────────────────────────────────────
    Login,
    Register,
    DeleteAccount,
    MainMenu,
    RoomList,
    CreateRoom,
    FriendList,
    Settings,
    Quit,
    JoinByInvite,

    // ── 필드 ──────────────────────────────────────────────────────────────────
    Id,
    Password,
    PasswordConfirm,
    PasswordChange,
    Nickname,
    RoomName,
    InviteCode,
    NetworkMode,
    Port,
    MaxConnections,
    LogPath,
    DownloadPath,
    MaxConcurrentDownloads,
    UploadSpeedLimit,
    DownloadSpeedLimit,
    Language,

    // ── 상태 ──────────────────────────────────────────────────────────────────
    Checking,
    Online,
    Offline,
    Expired,
    Active,
    Paused,
    Waiting,
    Completed,
    Cancelled,
    Unlimited,
    NoSync,
    DaysAgo,

    // ── 동작 ──────────────────────────────────────────────────────────────────
    Confirm,
    Cancel,
    Accept,
    Reject,
    Delete,
    Back,
    Save,
    Edit,
    Toggle,

    // ── 오류 ──────────────────────────────────────────────────────────────────
    ErrorWrongPassword,
    ErrorRoomExpired,
    ErrorNotFound,
    ErrorInvalidCode,
    ErrorTooManyAttempts,

    // ── 힌트 텍스트 (하단 안내) ───────────────────────────────────────────────
    HintMoveSelectBack,          // ↑↓ 이동  Enter 선택  Esc 뒤로
    HintMoveJoinDeleteBack,      // ↑↓ 이동  Enter 입장  D 삭제  Esc 뒤로
    HintMoveDeleteBack,          // ↑↓ 이동  D 삭제  Esc 뒤로
    HintMoveEditBack,            // ↑↓ 이동  Enter 편집  Esc 뒤로
    HintMoveToggleBack,          // ↑↓ 이동  Enter/Space 변경  Esc 뒤로
    HintEditConfirmCancel,       // 입력 후 Enter 확인  Esc 취소
    HintEscBack,                 // Esc 뒤로
    HintToggleBack,              // Enter/Space 토글  Esc 뒤로
    HintTabNextEnterRegisterEscCancel, // Tab 다음 항목  Enter 등록  Esc 취소
    HintTabMoveEnterDeleteConfirm,     // Tab 이동  Enter 삭제 확인  Esc 취소
    HintTabFocusEnterLoginEscBack,     // Tab 포커스 이동  Enter 로그인  Esc 뒤로
    HintAcceptRejectLater,       // ↑↓ 이동  Enter 수락  D 거절  Esc 나중에
    HintSpaceSelectAAllEnterStartEscCancel, // ↑↓ 이동  Space 선택  A 전체  Enter 시작  Esc 취소
    HintCurrentPwEnterNext,      // 현재 비밀번호 입력  Enter 다음  Esc 취소
    HintNewPwEnterChange,        // 새 비밀번호 입력  Enter 변경  Esc 취소
    HintEnterCreateEscCancel,    // Enter 생성  Esc 취소

    // ── 안내 ──────────────────────────────────────────────────────────────────
    HelpDownload,
    HelpShare,
    HelpInvite,
    PeersLabel,
    UploadLabel,
    DownloadLabel,
    LastSyncLabel,
    RoomLifetimeOneDay,
    RoomLifetimeThreeDays,
    RoomLifetimeSevenDays,
    RoomLifetimeUnlimited,

    // ── 설정 카테고리 ─────────────────────────────────────────────────────────
    CatProfile,
    CatNetwork,
    CatChat,
    CatFile,
    CatRoomManage,
    CatFriendManage,
    CatLanguage,

    // ── 기타 ──────────────────────────────────────────────────────────────────
    ReadOnly,           // (변경 불가) / (read-only)
    SpeedUnit,          // KB/s
    SpeedUnlimited,     // 무제한 / Unlimited
}

// ── 번역 테이블 ───────────────────────────────────────────────────────────────

/// 주어진 언어로 키를 번역한다.
pub fn t(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::Korean => ko(key),
        Lang::English => en(key),
    }
}

fn ko(key: Key) -> &'static str {
    match key {
        Key::Login => "로그인",
        Key::Register => "계정 등록",
        Key::DeleteAccount => "계정 삭제",
        Key::MainMenu => "메인 메뉴",
        Key::RoomList => "방 목록",
        Key::CreateRoom => "방 만들기",
        Key::FriendList => "친구 목록",
        Key::Settings => "설정",
        Key::Quit => "종료",
        Key::JoinByInvite => "초대 코드로 입장",

        Key::Id => "아이디",
        Key::Password => "비밀번호",
        Key::PasswordConfirm => "비밀번호 확인",
        Key::PasswordChange => "비밀번호 변경",
        Key::Nickname => "닉네임",
        Key::RoomName => "방 이름",
        Key::InviteCode => "초대 코드",
        Key::NetworkMode => "네트워크 모드",
        Key::Port => "포트",
        Key::MaxConnections => "최대 연결 수",
        Key::LogPath => "로그 저장 경로",
        Key::DownloadPath => "다운로드 경로",
        Key::MaxConcurrentDownloads => "최대 동시 다운로드",
        Key::UploadSpeedLimit => "업로드 속도 제한",
        Key::DownloadSpeedLimit => "다운로드 속도 제한",
        Key::Language => "언어 / Language",

        Key::Checking => "확인 중...",
        Key::Online => "온라인",
        Key::Offline => "오프라인",
        Key::Expired => "만료됨",
        Key::Active => "활성",
        Key::Paused => "일시정지",
        Key::Waiting => "대기 중",
        Key::Completed => "완료",
        Key::Cancelled => "취소됨",
        Key::Unlimited => "무제한",
        Key::NoSync => "동기화 없음",
        Key::DaysAgo => "일 전",

        Key::Confirm => "확인",
        Key::Cancel => "취소",
        Key::Accept => "수락",
        Key::Reject => "거절",
        Key::Delete => "삭제",
        Key::Back => "뒤로",
        Key::Save => "저장",
        Key::Edit => "편집",
        Key::Toggle => "전환",

        Key::ErrorWrongPassword => "비밀번호가 틀렸습니다.",
        Key::ErrorRoomExpired => "방 수명이 만료되었습니다.",
        Key::ErrorNotFound => "항목을 찾을 수 없습니다.",
        Key::ErrorInvalidCode => "유효하지 않은 초대 코드입니다.",
        Key::ErrorTooManyAttempts => "입력 횟수를 초과했습니다.",

        Key::HintMoveSelectBack => "↑↓ 이동  Enter 선택  Esc 뒤로",
        Key::HintMoveJoinDeleteBack => "↑↓ 이동  Enter 입장  D 삭제  Esc 뒤로",
        Key::HintMoveDeleteBack => "↑↓ 이동  D 삭제  Esc 뒤로",
        Key::HintMoveEditBack => "↑↓ 이동  Enter 편집  Esc 뒤로",
        Key::HintMoveToggleBack => "↑↓ 이동  Enter/Space 변경  Esc 뒤로",
        Key::HintEditConfirmCancel => "입력 후 Enter 확인  Esc 취소",
        Key::HintEscBack => "Esc 뒤로",
        Key::HintToggleBack => "Enter/Space 토글  Esc 뒤로",
        Key::HintTabNextEnterRegisterEscCancel => "Tab 다음 항목  Enter 등록  Esc 취소",
        Key::HintTabMoveEnterDeleteConfirm => "Tab 이동  Enter 삭제 확인  Esc 취소",
        Key::HintTabFocusEnterLoginEscBack => "Tab 포커스 이동  Enter 로그인  Esc 뒤로",
        Key::HintAcceptRejectLater => "↑↓ 이동  Enter 수락  D 거절  Esc 나중에",
        Key::HintSpaceSelectAAllEnterStartEscCancel => "↑↓ 이동  Space 선택  A 전체  Enter 시작  Esc 취소",
        Key::HintCurrentPwEnterNext => "현재 비밀번호 입력  Enter 다음  Esc 취소",
        Key::HintNewPwEnterChange => "새 비밀번호 입력  Enter 변경  Esc 취소",
        Key::HintEnterCreateEscCancel => "Enter 생성  Esc 취소",

        Key::HelpDownload => "/download <번호> — 다운로드 시작",
        Key::HelpShare => "/share <경로> — 파일/폴더 공유",
        Key::HelpInvite => "/invite — 초대 코드 생성",
        Key::PeersLabel => "피어",
        Key::UploadLabel => "업",
        Key::DownloadLabel => "다운",
        Key::LastSyncLabel => "마지막 동기화",
        Key::RoomLifetimeOneDay => "1일",
        Key::RoomLifetimeThreeDays => "3일",
        Key::RoomLifetimeSevenDays => "7일",
        Key::RoomLifetimeUnlimited => "무제한",

        Key::CatProfile => "프로필",
        Key::CatNetwork => "네트워크",
        Key::CatChat => "채팅",
        Key::CatFile => "파일",
        Key::CatRoomManage => "방 관리",
        Key::CatFriendManage => "친구 관리",
        Key::CatLanguage => "언어",

        Key::ReadOnly => "(변경 불가)",
        Key::SpeedUnit => "KB/s",
        Key::SpeedUnlimited => "무제한",
    }
}

fn en(key: Key) -> &'static str {
    match key {
        Key::Login => "Login",
        Key::Register => "Register",
        Key::DeleteAccount => "Delete Account",
        Key::MainMenu => "Main Menu",
        Key::RoomList => "Rooms",
        Key::CreateRoom => "Create Room",
        Key::FriendList => "Friends",
        Key::Settings => "Settings",
        Key::Quit => "Quit",
        Key::JoinByInvite => "Join by Invite Code",

        Key::Id => "ID",
        Key::Password => "Password",
        Key::PasswordConfirm => "Confirm Password",
        Key::PasswordChange => "Change Password",
        Key::Nickname => "Nickname",
        Key::RoomName => "Room Name",
        Key::InviteCode => "Invite Code",
        Key::NetworkMode => "Network Mode",
        Key::Port => "Port",
        Key::MaxConnections => "Max Connections",
        Key::LogPath => "Log Path",
        Key::DownloadPath => "Download Path",
        Key::MaxConcurrentDownloads => "Max Concurrent Downloads",
        Key::UploadSpeedLimit => "Upload Speed Limit",
        Key::DownloadSpeedLimit => "Download Speed Limit",
        Key::Language => "Language",

        Key::Checking => "Checking...",
        Key::Online => "Online",
        Key::Offline => "Offline",
        Key::Expired => "Expired",
        Key::Active => "Active",
        Key::Paused => "Paused",
        Key::Waiting => "Waiting",
        Key::Completed => "Completed",
        Key::Cancelled => "Cancelled",
        Key::Unlimited => "Unlimited",
        Key::NoSync => "never synced",
        Key::DaysAgo => "days ago",

        Key::Confirm => "Confirm",
        Key::Cancel => "Cancel",
        Key::Accept => "Accept",
        Key::Reject => "Reject",
        Key::Delete => "Delete",
        Key::Back => "Back",
        Key::Save => "Save",
        Key::Edit => "Edit",
        Key::Toggle => "Toggle",

        Key::ErrorWrongPassword => "Wrong password.",
        Key::ErrorRoomExpired => "Room has expired.",
        Key::ErrorNotFound => "Item not found.",
        Key::ErrorInvalidCode => "Invalid invite code.",
        Key::ErrorTooManyAttempts => "Too many attempts.",

        Key::HintMoveSelectBack => "↑↓ Move  Enter Select  Esc Back",
        Key::HintMoveJoinDeleteBack => "↑↓ Move  Enter Join  D Delete  Esc Back",
        Key::HintMoveDeleteBack => "↑↓ Move  D Delete  Esc Back",
        Key::HintMoveEditBack => "↑↓ Move  Enter Edit  Esc Back",
        Key::HintMoveToggleBack => "↑↓ Move  Enter/Space Toggle  Esc Back",
        Key::HintEditConfirmCancel => "Enter Confirm  Esc Cancel",
        Key::HintEscBack => "Esc Back",
        Key::HintToggleBack => "Enter/Space Toggle  Esc Back",
        Key::HintTabNextEnterRegisterEscCancel => "Tab Next  Enter Register  Esc Cancel",
        Key::HintTabMoveEnterDeleteConfirm => "Tab Move  Enter Confirm Delete  Esc Cancel",
        Key::HintTabFocusEnterLoginEscBack => "Tab Focus  Enter Login  Esc Back",
        Key::HintAcceptRejectLater => "↑↓ Move  Enter Accept  D Reject  Esc Later",
        Key::HintSpaceSelectAAllEnterStartEscCancel => "↑↓ Move  Space Select  A All  Enter Start  Esc Cancel",
        Key::HintCurrentPwEnterNext => "Enter current password  Enter Next  Esc Cancel",
        Key::HintNewPwEnterChange => "Enter new password  Enter Change  Esc Cancel",
        Key::HintEnterCreateEscCancel => "Enter Create  Esc Cancel",

        Key::HelpDownload => "/download <n> — start download",
        Key::HelpShare => "/share <path> — share file/folder",
        Key::HelpInvite => "/invite — generate invite code",
        Key::PeersLabel => "peers",
        Key::UploadLabel => "up",
        Key::DownloadLabel => "dn",
        Key::LastSyncLabel => "last sync",
        Key::RoomLifetimeOneDay => "1 day",
        Key::RoomLifetimeThreeDays => "3 days",
        Key::RoomLifetimeSevenDays => "7 days",
        Key::RoomLifetimeUnlimited => "Unlimited",

        Key::CatProfile => "Profile",
        Key::CatNetwork => "Network",
        Key::CatChat => "Chat",
        Key::CatFile => "File",
        Key::CatRoomManage => "Room Management",
        Key::CatFriendManage => "Friend Management",
        Key::CatLanguage => "Language",

        Key::ReadOnly => "(read-only)",
        Key::SpeedUnit => "KB/s",
        Key::SpeedUnlimited => "Unlimited",
    }
}
