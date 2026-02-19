//! 방 URL DHT 레코드.
//!
//! 초대자는 초대 코드 생성 시 자신의 user_id를 키로 방 목록을 DHT에 등록한다.
//! 피초대자는 초대자의 user_id(URL)를 입력해 DHT 조회 → 방 목록 수신 → 방 선택.
//!
//! DHT 키: `sha256("url_v1:" || url)`
//! DHT 값: `bincode(Vec<UrlRoomEntry>)`

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// URL DHT 레코드에 포함되는 방 항목.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlRoomEntry {
    /// 방 내부 ID (32바이트).
    pub room_id: [u8; 32],
    /// 방 식별자 — 기본값: 방 이름.
    pub identifier: String,
}

/// URL 문자열을 DHT 키(32바이트)로 해시한다.
///
/// `"url_v1:"` 접두사를 붙여 초대 코드 키와 충돌을 방지한다.
pub fn hash_url(url: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"url_v1:");
    hasher.update(url.as_bytes());
    hasher.finalize().into()
}

/// 방 목록을 DHT 값으로 직렬화한다.
pub fn encode_url_record(rooms: &[UrlRoomEntry]) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(rooms)
}

/// DHT 값을 방 목록으로 역직렬화한다.
pub fn decode_url_record(bytes: &[u8]) -> Result<Vec<UrlRoomEntry>, bincode::Error> {
    bincode::deserialize(bytes)
}
