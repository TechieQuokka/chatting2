use std::path::Path;
use zeroize::Zeroizing;

use super::aead::{decrypt, encrypt, EncryptedData};

/// `.enc` 파일을 읽어 복호화한 뒤 평문을 반환한다.
pub fn load_enc(path: &Path, key: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>, EncFileError> {
    let raw = std::fs::read(path).map_err(EncFileError::Io)?;
    let data = EncryptedData::from_bytes(raw);
    decrypt(key, &data).map_err(|_| EncFileError::DecryptFailed)
}

/// 평문을 암호화해 `.enc` 파일로 저장한다.
///
/// 임시 파일(`path` + `.tmp`)에 먼저 쓰고 원자적으로 rename한다.
pub fn save_enc(path: &Path, key: &[u8; 32], plaintext: &[u8]) -> Result<(), EncFileError> {
    let encrypted = encrypt(key, plaintext).map_err(|_| EncFileError::EncryptFailed)?;

    let tmp_path = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".into()),
    );

    std::fs::write(&tmp_path, encrypted.as_bytes()).map_err(EncFileError::Io)?;
    std::fs::rename(&tmp_path, path).map_err(EncFileError::Io)?;

    Ok(())
}

#[derive(Debug)]
pub enum EncFileError {
    Io(std::io::Error),
    DecryptFailed,
    EncryptFailed,
}

impl std::fmt::Display for EncFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncFileError::Io(e) => write!(f, "io error: {e}"),
            EncFileError::DecryptFailed => write!(f, "decryption failed"),
            EncFileError::EncryptFailed => write!(f, "encryption failed"),
        }
    }
}

impl std::error::Error for EncFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EncFileError::Io(e) => Some(e),
            _ => None,
        }
    }
}
