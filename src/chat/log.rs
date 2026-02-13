use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ── 로그 항목 ────────────────────────────────────────────────────────────────

/// 채팅 로그 파일에 저장되는 단일 항목.
///
/// 형식: JSON Lines (`.jsonl`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unix 타임스탬프 (밀리초).
    pub timestamp_ms: u64,
    pub kind: LogEntryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogEntryKind {
    /// 채팅 메시지.
    Chat {
        sender_nickname: String,
        /// 발신자 PeerId (표시용 헥스 앞 8자리 등).
        sender_peer_short: String,
        text: String,
    },
    /// 시스템 알림 (입장, 퇴장, 방 만료 등).
    System { message: String },
    /// 파일 이벤트 (공유 알림, 공유 철회).
    FileEvent { message: String },
}

impl LogEntry {
    pub fn chat(sender_nickname: &str, sender_peer_short: &str, text: &str) -> Self {
        Self {
            timestamp_ms: now_ms(),
            kind: LogEntryKind::Chat {
                sender_nickname: sender_nickname.to_string(),
                sender_peer_short: sender_peer_short.to_string(),
                text: text.to_string(),
            },
        }
    }

    pub fn system(message: &str) -> Self {
        Self {
            timestamp_ms: now_ms(),
            kind: LogEntryKind::System { message: message.to_string() },
        }
    }

    pub fn file_event(message: &str) -> Self {
        Self {
            timestamp_ms: now_ms(),
            kind: LogEntryKind::FileEvent { message: message.to_string() },
        }
    }
}

// ── ChatLog ──────────────────────────────────────────────────────────────────

/// 방별 채팅 로그 파일 핸들러.
///
/// 파일 위치: `<log_dir>/<room_id_hex>.jsonl`
pub struct ChatLog {
    path: PathBuf,
}

impl ChatLog {
    /// 로그 핸들러 생성 (파일은 append 모드로 열림).
    pub fn new(log_dir: &Path, room_id: &[u8; 32]) -> Result<Self, io::Error> {
        fs::create_dir_all(log_dir)?;
        let hex = hex_short(room_id);
        let path = log_dir.join(format!("{hex}.jsonl"));
        Ok(Self { path })
    }

    /// 로그 항목 1개 추가 (append).
    pub fn append(&self, entry: &LogEntry) -> Result<(), io::Error> {
        let line = serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// 저장된 모든 로그 항목 로드.
    pub fn load_all(&self) -> Result<Vec<LogEntry>, io::Error> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<LogEntry>(&line) {
                Ok(entry) => entries.push(entry),
                Err(_) => continue, // 손상된 줄은 무시
            }
        }
        Ok(entries)
    }

    /// 로그 파일 경로.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ── 유틸 ────────────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hex_short(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
