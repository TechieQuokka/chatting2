use libp2p::identity::{Keypair, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── 상수 ────────────────────────────────────────────────────────────────────

/// 초대 코드 길이 (영문 대문자 + 숫자).
const CODE_LENGTH: usize = 8;

/// 초대 코드에 사용 가능한 문자 집합.
const CODE_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// 코드 입력 시점부터의 TTL (밀리초): 3분.
pub const INVITE_TTL_MS: u64 = 3 * 60 * 1_000;

/// 최대 오입력 횟수.
pub const MAX_ATTEMPTS: u32 = 3;

// ── 에러 ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum InviteCodeError {
    Serialize(bincode::Error),
    SignFailed(String),
    VerifyFailed,
    InvalidPublicKey,
}

impl std::fmt::Display for InviteCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "serialize: {e}"),
            Self::SignFailed(e) => write!(f, "sign failed: {e}"),
            Self::VerifyFailed => write!(f, "signature verification failed"),
            Self::InvalidPublicKey => write!(f, "invalid public key in DHT record"),
        }
    }
}

impl std::error::Error for InviteCodeError {}

impl From<bincode::Error> for InviteCodeError {
    fn from(e: bincode::Error) -> Self { Self::Serialize(e) }
}

// ── DHT 레코드 ───────────────────────────────────────────────────────────────

/// DHT에 저장되는 초대 레코드.
///
/// DHT key = sha256(invite_code)
/// DHT value = bincode(InviteDhtRecord)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteDhtRecord {
    /// 방 내부 ID (32바이트).
    pub room_id: [u8; 32],
    /// 코드 생성자의 공개키 (protobuf 인코딩).
    pub creator_public_key: Vec<u8>,
    /// 서명 대상: sha256(invite_code). 생성자 Ed25519 개인키로 서명.
    pub signature: Vec<u8>,
}

// ── 초대 코드 생성 ────────────────────────────────────────────────────────────

/// 랜덤 초대 코드 (8자 대문자+숫자) 를 생성한다.
pub fn generate_code() -> String {
    let mut raw = [0u8; CODE_LENGTH];
    getrandom::fill(&mut raw).expect("getrandom failed");
    raw.iter()
        .map(|&b| CODE_CHARSET[b as usize % CODE_CHARSET.len()] as char)
        .collect()
}

/// 초대 코드를 해시해 DHT 키를 반환.
pub fn hash_code(code: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    hasher.finalize().into()
}

/// 초대 코드로 DHT 레코드를 생성한다.
///
/// - `keypair`: 코드 생성자의 libp2p 키쌍
/// - `code`: 생성된 초대 코드
/// - `room_id`: 방 내부 ID
pub fn create_dht_record(
    keypair: &Keypair,
    code: &str,
    room_id: [u8; 32],
) -> Result<InviteDhtRecord, InviteCodeError> {
    let code_hash = hash_code(code);
    let signature = keypair
        .sign(&code_hash)
        .map_err(|e| InviteCodeError::SignFailed(e.to_string()))?;
    let creator_public_key = keypair.public().encode_protobuf();

    Ok(InviteDhtRecord {
        room_id,
        creator_public_key,
        signature,
    })
}

/// DHT에서 받은 레코드를 직렬화한다.
pub fn encode_dht_record(record: &InviteDhtRecord) -> Result<Vec<u8>, InviteCodeError> {
    Ok(bincode::serialize(record)?)
}

/// DHT 값 바이트를 역직렬화한다.
pub fn decode_dht_record(bytes: &[u8]) -> Result<InviteDhtRecord, InviteCodeError> {
    Ok(bincode::deserialize(bytes)?)
}

/// DHT 레코드의 서명을 검증한다.
///
/// - `code`: 피초대자가 입력한 초대 코드
/// - `record`: DHT에서 받은 레코드
pub fn verify_dht_record(code: &str, record: &InviteDhtRecord) -> Result<(), InviteCodeError> {
    let public_key = PublicKey::try_decode_protobuf(&record.creator_public_key)
        .map_err(|_| InviteCodeError::InvalidPublicKey)?;

    let code_hash = hash_code(code);
    if !public_key.verify(&code_hash, &record.signature) {
        return Err(InviteCodeError::VerifyFailed);
    }
    Ok(())
}
