use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::crypto::{derive_key, load_enc, save_enc};

/// `config.enc`에 저장되는 계정 설정.
///
/// 민감하지 않은 값도 포함되나, 편의상 전체를 암호화한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub nickname: String,

    // 네트워크
    pub network_mode: NetworkMode,
    pub port: u16,
    pub max_connections: u32,

    // 파일
    pub download_path: String,
    pub max_concurrent_downloads: u32,

    // 채팅
    pub log_path: String,

    // 언어
    pub language: Language,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    Internet,
    Intranet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Korean,
    English,
}

impl Config {
    pub fn default_for(nickname: String, download_path: String, log_path: String) -> Self {
        Self {
            nickname,
            network_mode: NetworkMode::Internet,
            port: 9000,
            max_connections: 50,
            download_path,
            max_concurrent_downloads: 3,
            log_path,
            language: Language::Korean,
        }
    }

    /// `config.enc`에서 설정을 로드한다.
    pub fn load(path: &Path, password: &[u8], salt: &[u8]) -> Result<Self, ConfigError> {
        let enc_key = derive_key(password, salt)
            .map_err(|e| ConfigError::Crypto(e.to_string()))?;

        let plaintext = load_enc(path, &enc_key)
            .map_err(|e| ConfigError::Crypto(e.to_string()))?;

        let config: Config = serde_json::from_slice(&plaintext)
            .map_err(ConfigError::Json)?;

        Ok(config)
    }

    /// `config.enc`에 설정을 저장한다.
    pub fn save(&self, path: &Path, password: &[u8], salt: &[u8]) -> Result<(), ConfigError> {
        let enc_key = derive_key(password, salt)
            .map_err(|e| ConfigError::Crypto(e.to_string()))?;

        let plaintext = serde_json::to_vec(self).map_err(ConfigError::Json)?;

        save_enc(path, &enc_key, &plaintext)
            .map_err(|e| ConfigError::Crypto(e.to_string()))?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Crypto(String),
    Json(serde_json::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Crypto(s) => write!(f, "crypto error: {s}"),
            ConfigError::Json(e) => write!(f, "json error: {e}"),
            ConfigError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}
