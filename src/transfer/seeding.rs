use std::path::PathBuf;

use super::bitfield::Bitfield;

// ── 업로드 속도 제한 (토큰 버킷) ─────────────────────────────────────────────

/// 토큰 버킷 방식의 업로드 속도 제한기.
///
/// `bytes_per_sec = 0` 이면 무제한.
pub struct UploadRateLimiter {
    /// 초당 허용 바이트 수 (0 = 무제한).
    pub bytes_per_sec: u64,
    /// 현재 버킷에 남은 토큰 수 (바이트 단위).
    tokens: u64,
    /// 마지막 토큰 보충 시각 (Unix 밀리초).
    last_refill_ms: u64,
}

impl UploadRateLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            bytes_per_sec,
            tokens: bytes_per_sec,
            last_refill_ms: now_ms(),
        }
    }

    /// 무제한 속도로 초기화.
    pub fn unlimited() -> Self {
        Self::new(0)
    }

    /// `bytes`를 전송할 수 있으면 토큰을 소비하고 `true` 반환.
    ///
    /// 속도 제한 초과 시 `false` 반환 — 호출자는 대기 후 재시도해야 함.
    pub fn try_consume(&mut self, bytes: u64) -> bool {
        if self.bytes_per_sec == 0 {
            return true; // 무제한
        }

        self.refill();

        if self.tokens >= bytes {
            self.tokens -= bytes;
            true
        } else {
            false
        }
    }

    /// 경과 시간에 비례해 토큰을 보충한다.
    fn refill(&mut self) {
        let now = now_ms();
        let elapsed_ms = now.saturating_sub(self.last_refill_ms);
        if elapsed_ms == 0 {
            return;
        }

        let new_tokens = self.bytes_per_sec.saturating_mul(elapsed_ms) / 1000;
        self.tokens = self.tokens.saturating_add(new_tokens).min(self.bytes_per_sec);
        self.last_refill_ms = now;
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── 시딩 상태 ─────────────────────────────────────────────────────────────────

/// 시딩 항목 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedStatus {
    /// 활성 시딩 중 (청크 요청 수신 시 응답).
    Active,
    /// 자동 일시정지 (방 퇴장 시). 재입장 시 자동 재개.
    AutoPaused,
    /// 수동 일시정지 (`/seed-pause`). 재입장 시 유지.
    ManualPaused,
}

// ── SeedEntry ─────────────────────────────────────────────────────────────────

/// 시딩 큐의 단일 항목.
pub struct SeedEntry {
    /// 파일 SHA-256 해시.
    pub file_hash: [u8; 32],
    /// 표시 이름.
    pub file_name: String,
    /// 로컬 파일 경로.
    pub local_path: PathBuf,
    /// 현재 상태.
    pub status: SeedStatus,
    /// 로컬 보유 비트필드.
    pub bitfield: Bitfield,
}

impl SeedEntry {
    pub fn new(
        file_hash: [u8; 32],
        file_name: String,
        local_path: PathBuf,
        bitfield: Bitfield,
    ) -> Self {
        Self {
            file_hash,
            file_name,
            local_path,
            status: SeedStatus::Active,
            bitfield,
        }
    }
}

// ── SeedingManager ────────────────────────────────────────────────────────────

/// 전체 시딩 목록 관리.
pub struct SeedingManager {
    pub entries: Vec<SeedEntry>,
    /// 업로드 속도 제한기.
    pub rate_limiter: UploadRateLimiter,
}

impl SeedingManager {
    pub fn new() -> Self {
        Self { entries: Vec::new(), rate_limiter: UploadRateLimiter::unlimited() }
    }

    /// 업로드 속도 제한 설정 (bytes/sec, 0 = 무제한).
    pub fn set_upload_limit(&mut self, bytes_per_sec: u64) {
        self.rate_limiter = UploadRateLimiter::new(bytes_per_sec);
    }

    /// 시딩 목록에 파일 추가. 이미 있으면 무시.
    pub fn add(
        &mut self,
        file_hash: [u8; 32],
        file_name: String,
        local_path: PathBuf,
        bitfield: Bitfield,
    ) {
        if self.entries.iter().any(|e| e.file_hash == file_hash) {
            return;
        }
        self.entries.push(SeedEntry::new(file_hash, file_name, local_path, bitfield));
    }

    fn entry_mut(&mut self, number: u32) -> Option<&mut SeedEntry> {
        self.entries.get_mut((number as usize).saturating_sub(1))
    }

    // ── 상태 변경 ─────────────────────────────────────────────────────────────

    /// `/seed-pause` — 수동 일시정지.
    pub fn manual_pause(&mut self, number: u32) {
        if let Some(e) = self.entry_mut(number) {
            if e.status == SeedStatus::Active || e.status == SeedStatus::AutoPaused {
                e.status = SeedStatus::ManualPaused;
            }
        }
    }

    /// `/seed-resume` — 재개.
    pub fn resume(&mut self, number: u32) {
        if let Some(e) = self.entry_mut(number) {
            e.status = SeedStatus::Active;
        }
    }

    /// `/remove` — 시딩 중단 + 목록 제거 (로컬 파일 유지).
    pub fn remove(&mut self, number: u32) -> Option<PathBuf> {
        let idx = (number as usize).saturating_sub(1);
        if idx < self.entries.len() {
            self.entries.remove(idx);
        }
        None
    }

    /// `/remove-all` — 시딩 중단 + 목록 제거 + 로컬 파일 경로 반환 (호출자가 삭제).
    pub fn remove_and_delete(&mut self, number: u32) -> Option<PathBuf> {
        let idx = (number as usize).saturating_sub(1);
        if idx < self.entries.len() {
            let e = self.entries.remove(idx);
            return Some(e.local_path);
        }
        None
    }

    // ── 방 퇴장 / 재입장 ──────────────────────────────────────────────────────

    /// 방 퇴장 시 활성 시딩 자동 일시정지.
    pub fn auto_pause_all(&mut self) {
        for e in &mut self.entries {
            if e.status == SeedStatus::Active {
                e.status = SeedStatus::AutoPaused;
            }
        }
    }

    /// 방 재입장 시 자동 일시정지된 항목만 재개 (수동 일시정지 유지).
    pub fn auto_resume_on_rejoin(&mut self) {
        for e in &mut self.entries {
            if e.status == SeedStatus::AutoPaused {
                e.status = SeedStatus::Active;
            }
        }
    }

    // ── ChunkRequest 처리 ─────────────────────────────────────────────────────

    /// ChunkRequest를 처리할 수 있는 항목인지 확인.
    ///
    /// 활성 상태이고 해당 청크를 보유해야 하며, 업로드 속도 제한에 여유가 있어야 함.
    /// `chunk_size`: 전송할 청크 크기 (bytes).
    pub fn can_serve(&self, file_hash: &[u8; 32], chunk_index: u32) -> bool {
        self.entries.iter().any(|e| {
            &e.file_hash == file_hash
                && e.status == SeedStatus::Active
                && e.bitfield.get(chunk_index)
        })
    }

    /// 속도 제한을 적용하여 청크 전송 가능 여부 확인 (토큰 소비).
    pub fn try_serve_with_limit(&mut self, file_hash: &[u8; 32], chunk_index: u32, chunk_size: u64) -> bool {
        if !self.can_serve(file_hash, chunk_index) {
            return false;
        }
        self.rate_limiter.try_consume(chunk_size)
    }

    /// 시딩 중인 특정 파일의 로컬 경로 반환.
    pub fn local_path(&self, file_hash: &[u8; 32]) -> Option<&PathBuf> {
        self.entries.iter()
            .find(|e| &e.file_hash == file_hash && e.status == SeedStatus::Active)
            .map(|e| &e.local_path)
    }
}

impl Default for SeedingManager {
    fn default() -> Self {
        Self::new()
    }
}
