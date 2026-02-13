use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use libp2p::PeerId;

use super::bitfield::{bf_path, Bitfield, BlacklistSet};

// ── 다운로드 상태 ─────────────────────────────────────────────────────────────

/// 개별 다운로드 항목의 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadStatus {
    /// 활성 다운로드 중.
    Active,
    /// 자동 일시정지 (방 퇴장 시).
    AutoPaused,
    /// 수동 일시정지 (`/pause`).
    ManualPaused,
    /// 청크를 제공하는 피어 없음 — 새 피어 접속 시 자동 재개.
    Waiting,
    /// 완료.
    Completed,
    /// 취소됨.
    Cancelled,
}

// ── DownloadEntry ─────────────────────────────────────────────────────────────

/// 다운로드 큐의 단일 항목.
#[derive(Debug)]
pub struct DownloadEntry {
    /// 파일 SHA-256 해시 (고유 ID).
    pub file_hash: [u8; 32],
    /// 표시 이름.
    pub file_name: String,
    /// 전체 청크 수.
    pub chunk_count: u32,
    /// 로컬 완료 비트필드.
    pub bitfield: Bitfield,
    /// 로컬 저장 경로.
    pub local_path: PathBuf,
    /// 현재 상태.
    pub status: DownloadStatus,
    /// 현재 요청 중인 청크 인덱스 집합.
    pub in_flight: HashSet<u32>,
}

impl DownloadEntry {
    pub fn new(
        file_hash: [u8; 32],
        file_name: String,
        chunk_count: u32,
        local_path: PathBuf,
    ) -> Self {
        let bf_file = bf_path(&local_path);
        let bitfield = Bitfield::load(&bf_file, chunk_count);
        Self {
            file_hash,
            file_name,
            chunk_count,
            bitfield,
            local_path,
            status: DownloadStatus::AutoPaused,
            in_flight: HashSet::new(),
        }
    }

    /// .bf 파일 경로.
    pub fn bf_path(&self) -> PathBuf {
        bf_path(&self.local_path)
    }

    /// 완료 백분율 (0~100).
    pub fn progress_pct(&self) -> f32 {
        if self.chunk_count == 0 {
            return 100.0;
        }
        self.bitfield.completed() as f32 / self.chunk_count as f32 * 100.0
    }
}

// ── DownloadManager ───────────────────────────────────────────────────────────

/// 전체 다운로드 큐 관리.
///
/// 우선순위는 목록 인덱스(0 = 최고 우선순위).
pub struct DownloadManager {
    pub entries: Vec<DownloadEntry>,
    pub blacklist: BlacklistSet,
    /// 최대 동시 활성 다운로드 수.
    pub max_concurrent: usize,
}

impl DownloadManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            entries: Vec::new(),
            blacklist: BlacklistSet::default(),
            max_concurrent,
        }
    }

    /// 새 다운로드 추가.
    pub fn add(
        &mut self,
        file_hash: [u8; 32],
        file_name: String,
        chunk_count: u32,
        local_path: PathBuf,
    ) {
        // 중복 검사
        if self.entries.iter().any(|e| e.file_hash == file_hash) {
            return;
        }
        self.entries.push(DownloadEntry::new(file_hash, file_name, chunk_count, local_path));
    }

    /// 인덱스로 항목 찾기 (1-based, TUI 표시 번호).
    fn entry_mut(&mut self, number: u32) -> Option<&mut DownloadEntry> {
        self.entries.get_mut((number as usize).saturating_sub(1))
    }

    fn entry(&self, number: u32) -> Option<&DownloadEntry> {
        self.entries.get((number as usize).saturating_sub(1))
    }

    // ── 상태 변경 ─────────────────────────────────────────────────────────────

    pub fn pause(&mut self, number: u32) {
        if let Some(e) = self.entry_mut(number) {
            if e.status == DownloadStatus::Active || e.status == DownloadStatus::Waiting {
                e.status = DownloadStatus::ManualPaused;
                e.in_flight.clear();
            }
        }
    }

    pub fn resume(&mut self, number: u32) {
        if let Some(e) = self.entry_mut(number) {
            if e.status == DownloadStatus::ManualPaused
                || e.status == DownloadStatus::AutoPaused
                || e.status == DownloadStatus::Waiting
            {
                e.status = DownloadStatus::Active;
            }
        }
    }

    pub fn cancel(&mut self, number: u32) -> Option<(PathBuf, PathBuf)> {
        let idx = (number as usize).saturating_sub(1);
        if idx < self.entries.len() {
            let e = self.entries.remove(idx);
            let bf = e.bf_path();
            return Some((e.local_path, bf));
        }
        None
    }

    // ── 우선순위 관리 ─────────────────────────────────────────────────────────

    pub fn top(&mut self, number: u32) {
        let idx = (number as usize).saturating_sub(1);
        if idx > 0 && idx < self.entries.len() {
            let e = self.entries.remove(idx);
            self.entries.insert(0, e);
        }
    }

    pub fn up(&mut self, number: u32) {
        let idx = (number as usize).saturating_sub(1);
        if idx > 0 && idx < self.entries.len() {
            self.entries.swap(idx, idx - 1);
        }
    }

    pub fn down(&mut self, number: u32) {
        let idx = (number as usize).saturating_sub(1);
        if idx + 1 < self.entries.len() {
            self.entries.swap(idx, idx + 1);
        }
    }

    // ── 방 퇴장 / 재입장 ──────────────────────────────────────────────────────

    /// 방 퇴장 시 활성/대기 다운로드를 자동 일시정지.
    pub fn auto_pause_all(&mut self) {
        for e in &mut self.entries {
            if e.status == DownloadStatus::Active || e.status == DownloadStatus::Waiting {
                e.status = DownloadStatus::AutoPaused;
                e.in_flight.clear();
            }
        }
    }

    /// 방 재입장 시 자동 일시정지된 항목만 재개 (수동 일시정지는 유지).
    /// `max_concurrent` 슬롯 내에서만 활성화.
    pub fn auto_resume_on_rejoin(&mut self) {
        let mut active_count = self.entries.iter().filter(|e| e.status == DownloadStatus::Active).count();
        for e in &mut self.entries {
            if e.status == DownloadStatus::AutoPaused {
                if active_count < self.max_concurrent {
                    e.status = DownloadStatus::Active;
                    active_count += 1;
                }
            }
        }
    }

    // ── 청크 완료 처리 ────────────────────────────────────────────────────────

    /// 청크 수신 성공. 비트필드 업데이트 및 .bf 플러시.
    ///
    /// 쓰기 순서: 청크 데이터는 호출 전에 디스크에 이미 기록돼야 함.
    pub fn mark_chunk_done(
        &mut self,
        file_hash: &[u8; 32],
        chunk_index: u32,
    ) -> Result<bool, std::io::Error> {
        let Some(e) = self.entries.iter_mut().find(|e| &e.file_hash == file_hash) else {
            return Ok(false);
        };
        e.in_flight.remove(&chunk_index);
        e.bitfield.set(chunk_index);
        e.bitfield.flush(&e.bf_path())?;

        if e.bitfield.is_complete() {
            e.status = DownloadStatus::Completed;
            return Ok(true); // 완료됨
        }
        Ok(false)
    }

    /// 청크 해시 검증 실패 처리.
    ///
    /// 반환: 피어 차단 여부.
    pub fn record_chunk_failure(
        &mut self,
        file_hash: &[u8; 32],
        peer: PeerId,
        chunk_index: u32,
    ) -> bool {
        if let Some(e) = self.entries.iter_mut().find(|e| &e.file_hash == file_hash) {
            e.in_flight.remove(&chunk_index);
        }
        self.blacklist.record_failure(file_hash, peer, chunk_index)
    }

    /// FileRemove 수신: 해당 피어의 bitfield 효과 제거 (blacklist에 기록하지 않음, 단순 제거).
    pub fn on_file_remove(&mut self, _file_hash: &[u8; 32], _peer: PeerId) {
        // PeerBitfields는 transfer 이벤트 루프에서 관리.
        // DownloadManager는 상태 변경 없음 — 다운로드 계속 진행.
    }

    // ── 폴더 선택적 다운로드 ──────────────────────────────────────────────────

    /// 선택된 파일만 다운로드 큐에 추가한다 (폴더 선택적 다운로드).
    ///
    /// `files` — FileAnnounce.files 목록
    /// `selected_hashes` — 사용자가 선택한 파일 해시 집합
    /// `download_dir` — 파일 저장 디렉토리
    pub fn add_selected(
        &mut self,
        files: &[crate::protocol::gossip::FileEntry],
        selected_hashes: &HashSet<[u8; 32]>,
        download_dir: &Path,
    ) {
        for file in files {
            if selected_hashes.contains(&file.file_hash) {
                let local_path = download_dir.join(&file.name);
                self.add(file.file_hash, file.name.clone(), file.chunk_count, local_path);
            }
        }
    }

    /// 전체 파일 다운로드 큐에 추가 (비선택적 — 폴더 전체 수신).
    pub fn add_all(
        &mut self,
        files: &[crate::protocol::gossip::FileEntry],
        download_dir: &Path,
    ) {
        for file in files {
            let local_path = download_dir.join(&file.name);
            self.add(file.file_hash, file.name.clone(), file.chunk_count, local_path);
        }
    }
}
