use std::path::PathBuf;

use zeroize::Zeroizing;

use super::{
    config::Config,
    identity::Identity,
    store::UserStore,
    user::UserRecord,
};

/// 계정 디렉토리 레이아웃.
///
/// ```
/// data_root/
///   users.json
///   users/
///     <id>/
///       identity.enc
///       config.enc
///       rooms.enc        (나중에)
///       friends.enc      (나중에)
///       chatting2.pid
/// ```
#[derive(Clone)]
pub struct AccountPaths {
    pub data_root: PathBuf,
}

impl AccountPaths {
    pub fn new(data_root: PathBuf) -> Self {
        Self { data_root }
    }

    pub fn users_json(&self) -> PathBuf {
        self.data_root.join("users.json")
    }

    pub fn user_dir(&self, id: &str) -> PathBuf {
        self.data_root.join("users").join(id)
    }

    pub fn identity_enc(&self, id: &str) -> PathBuf {
        self.user_dir(id).join("identity.enc")
    }

    pub fn config_enc(&self, id: &str) -> PathBuf {
        self.user_dir(id).join("config.enc")
    }

    pub fn pid_file(&self, id: &str) -> PathBuf {
        self.user_dir(id).join("chatting2.pid")
    }
}

/// 새 계정을 등록한다.
///
/// 1. ID 중복 검사
/// 2. 랜덤 salt 생성
/// 3. identity.enc + config.enc 초기 생성
/// 4. users.json 갱신
pub fn register(
    paths: &AccountPaths,
    id: &str,
    nickname: &str,
    password: &[u8],
    download_path: &str,
    log_path: &str,
) -> Result<(), RegisterError> {
    validate_id(id).map_err(|_| RegisterError::InvalidId)?;

    let mut store = UserStore::load(&paths.users_json())
        .map_err(|e| RegisterError::Store(e.to_string()))?;

    // ID 중복 검사
    if store.find(id).is_some() {
        return Err(RegisterError::DuplicateId);
    }

    // 32 바이트 랜덤 salt 생성
    let mut salt = Zeroizing::new([0u8; 32]);
    getrandom::fill(salt.as_mut()).expect("getrandom failed");
    let salt_hex = hex_encode(&*salt);

    // 디렉토리 생성
    let user_dir = paths.user_dir(id);
    std::fs::create_dir_all(&user_dir)
        .map_err(|e| RegisterError::Io(e.to_string()))?;

    // identity.enc 생성
    let identity = Identity::generate();
    identity
        .save(&paths.identity_enc(id), password, &*salt)
        .map_err(|e| RegisterError::Crypto(e.to_string()))?;

    // config.enc 생성
    let config = Config::default_for(
        nickname.to_string(),
        download_path.to_string(),
        log_path.to_string(),
    );
    config
        .save(&paths.config_enc(id), password, &*salt)
        .map_err(|e| RegisterError::Crypto(e.to_string()))?;

    // users.json 갱신
    store
        .add(UserRecord {
            id: id.to_string(),
            nickname: nickname.to_string(),
            salt_hex,
        })
        .map_err(|e| RegisterError::Store(e.to_string()))?;

    Ok(())
}

/// 로그인 (PW 검증).
///
/// identity.enc와 config.enc 복호화 성공 여부로 패스워드를 검증한다.
pub fn login(
    paths: &AccountPaths,
    id: &str,
    password: &[u8],
) -> Result<(Identity, Config), LoginError> {
    let store = UserStore::load(&paths.users_json())
        .map_err(|e| LoginError::Store(e.to_string()))?;

    let record = store.find(id).ok_or(LoginError::NotFound)?;
    let salt = hex_decode(&record.salt_hex).map_err(|_| LoginError::InvalidSalt)?;

    let identity = Identity::load(&paths.identity_enc(id), password, &salt)
        .map_err(|_| LoginError::WrongPassword)?;

    let config = Config::load(&paths.config_enc(id), password, &salt)
        .map_err(|_| LoginError::WrongPassword)?;

    Ok((identity, config))
}

/// 비밀번호 변경.
///
/// 1. 현재 PW 검증
/// 2. 새 PW로 모든 `.enc` 파일을 `.enc.new`로 먼저 기록
/// 3. atomic rename으로 교체
///
/// save_enc 내부에서 이미 `.tmp` rename을 사용하므로
/// 여기서는 순서 보장에 집중한다.
pub fn change_password(
    paths: &AccountPaths,
    id: &str,
    current_pw: &[u8],
    new_pw: &[u8],
) -> Result<(), PasswordChangeError> {
    let store = UserStore::load(&paths.users_json())
        .map_err(|e| PasswordChangeError::Store(e.to_string()))?;

    let record = store.find(id).ok_or(PasswordChangeError::NotFound)?;
    let salt = hex_decode(&record.salt_hex)
        .map_err(|_| PasswordChangeError::InvalidSalt)?;

    // 현재 PW로 복호화 (검증)
    let identity = Identity::load(&paths.identity_enc(id), current_pw, &salt)
        .map_err(|_| PasswordChangeError::WrongPassword)?;

    let config = Config::load(&paths.config_enc(id), current_pw, &salt)
        .map_err(|_| PasswordChangeError::WrongPassword)?;

    // 새 PW로 재암호화 (save_enc 내부에서 atomic rename)
    identity
        .save(&paths.identity_enc(id), new_pw, &salt)
        .map_err(|e| PasswordChangeError::Crypto(e.to_string()))?;

    config
        .save(&paths.config_enc(id), new_pw, &salt)
        .map_err(|e| PasswordChangeError::Crypto(e.to_string()))?;

    Ok(())
}

/// 닉네임 변경.
///
/// `users.json`의 레코드와 `config.enc`를 모두 갱신한다.
pub fn change_nickname(
    paths: &AccountPaths,
    id: &str,
    password: &[u8],
    new_nickname: &str,
) -> Result<(), NicknameError> {
    let mut store = UserStore::load(&paths.users_json())
        .map_err(|e| NicknameError::Store(e.to_string()))?;

    let record = store.find(id).ok_or(NicknameError::NotFound)?;
    let salt = hex_decode(&record.salt_hex).map_err(|_| NicknameError::InvalidSalt)?;

    // config.enc 로드 및 닉네임 갱신
    let mut config = Config::load(&paths.config_enc(id), password, &salt)
        .map_err(|_| NicknameError::WrongPassword)?;
    config.nickname = new_nickname.to_string();

    // config.enc 저장
    config
        .save(&paths.config_enc(id), password, &salt)
        .map_err(|e| NicknameError::Crypto(e.to_string()))?;

    // users.json 갱신
    store
        .update_nickname(id, new_nickname.to_string())
        .map_err(|e| NicknameError::Store(e.to_string()))?;

    Ok(())
}

/// 크래시 복구: `*.enc.tmp` 파일 감지 및 자동 삭제.
///
/// 비밀번호 변경 중단 시 남은 임시 파일을 정리한다.
pub fn recover_stale_tmp(paths: &AccountPaths, id: &str) {
    for filename in ["identity.enc.tmp", "config.enc.tmp"] {
        let tmp = paths.user_dir(id).join(filename);
        if tmp.exists() {
            std::fs::remove_file(&tmp).ok();
        }
    }
}

/// 계정 삭제.
///
/// `users/<id>/` 디렉토리 전체를 제거하고 `users.json`을 갱신한다.
/// `downloads/` 는 보존한다.
pub fn delete_account(
    paths: &AccountPaths,
    id: &str,
    password: &[u8],
) -> Result<(), DeleteError> {
    // PW 검증 먼저
    login(paths, id, password).map_err(|_| DeleteError::WrongPassword)?;

    // 디렉토리 삭제
    let user_dir = paths.user_dir(id);
    std::fs::remove_dir_all(&user_dir)
        .map_err(|e| DeleteError::Io(e.to_string()))?;

    // users.json 갱신
    let mut store = UserStore::load(&paths.users_json())
        .map_err(|e| DeleteError::Store(e.to_string()))?;

    store
        .remove(id)
        .map_err(|e| DeleteError::Store(e.to_string()))?;

    Ok(())
}

// ── 유효성 검사 ──────────────────────────────────────────────────────────────

fn validate_id(id: &str) -> Result<(), ()> {
    let len = id.len();
    if !(3..=32).contains(&len) {
        return Err(());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(());
    }
    Ok(())
}

// ── 헬퍼 ─────────────────────────────────────────────────────────────────────

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

// ── 에러 타입들 ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RegisterError {
    InvalidId,
    DuplicateId,
    Crypto(String),
    Store(String),
    Io(String),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterError::InvalidId => write!(f, "invalid account id"),
            RegisterError::DuplicateId => write!(f, "account id already exists"),
            RegisterError::Crypto(s) => write!(f, "crypto error: {s}"),
            RegisterError::Store(s) => write!(f, "store error: {s}"),
            RegisterError::Io(s) => write!(f, "io error: {s}"),
        }
    }
}

impl std::error::Error for RegisterError {}

#[derive(Debug)]
pub enum LoginError {
    NotFound,
    WrongPassword,
    InvalidSalt,
    Store(String),
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginError::NotFound => write!(f, "account not found"),
            LoginError::WrongPassword => write!(f, "wrong password"),
            LoginError::InvalidSalt => write!(f, "invalid salt in record"),
            LoginError::Store(s) => write!(f, "store error: {s}"),
        }
    }
}

impl std::error::Error for LoginError {}

#[derive(Debug)]
pub enum PasswordChangeError {
    NotFound,
    WrongPassword,
    InvalidSalt,
    Crypto(String),
    Store(String),
}

impl std::fmt::Display for PasswordChangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasswordChangeError::NotFound => write!(f, "account not found"),
            PasswordChangeError::WrongPassword => write!(f, "wrong password"),
            PasswordChangeError::InvalidSalt => write!(f, "invalid salt"),
            PasswordChangeError::Crypto(s) => write!(f, "crypto error: {s}"),
            PasswordChangeError::Store(s) => write!(f, "store error: {s}"),
        }
    }
}

impl std::error::Error for PasswordChangeError {}

#[derive(Debug)]
pub enum DeleteError {
    WrongPassword,
    Io(String),
    Store(String),
}

impl std::fmt::Display for DeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeleteError::WrongPassword => write!(f, "wrong password"),
            DeleteError::Io(s) => write!(f, "io error: {s}"),
            DeleteError::Store(s) => write!(f, "store error: {s}"),
        }
    }
}

impl std::error::Error for DeleteError {}

#[derive(Debug)]
pub enum NicknameError {
    NotFound,
    WrongPassword,
    InvalidSalt,
    Crypto(String),
    Store(String),
}

impl std::fmt::Display for NicknameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NicknameError::NotFound => write!(f, "account not found"),
            NicknameError::WrongPassword => write!(f, "wrong password"),
            NicknameError::InvalidSalt => write!(f, "invalid salt"),
            NicknameError::Crypto(s) => write!(f, "crypto error: {s}"),
            NicknameError::Store(s) => write!(f, "store error: {s}"),
        }
    }
}

impl std::error::Error for NicknameError {}
