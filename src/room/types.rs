use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::protocol::gossip::FileEntry;

// ── RoomLifetime ─────────────────────────────────────────────────────────────

/// 방 수명.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomLifetime {
    OneDay,
    ThreeDays,
    SevenDays,
    /// 수동 삭제 전까지 유지.
    Unlimited,
}

impl RoomLifetime {
    /// 수명을 밀리초 단위로 반환. `Unlimited`는 `None`.
    pub fn as_millis(&self) -> Option<u64> {
        match self {
            Self::OneDay => Some(24 * 60 * 60 * 1_000),
            Self::ThreeDays => Some(3 * 24 * 60 * 60 * 1_000),
            Self::SevenDays => Some(7 * 24 * 60 * 60 * 1_000),
            Self::Unlimited => None,
        }
    }

    pub fn default_lifetime() -> Self {
        Self::OneDay
    }
}

// ── RoomKey ──────────────────────────────────────────────────────────────────

/// 방 키 (AES-256 키, 32바이트). 메모리 해제 시 0 덮어쓰기.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RoomKey(pub [u8; 32]);

impl std::fmt::Debug for RoomKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoomKey([REDACTED])")
    }
}

// ── RoomRecord ───────────────────────────────────────────────────────────────

/// rooms.enc에 저장되는 방 정보.
#[derive(Debug, Clone)]
pub struct RoomRecord {
    /// 랜덤 고유 ID (32바이트). DHT content ID로 사용.
    pub room_id: [u8; 32],
    /// 방 이름 (도메인 스타일, e.g. `dev.team`).
    pub name: String,
    /// 방 키 (AES-256).
    pub key: RoomKey,
    /// 방 생성 시각 (Unix ms).
    pub created_at_ms: u64,
    /// 방 수명 설정.
    pub lifetime: RoomLifetime,
    /// 방의 파일 메타데이터 목록 (오프라인 입장 시 표시용).
    pub files: Vec<FileEntry>,
    /// 마지막 동기화 시각 (Unix ms). `None`이면 동기화 이력 없음.
    pub last_sync_ms: Option<u64>,
}

impl RoomRecord {
    /// 방 수명이 만료됐는지 확인 (로컬 시각 기준).
    pub fn is_expired(&self, now_ms: u64) -> bool {
        match self.lifetime.as_millis() {
            Some(duration) => now_ms.saturating_sub(self.created_at_ms) >= duration,
            None => false,
        }
    }
}

// ── serde 직렬화 형식 (rooms.enc 내부) ───────────────────────────────────────

/// `RoomRecord`의 직렬화 전용 중간 타입.
///
/// `RoomKey`는 Serialize를 구현하지 않으므로 bytes로 직렬화.
#[derive(Serialize, Deserialize)]
pub(super) struct SerialRoom {
    pub room_id: [u8; 32],
    pub name: String,
    pub key_bytes: [u8; 32],
    pub created_at_ms: u64,
    pub lifetime: RoomLifetime,
    pub files: Vec<FileEntry>,
    pub last_sync_ms: Option<u64>,
}

impl From<&RoomRecord> for SerialRoom {
    fn from(r: &RoomRecord) -> Self {
        Self {
            room_id: r.room_id,
            name: r.name.clone(),
            key_bytes: r.key.0,
            created_at_ms: r.created_at_ms,
            lifetime: r.lifetime,
            files: r.files.clone(),
            last_sync_ms: r.last_sync_ms,
        }
    }
}

impl From<SerialRoom> for RoomRecord {
    fn from(s: SerialRoom) -> Self {
        Self {
            room_id: s.room_id,
            name: s.name,
            key: RoomKey(s.key_bytes),
            created_at_ms: s.created_at_ms,
            lifetime: s.lifetime,
            files: s.files,
            last_sync_ms: s.last_sync_ms,
        }
    }
}

// ── RoomName 유효성 검사 ──────────────────────────────────────────────────────

/// 방 이름 유효성 검사.
///
/// 도메인 스타일: `dev.team`, `project-x.secret`
/// - 허용 문자: 영문, 숫자, `-`, `.`
/// - 전체 길이: 5~32자
/// - `.` 앞: 최소 2자, `.` 뒤: 최소 2자
/// - 최소 1개의 `.` 필수
pub fn validate_room_name(name: &str) -> Result<(), RoomNameError> {
    let len = name.len();
    if len < 5 || len > 32 {
        return Err(RoomNameError::InvalidLength);
    }

    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        return Err(RoomNameError::InvalidChar);
    }

    // 첫 번째 `.` 위치 기준으로 앞/뒤 길이 확인 (마지막 `.` 기준이 아닌 첫 `.` 기준)
    // 예) `a.b.cd` → 앞=1(너무 짧음), 뒤=`b.cd`=4
    // 실제로는 첫 `.` 앞 = 이름 파트, 마지막 `.` 뒤 = 접미사 파트로 해석
    let dot_pos = name.rfind('.').ok_or(RoomNameError::NoDot)?;
    let prefix = &name[..dot_pos];
    let suffix = &name[dot_pos + 1..];

    if prefix.len() < 2 {
        return Err(RoomNameError::PrefixTooShort);
    }
    if suffix.len() < 2 {
        return Err(RoomNameError::SuffixTooShort);
    }

    // `-`나 `.`으로 시작하거나 끝나는 경우 불허
    if name.starts_with('-') || name.starts_with('.') || name.ends_with('-') || name.ends_with('.') {
        return Err(RoomNameError::InvalidChar);
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomNameError {
    InvalidLength,
    InvalidChar,
    NoDot,
    PrefixTooShort,
    SuffixTooShort,
}

impl std::fmt::Display for RoomNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "방 이름은 5~32자여야 합니다"),
            Self::InvalidChar => write!(f, "허용 문자: 영문, 숫자, -, ."),
            Self::NoDot => write!(f, "방 이름에 '.'이 하나 이상 있어야 합니다"),
            Self::PrefixTooShort => write!(f, "'.' 앞 이름은 최소 2자여야 합니다"),
            Self::SuffixTooShort => write!(f, "'.' 뒤 접미사는 최소 2자여야 합니다"),
        }
    }
}

impl std::error::Error for RoomNameError {}
