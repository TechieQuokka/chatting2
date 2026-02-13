use libp2p::identity::{self, ed25519, Keypair, PeerId};
use zeroize::Zeroizing;

use crate::crypto::{derive_key, load_enc, save_enc};
use std::path::Path;

/// 계정의 libp2p Keypair.
///
/// libp2p PeerId의 근거가 된다.
/// 비밀키 bytes는 메모리에서 Zeroizing으로 관리한다.
pub struct Identity {
    keypair: Keypair,
    pub peer_id: PeerId,
}

#[derive(Debug)]
pub enum IdentityError {
    Crypto(String),
    InvalidKey(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::Crypto(s) => write!(f, "crypto error: {s}"),
            IdentityError::InvalidKey(s) => write!(f, "invalid key: {s}"),
        }
    }
}

impl std::error::Error for IdentityError {}

impl Identity {
    /// 새 Ed25519 Keypair를 생성한다.
    pub fn generate() -> Self {
        let keypair = identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        Self { keypair, peer_id }
    }

    /// `identity.enc` 파일에서 Keypair를 로드한다.
    ///
    /// 저장 포맷: 64바이트 (secret 32 + public 32), libp2p ed25519::Keypair::to_bytes()
    pub fn load(path: &Path, password: &[u8], salt: &[u8]) -> Result<Self, IdentityError> {
        let enc_key = derive_key(password, salt)
            .map_err(|e| IdentityError::Crypto(e.to_string()))?;

        let plaintext = load_enc(path, &enc_key)
            .map_err(|e| IdentityError::Crypto(e.to_string()))?;

        if plaintext.len() != 64 {
            return Err(IdentityError::InvalidKey(format!(
                "expected 64 bytes, got {}",
                plaintext.len()
            )));
        }

        // try_from_bytes는 성공 시 입력 슬라이스를 0으로 덮어씀 (보안)
        // Zeroizing<Vec>에 복사해서 원본 보호
        let mut kp_bytes = Zeroizing::new(plaintext.to_vec());
        let ed25519_kp = ed25519::Keypair::try_from_bytes(&mut kp_bytes)
            .map_err(|e| IdentityError::InvalidKey(e.to_string()))?;

        let keypair = Keypair::from(ed25519_kp);
        let peer_id = keypair.public().to_peer_id();

        Ok(Self { keypair, peer_id })
    }

    /// `identity.enc` 파일에 Keypair를 저장한다.
    ///
    /// 저장 포맷: 64바이트 (secret 32 + public 32)
    pub fn save(&self, path: &Path, password: &[u8], salt: &[u8]) -> Result<(), IdentityError> {
        let ed25519_kp = self
            .keypair
            .clone()
            .try_into_ed25519()
            .map_err(|e| IdentityError::InvalidKey(e.to_string()))?;

        let kp_bytes = Zeroizing::new(ed25519_kp.to_bytes());

        let enc_key = derive_key(password, salt)
            .map_err(|e| IdentityError::Crypto(e.to_string()))?;

        save_enc(path, &enc_key, kp_bytes.as_ref())
            .map_err(|e| IdentityError::Crypto(e.to_string()))?;

        Ok(())
    }

    /// libp2p Swarm 구성에 필요한 Keypair 참조를 반환한다.
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// PeerId를 hex 문자열로 반환한다 (표시용, #code).
    pub fn peer_id_short(&self) -> String {
        // PeerId의 멀티해시 bytes 앞 4바이트를 hex으로 → 8자리 코드
        let bytes = self.peer_id.to_bytes();
        bytes[bytes.len().saturating_sub(4)..]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}
