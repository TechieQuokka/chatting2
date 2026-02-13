use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

#[derive(Debug)]
pub enum KdfError {
    InvalidParams,
    HashFailed,
}

impl std::fmt::Display for KdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KdfError::InvalidParams => write!(f, "invalid argon2 params"),
            KdfError::HashFailed => write!(f, "argon2 hashing failed"),
        }
    }
}

impl std::error::Error for KdfError {}

/// Argon2id로 패스워드에서 32바이트 암호화 키를 유도한다.
///
/// - m_cost: 65536 KiB (64 MiB)
/// - t_cost: 3 회
/// - p_cost: 1 스레드
/// - output: 32 바이트 (AES-256 키)
pub fn derive_key(password: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, KdfError> {
    let params = Params::new(
        65536, // m_cost (KiB)
        3,     // t_cost
        1,     // p_cost
        Some(32),
    )
    .map_err(|_| KdfError::InvalidParams)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|_| KdfError::HashFailed)?;

    Ok(key)
}
