use serde::{Deserialize, Serialize};

use crate::crypto::{decrypt, encrypt, EncryptedData};

// ── 에러 ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum GossipError {
    Serialize(bincode::Error),
    Encrypt(aes_gcm::Error),
    Decrypt(aes_gcm::Error),
    TooShort,
}

impl std::fmt::Display for GossipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "serialize: {e}"),
            Self::Encrypt(e) => write!(f, "encrypt: {e}"),
            Self::Decrypt(e) => write!(f, "decrypt: {e}"),
            Self::TooShort => write!(f, "data too short"),
        }
    }
}

impl std::error::Error for GossipError {}

impl From<bincode::Error> for GossipError {
    fn from(e: bincode::Error) -> Self {
        Self::Serialize(e)
    }
}

// ── GossipSub 페이로드 ───────────────────────────────────────────────────────

/// GossipSub으로 브로드캐스트되는 메시지 타입.
///
/// GossipSub data 필드 = AES-256-GCM(room_key, bincode(GossipPayload)).
/// 발신자 PeerId 인증은 GossipSub StrictSign 모드가 자동 처리.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipPayload {
    Chat(ChatMessage),
    FileAnnounce(FileAnnounce),
    FileRemove(FileRemove),
    BitfieldUpdate(BitfieldUpdate),
    /// 초대 승인 결정 브로드캐스트.
    ///
    /// 첫 번째 승인자가 결정하면 나머지 멤버의 팝업을 자동으로 닫는다.
    InviteApproval(InviteApproval),
}

// ── InviteApproval ────────────────────────────────────────────────────────────

/// 초대 승인/거절 결정 브로드캐스트.
///
/// 방 내부에서 누군가 먼저 승인/거절하면 전체 방에 브로드캐스트해
/// 나머지 멤버의 승인 팝업을 자동으로 닫는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteApproval {
    /// 초대 코드 SHA-256 해시 (어떤 초대인지 식별).
    pub code_hash: [u8; 32],
    /// 초대 요청자 PeerId bytes.
    pub invitee_peer_id: Vec<u8>,
    /// 승인 여부.
    pub accepted: bool,
    /// 결정을 내린 멤버의 PeerId bytes.
    pub decided_by: Vec<u8>,
}

// ── ChatMessage ──────────────────────────────────────────────────────────────

/// 채팅 메시지.
///
/// - `nickname`: 발신 시점 닉네임 (닉네임 변경 시 다음 메시지에서 자동 전파)
/// - `text`: 평문 채팅 내용 (GossipPayload 전체가 방 키로 암호화됨)
/// - `timestamp`: Unix 타임스탬프 (밀리초)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub nickname: String,
    pub text: String,
    pub timestamp_ms: u64,
}

// ── FileAnnounce ─────────────────────────────────────────────────────────────

/// 파일/폴더 공유 알림.
///
/// 단일 메시지로 파일 또는 폴더 전체를 원자적으로 선언.
/// 수신자는 이 메시지 하나로 선택적 다운로드 화면 구성 가능.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnnounce {
    pub share_type: ShareType,
    /// 파일명 또는 폴더명.
    pub name: String,
    /// 전체 크기 (바이트).
    pub total_size: u64,
    /// 폴더 구조 (폴더 공유 시만 Some).
    pub dir_structure: Option<DirNode>,
    /// 파일 항목 목록 (단일 파일 공유 시 길이 1).
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareType {
    File,
    Folder,
}

/// 폴더 트리 노드.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirNode {
    pub name: String,
    pub children: Vec<DirNode>,
}

/// 개별 파일 항목.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    /// 파일 크기 (바이트).
    pub size: u64,
    /// 256KB 청크 수.
    pub chunk_count: u32,
    /// 청크별 SHA-256 해시.
    pub chunk_hashes: Vec<[u8; 32]>,
    /// 전체 파일 SHA-256 해시 (파일 고유 ID로도 사용).
    pub file_hash: [u8; 32],
}

// ── FileRemove ───────────────────────────────────────────────────────────────

/// 파일 공유 철회 알림.
///
/// 발신자가 해당 파일을 더 이상 시딩하지 않음을 의미.
/// 수신자는 해당 피어의 bitfield에서 해당 파일을 제거한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRemove {
    /// 철회할 파일의 SHA-256 해시.
    pub file_hash: [u8; 32],
}

// ── BitfieldUpdate ───────────────────────────────────────────────────────────

/// 청크 보유 현황 브로드캐스트 (토렌트 HAVE 방식).
///
/// 청크 다운로드 완료 즉시 1회 브로드캐스트. 배치/주기 없음.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitfieldUpdate {
    /// 파일 SHA-256 해시.
    pub file_hash: [u8; 32],
    /// 완료된 청크 인덱스.
    pub chunk_index: u32,
}

// ── 직렬화 / 암호화 헬퍼 ─────────────────────────────────────────────────────

/// GossipPayload를 방 키로 암호화해 GossipSub data 바이트를 생성.
pub fn encode(payload: &GossipPayload, room_key: &[u8; 32]) -> Result<Vec<u8>, GossipError> {
    let plain = bincode::serialize(payload)?;
    let encrypted = encrypt(room_key, &plain).map_err(GossipError::Encrypt)?;
    Ok(encrypted.0)
}

/// GossipSub data 바이트를 방 키로 복호화해 GossipPayload를 반환.
pub fn decode(data: &[u8], room_key: &[u8; 32]) -> Result<GossipPayload, GossipError> {
    if data.len() < 12 {
        return Err(GossipError::TooShort);
    }
    let encrypted = EncryptedData(data.to_vec());
    let plain = decrypt(room_key, &encrypted).map_err(GossipError::Decrypt)?;
    let payload = bincode::deserialize(&plain)?;
    Ok(payload)
}
