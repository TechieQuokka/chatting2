use std::path::{Path, PathBuf};

use serde_json;

use super::user::UserRecord;

/// `users.json` 파일을 관리한다.
///
/// 파일 형식: `[UserRecord, ...]` JSON 배열 (평문).
pub struct UserStore {
    path: PathBuf,
    records: Vec<UserRecord>,
}

#[derive(Debug)]
pub enum UserStoreError {
    Io(std::io::Error),
    Json(serde_json::Error),
    DuplicateId,
    NotFound,
}

impl std::fmt::Display for UserStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserStoreError::Io(e) => write!(f, "io error: {e}"),
            UserStoreError::Json(e) => write!(f, "json error: {e}"),
            UserStoreError::DuplicateId => write!(f, "account id already exists"),
            UserStoreError::NotFound => write!(f, "account not found"),
        }
    }
}

impl std::error::Error for UserStoreError {}

impl UserStore {
    /// `users.json`을 로드한다. 파일이 없으면 빈 목록으로 시작한다.
    pub fn load(path: &Path) -> Result<Self, UserStoreError> {
        let records = if path.exists() {
            let raw = std::fs::read_to_string(path).map_err(UserStoreError::Io)?;
            serde_json::from_str(&raw).map_err(UserStoreError::Json)?
        } else {
            Vec::new()
        };

        Ok(Self {
            path: path.to_path_buf(),
            records,
        })
    }

    /// 현재 레코드 목록을 `users.json`에 저장한다.
    pub fn save(&self) -> Result<(), UserStoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(UserStoreError::Io)?;
        }
        let json = serde_json::to_string_pretty(&self.records).map_err(UserStoreError::Json)?;
        // 임시 파일 → atomic rename
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(UserStoreError::Io)?;
        std::fs::rename(&tmp, &self.path).map_err(UserStoreError::Io)?;
        Ok(())
    }

    pub fn records(&self) -> &[UserRecord] {
        &self.records
    }

    pub fn find(&self, id: &str) -> Option<&UserRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    /// 새 계정을 추가한다. ID 중복 시 에러.
    pub fn add(&mut self, record: UserRecord) -> Result<(), UserStoreError> {
        if self.find(&record.id).is_some() {
            return Err(UserStoreError::DuplicateId);
        }
        self.records.push(record);
        self.save()
    }

    /// 계정을 삭제한다.
    pub fn remove(&mut self, id: &str) -> Result<(), UserStoreError> {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        if self.records.len() == before {
            return Err(UserStoreError::NotFound);
        }
        self.save()
    }

    /// 닉네임을 변경한다.
    pub fn update_nickname(&mut self, id: &str, nickname: String) -> Result<(), UserStoreError> {
        let record = self
            .records
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(UserStoreError::NotFound)?;
        record.nickname = nickname;
        self.save()
    }
}
