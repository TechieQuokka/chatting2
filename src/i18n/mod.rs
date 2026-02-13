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
    // ── 메뉴 ──────────────────────────────────────────────────────────────────
    Login,
    Register,
    DeleteAccount,
    MainMenu,
    RoomList,
    CreateRoom,
    FriendList,
    Settings,

    // ── 필드 ──────────────────────────────────────────────────────────────────
    Id,
    Password,
    PasswordConfirm,
    Nickname,
    RoomName,
    InviteCode,

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

    // ── 동작 ──────────────────────────────────────────────────────────────────
    Confirm,
    Cancel,
    Accept,
    Reject,
    Delete,
    Back,
    Save,

    // ── 오류 ──────────────────────────────────────────────────────────────────
    ErrorWrongPassword,
    ErrorRoomExpired,
    ErrorNotFound,
    ErrorInvalidCode,
    ErrorTooManyAttempts,

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

        Key::Id => "아이디",
        Key::Password => "비밀번호",
        Key::PasswordConfirm => "비밀번호 확인",
        Key::Nickname => "닉네임",
        Key::RoomName => "방 이름",
        Key::InviteCode => "초대 코드",

        Key::Checking => "확인 중...",
        Key::Online => "온라인",
        Key::Offline => "오프라인",
        Key::Expired => "만료됨",
        Key::Active => "활성",
        Key::Paused => "일시정지",
        Key::Waiting => "대기 중",
        Key::Completed => "완료",
        Key::Cancelled => "취소됨",

        Key::Confirm => "확인",
        Key::Cancel => "취소",
        Key::Accept => "수락",
        Key::Reject => "거절",
        Key::Delete => "삭제",
        Key::Back => "뒤로",
        Key::Save => "저장",

        Key::ErrorWrongPassword => "비밀번호가 틀렸습니다.",
        Key::ErrorRoomExpired => "방 수명이 만료되었습니다.",
        Key::ErrorNotFound => "항목을 찾을 수 없습니다.",
        Key::ErrorInvalidCode => "유효하지 않은 초대 코드입니다.",
        Key::ErrorTooManyAttempts => "입력 횟수를 초과했습니다.",

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

        Key::Id => "ID",
        Key::Password => "Password",
        Key::PasswordConfirm => "Confirm Password",
        Key::Nickname => "Nickname",
        Key::RoomName => "Room Name",
        Key::InviteCode => "Invite Code",

        Key::Checking => "Checking...",
        Key::Online => "Online",
        Key::Offline => "Offline",
        Key::Expired => "Expired",
        Key::Active => "Active",
        Key::Paused => "Paused",
        Key::Waiting => "Waiting",
        Key::Completed => "Completed",
        Key::Cancelled => "Cancelled",

        Key::Confirm => "Confirm",
        Key::Cancel => "Cancel",
        Key::Accept => "Accept",
        Key::Reject => "Reject",
        Key::Delete => "Delete",
        Key::Back => "Back",
        Key::Save => "Save",

        Key::ErrorWrongPassword => "Wrong password.",
        Key::ErrorRoomExpired => "Room has expired.",
        Key::ErrorNotFound => "Item not found.",
        Key::ErrorInvalidCode => "Invalid invite code.",
        Key::ErrorTooManyAttempts => "Too many attempts.",

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
    }
}
