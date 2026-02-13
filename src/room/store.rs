use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{load_enc, save_enc, EncFileError};
use crate::protocol::gossip::FileEntry;

use super::types::{RoomKey, RoomLifetime, RoomRecord, SerialRoom, validate_room_name, RoomNameError};

// ── 에러 ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RoomStoreError {
    Io(std::io::Error),
    EncFile(EncFileError),
    Serialize(serde_json::Error),
    InvalidName(RoomNameError),
    RoomNotFound,
}

impl std::fmt::Display for RoomStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::EncFile(e) => write!(f, "enc file: {e}"),
            Self::Serialize(e) => write!(f, "serialize: {e}"),
            Self::InvalidName(e) => write!(f, "invalid room name: {e}"),
            Self::RoomNotFound => write!(f, "room not found"),
        }
    }
}

impl std::error::Error for RoomStoreError {}

impl From<std::io::Error> for RoomStoreError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}
impl From<EncFileError> for RoomStoreError {
    fn from(e: EncFileError) -> Self { Self::EncFile(e) }
}
impl From<serde_json::Error> for RoomStoreError {
    fn from(e: serde_json::Error) -> Self { Self::Serialize(e) }
}
impl From<RoomNameError> for RoomStoreError {
    fn from(e: RoomNameError) -> Self { Self::InvalidName(e) }
}

// ── RoomStore ────────────────────────────────────────────────────────────────

/// 방 정보 저장소. rooms.enc 파일을 읽고 쓴다.
pub struct RoomStore {
    path: PathBuf,
    rooms: Vec<RoomRecord>,
}

impl RoomStore {
    /// rooms.enc를 복호화해 로드. 파일이 없으면 빈 저장소 반환.
    pub fn load(path: &Path, key: &[u8; 32]) -> Result<Self, RoomStoreError> {
        if !path.exists() {
            return Ok(Self { path: path.to_path_buf(), rooms: Vec::new() });
        }
        let plain = load_enc(path, key)?;
        let serial: Vec<SerialRoom> = serde_json::from_slice(&plain)?;
        let rooms = serial.into_iter().map(RoomRecord::from).collect();
        Ok(Self { path: path.to_path_buf(), rooms })
    }

    /// 현재 방 목록을 rooms.enc로 저장 (원자적 쓰기).
    pub fn save(&self, key: &[u8; 32]) -> Result<(), RoomStoreError> {
        let serial: Vec<SerialRoom> = self.rooms.iter().map(SerialRoom::from).collect();
        let plain = serde_json::to_vec(&serial)?;
        save_enc(&self.path, key, &plain)?;
        Ok(())
    }

    /// 현재 Unix 타임스탬프 (밀리초).
    pub fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    // ── 방 생성 ───────────────────────────────────────────────────────────────

    /// 새 방을 생성하고 저장소에 추가한다.
    ///
    /// - 방 이름 유효성 검사
    /// - 랜덤 room_id (32바이트) 및 room_key (32바이트) 생성
    /// - 현재 시각 기록
    ///
    /// DHT 등록은 호출자(app 레이어)가 담당.
    pub fn create_room(
        &mut self,
        name: &str,
        lifetime: RoomLifetime,
    ) -> Result<&RoomRecord, RoomStoreError> {
        validate_room_name(name)?;

        let mut room_id = [0u8; 32];
        let mut room_key = [0u8; 32];
        getrandom::fill(&mut room_id).expect("getrandom failed");
        getrandom::fill(&mut room_key).expect("getrandom failed");

        let record = RoomRecord {
            room_id,
            name: name.to_string(),
            key: RoomKey(room_key),
            created_at_ms: Self::now_ms(),
            lifetime,
            files: Vec::new(),
            last_sync_ms: None,
        };

        // room_key 바이트 즉시 zeroize (RoomKey로 이동됐으므로 스택 잔재만 처리)
        zeroize::Zeroize::zeroize(&mut room_key);

        self.rooms.push(record);
        Ok(self.rooms.last().unwrap())
    }

    // ── 조회 ─────────────────────────────────────────────────────────────────

    pub fn get(&self, room_id: &[u8; 32]) -> Option<&RoomRecord> {
        self.rooms.iter().find(|r| &r.room_id == room_id)
    }

    pub fn get_mut(&mut self, room_id: &[u8; 32]) -> Option<&mut RoomRecord> {
        self.rooms.iter_mut().find(|r| &r.room_id == room_id)
    }

    pub fn all(&self) -> &[RoomRecord] {
        &self.rooms
    }

    // ── 방 삭제 ───────────────────────────────────────────────────────────────

    /// 방을 수동 삭제. 방 키 포함 모든 데이터 제거.
    ///
    /// 채팅 로그(`logs/`)는 별도 보존 (이 메서드에서 삭제하지 않음).
    pub fn remove(&mut self, room_id: &[u8; 32]) -> Result<(), RoomStoreError> {
        let pos = self.rooms.iter().position(|r| &r.room_id == room_id)
            .ok_or(RoomStoreError::RoomNotFound)?;
        self.rooms.remove(pos); // Drop 시 RoomKey::drop()에서 key 자동 zeroize
        Ok(())
    }

    /// 만료된 방을 모두 삭제하고, 삭제된 방 수를 반환.
    ///
    /// 앱 실행 시 / 방 목록 진입 시 호출.
    pub fn remove_expired(&mut self) -> usize {
        let now = Self::now_ms();
        let before = self.rooms.len();
        self.rooms.retain(|r| !r.is_expired(now));
        before - self.rooms.len()
    }

    // ── 파일 메타데이터 갱신 ──────────────────────────────────────────────────

    /// 방의 파일 목록을 갱신하고 마지막 동기화 시각을 기록.
    pub fn update_files(
        &mut self,
        room_id: &[u8; 32],
        files: Vec<FileEntry>,
    ) -> Result<(), RoomStoreError> {
        let room = self.get_mut(room_id).ok_or(RoomStoreError::RoomNotFound)?;
        room.files = files;
        room.last_sync_ms = Some(Self::now_ms());
        Ok(())
    }

    /// FileAnnounce 수신 시 개별 파일 추가 (중복 시 갱신).
    pub fn upsert_file(&mut self, room_id: &[u8; 32], file: FileEntry) -> Result<(), RoomStoreError> {
        let room = self.get_mut(room_id).ok_or(RoomStoreError::RoomNotFound)?;
        if let Some(existing) = room.files.iter_mut().find(|f| f.file_hash == file.file_hash) {
            *existing = file;
        } else {
            room.files.push(file);
        }
        Ok(())
    }

    /// FileRemove 수신 시 해당 파일 제거.
    pub fn remove_file(&mut self, room_id: &[u8; 32], file_hash: &[u8; 32]) -> Result<(), RoomStoreError> {
        let room = self.get_mut(room_id).ok_or(RoomStoreError::RoomNotFound)?;
        room.files.retain(|f| &f.file_hash != file_hash);
        Ok(())
    }

    // ── 만료 여부 확인 ────────────────────────────────────────────────────────

    /// 특정 방의 만료 여부를 현재 시각 기준으로 확인.
    pub fn is_expired(&self, room_id: &[u8; 32]) -> bool {
        let now = Self::now_ms();
        self.rooms.iter()
            .find(|r| &r.room_id == room_id)
            .map(|r| r.is_expired(now))
            .unwrap_or(true) // 존재하지 않는 방은 만료로 간주
    }
}
