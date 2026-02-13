use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use zeroize::Zeroizing;

/// `nonce(12B) || ciphertext` 포맷으로 암호화된 데이터
#[derive(Debug, Clone)]
pub struct EncryptedData(pub Vec<u8>);

impl EncryptedData {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// AES-256-GCM으로 평문을 암호화한다.
///
/// CSPRNG로 생성한 12바이트 nonce를 앞에 붙여 반환한다:
/// `nonce(12B) || ciphertext`
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<EncryptedData, aes_gcm::Error> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);

    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext)?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);

    Ok(EncryptedData(out))
}

/// `nonce(12B) || ciphertext` 포맷의 데이터를 복호화한다.
pub fn decrypt(key: &[u8; 32], data: &EncryptedData) -> Result<Zeroizing<Vec<u8>>, aes_gcm::Error> {
    let bytes = &data.0;
    if bytes.len() < 12 {
        return Err(aes_gcm::Error);
    }

    let (nonce_bytes, ciphertext) = bytes.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher.decrypt(nonce, ciphertext)?;
    Ok(Zeroizing::new(plaintext))
}
