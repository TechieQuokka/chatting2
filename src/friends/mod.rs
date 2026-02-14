use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::crypto::{load_enc, save_enc, EncFileError};

// ── FriendRecord ──────────────────────────────────────────────────────────────

/// 친구 정보.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRecord {
    /// 상대방 PeerId (바이트 표현).
    pub peer_id_bytes: Vec<u8>,
    /// 최신 닉네임 (ChatMessage 수신 시 자동 갱신).
    pub nickname: String,
    /// 최초 연결 시각 (Unix ms).
    pub connected_at_ms: u64,
}

impl FriendRecord {
    pub fn new(peer_id_bytes: Vec<u8>, nickname: String) -> Self {
        Self {
            peer_id_bytes,
            nickname,
            connected_at_ms: now_ms(),
        }
    }

    /// 닉네임#코드 표시 형식. `#코드`는 peer_id_bytes 앞 4바이트 헥스.
    pub fn display_name(&self) -> String {
        let code: String = self.peer_id_bytes.iter().take(4).map(|b| format!("{b:02x}")).collect();
        format!("{}#{}", self.nickname, &code[..code.len().min(8)])
    }
}

// ── 에러 ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum FriendStoreError {
    Io(std::io::Error),
    EncFile(EncFileError),
    Serialize(serde_json::Error),
    AlreadyFriend,
    NotFound,
}

impl std::fmt::Display for FriendStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::EncFile(e) => write!(f, "enc: {e}"),
            Self::Serialize(e) => write!(f, "serialize: {e}"),
            Self::AlreadyFriend => write!(f, "이미 친구입니다"),
            Self::NotFound => write!(f, "친구를 찾을 수 없습니다"),
        }
    }
}

impl std::error::Error for FriendStoreError {}

impl From<EncFileError> for FriendStoreError {
    fn from(e: EncFileError) -> Self { Self::EncFile(e) }
}
impl From<serde_json::Error> for FriendStoreError {
    fn from(e: serde_json::Error) -> Self { Self::Serialize(e) }
}

// ── FriendStore ───────────────────────────────────────────────────────────────

/// 친구 목록. `friends.enc`에 저장.
pub struct FriendStore {
    path: PathBuf,
    friends: Vec<FriendRecord>,
}

impl FriendStore {
    /// 빈 친구 목록 생성 (파일이 없는 초기 상태).
    pub fn new(path: PathBuf) -> Self {
        Self { path, friends: Vec::new() }
    }

    /// `friends.enc` 로드. 없으면 빈 목록.
    pub fn load(path: &Path, key: &[u8; 32]) -> Result<Self, FriendStoreError> {
        if !path.exists() {
            return Ok(Self { path: path.to_path_buf(), friends: Vec::new() });
        }
        let plain = load_enc(path, key)?;
        let friends: Vec<FriendRecord> = serde_json::from_slice(&plain)?;
        Ok(Self { path: path.to_path_buf(), friends })
    }

    /// 현재 목록 저장.
    pub fn save(&self, key: &[u8; 32]) -> Result<(), FriendStoreError> {
        let plain = serde_json::to_vec(&self.friends)?;
        save_enc(&self.path, key, &plain).map_err(FriendStoreError::EncFile)
    }

    pub fn all(&self) -> &[FriendRecord] {
        &self.friends
    }

    /// 친구 추가.
    pub fn add(&mut self, record: FriendRecord) -> Result<(), FriendStoreError> {
        if self.friends.iter().any(|f| f.peer_id_bytes == record.peer_id_bytes) {
            return Err(FriendStoreError::AlreadyFriend);
        }
        self.friends.push(record);
        Ok(())
    }

    /// 친구 삭제 (peer_id_bytes 기준).
    pub fn remove(&mut self, peer_id_bytes: &[u8]) -> Result<(), FriendStoreError> {
        let before = self.friends.len();
        self.friends.retain(|f| f.peer_id_bytes.as_slice() != peer_id_bytes);
        if self.friends.len() == before {
            return Err(FriendStoreError::NotFound);
        }
        Ok(())
    }

    /// ChatMessage 수신 시 닉네임 자동 갱신.
    pub fn update_nickname(&mut self, peer_id_bytes: &[u8], new_nickname: &str) {
        if let Some(f) = self.friends.iter_mut().find(|f| f.peer_id_bytes.as_slice() == peer_id_bytes) {
            f.nickname = new_nickname.to_string();
        }
    }

    /// PeerId 바이트로 친구 찾기.
    pub fn find(&self, peer_id_bytes: &[u8]) -> Option<&FriendRecord> {
        self.friends.iter().find(|f| f.peer_id_bytes.as_slice() == peer_id_bytes)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
