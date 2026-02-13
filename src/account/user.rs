use serde::{Deserialize, Serialize};

/// `users.json`에 저장되는 계정 레코드 (평문).
///
/// 비밀정보(키, 설정)는 별도 `.enc` 파일에 저장한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    /// 고유 계정 ID (영숫자, 3-32자)
    pub id: String,
    /// 화면에 표시되는 닉네임
    pub nickname: String,
    /// Argon2id 솔트 (hex, 32바이트 = 64자)
    pub salt_hex: String,
}
