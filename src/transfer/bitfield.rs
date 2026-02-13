use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libp2p::PeerId;

// ── Bitfield ─────────────────────────────────────────────────────────────────

/// 청크 완료 비트맵.
///
/// 1비트 = 1청크. LSB-first 저장.
/// `.bf` 파일 형식: 비트맵 바이트 그대로 (`chunk_count` 비트, 나머지 비트 0).
#[derive(Debug, Clone)]
pub struct Bitfield {
    bytes: Vec<u8>,
    chunk_count: u32,
}

impl Bitfield {
    /// 모든 청크가 0인 비트필드 생성.
    pub fn new(chunk_count: u32) -> Self {
        let byte_len = ((chunk_count + 7) / 8) as usize;
        Self { bytes: vec![0u8; byte_len], chunk_count }
    }

    /// `.bf` 파일에서 로드. 파일이 없으면 빈 비트필드 반환.
    pub fn load(path: &Path, chunk_count: u32) -> Self {
        let expected_bytes = ((chunk_count + 7) / 8) as usize;
        match fs::read(path) {
            Ok(data) if data.len() == expected_bytes => Self { bytes: data, chunk_count },
            _ => Self::new(chunk_count),
        }
    }

    /// `.bf` 파일로 즉시 플러시.
    ///
    /// 쓰기 순서: `청크 데이터 기록 → 비트필드 메모리 업데이트 → 이 메서드 호출`.
    pub fn flush(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("bf.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&self.bytes)?;
            f.flush()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 청크를 완료로 표시.
    pub fn set(&mut self, chunk_index: u32) {
        if chunk_index >= self.chunk_count {
            return;
        }
        let byte = (chunk_index / 8) as usize;
        let bit = chunk_index % 8;
        self.bytes[byte] |= 1 << bit;
    }

    /// 청크 완료 여부 확인.
    pub fn get(&self, chunk_index: u32) -> bool {
        if chunk_index >= self.chunk_count {
            return false;
        }
        let byte = (chunk_index / 8) as usize;
        let bit = chunk_index % 8;
        (self.bytes[byte] >> bit) & 1 == 1
    }

    /// 완료된 청크 수.
    pub fn completed(&self) -> u32 {
        self.bytes.iter().map(|b| b.count_ones()).sum()
    }

    /// 전체 청크 수.
    pub fn total(&self) -> u32 {
        self.chunk_count
    }

    /// 모든 청크가 완료됐는지 확인.
    pub fn is_complete(&self) -> bool {
        self.completed() == self.chunk_count
    }

    /// 미완료 청크 인덱스 목록.
    pub fn missing(&self) -> Vec<u32> {
        (0..self.chunk_count).filter(|&i| !self.get(i)).collect()
    }

    /// 비트필드 바이트 (BitfieldResponse 전송용).
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// BitfieldResponse로 수신한 바이트에서 생성.
    pub fn from_bytes(bytes: Vec<u8>, chunk_count: u32) -> Self {
        let expected = ((chunk_count + 7) / 8) as usize;
        let bytes = if bytes.len() == expected {
            bytes
        } else {
            vec![0u8; expected]
        };
        Self { bytes, chunk_count }
    }
}

// ── PeerBitfields ─────────────────────────────────────────────────────────────

/// 피어별 청크 보유 현황.
///
/// 파일 해시 → 피어 → 비트필드.
#[derive(Debug, Default)]
pub struct PeerBitfields {
    inner: HashMap<[u8; 32], HashMap<PeerId, Bitfield>>,
}

impl PeerBitfields {
    /// 피어의 비트필드 갱신.
    pub fn update(&mut self, file_hash: [u8; 32], peer: PeerId, bf: Bitfield) {
        self.inner.entry(file_hash).or_default().insert(peer, bf);
    }

    /// 특정 파일에서 특정 청크를 완료로 표시 (BitfieldUpdate 수신 시).
    pub fn mark_chunk(&mut self, file_hash: &[u8; 32], peer: PeerId, chunk_index: u32, chunk_count: u32) {
        self.inner
            .entry(*file_hash)
            .or_default()
            .entry(peer)
            .or_insert_with(|| Bitfield::new(chunk_count))
            .set(chunk_index);
    }

    /// 피어 제거 (FileRemove 수신 또는 피어 연결 해제 시).
    pub fn remove_peer(&mut self, file_hash: &[u8; 32], peer: &PeerId) {
        if let Some(peers) = self.inner.get_mut(file_hash) {
            peers.remove(peer);
        }
    }

    /// 특정 파일에 대해 특정 청크를 보유한 피어 목록.
    pub fn peers_with_chunk(&self, file_hash: &[u8; 32], chunk_index: u32) -> Vec<PeerId> {
        self.inner
            .get(file_hash)
            .into_iter()
            .flat_map(|peers| peers.iter())
            .filter(|(_, bf)| bf.get(chunk_index))
            .map(|(peer, _)| *peer)
            .collect()
    }

    /// Rarest-first: 미완료 청크를 희귀도 순으로 반환.
    ///
    /// 희귀도 = 해당 청크를 가진 피어 수 (적을수록 희귀).
    /// 동률 시 청크 인덱스 오름차순.
    pub fn rarest_first(
        &self,
        file_hash: &[u8; 32],
        our_bitfield: &Bitfield,
        blacklist: &BlacklistSet,
    ) -> Vec<(u32, Vec<PeerId>)> {
        let empty = HashMap::new();
        let peers = self.inner.get(file_hash).unwrap_or(&empty);

        let mut candidates: Vec<(u32, Vec<PeerId>)> = our_bitfield
            .missing()
            .into_iter()
            .filter_map(|chunk_index| {
                let available: Vec<PeerId> = peers
                    .iter()
                    .filter(|(peer, bf)| {
                        bf.get(chunk_index)
                            && !blacklist.is_chunk_blacklisted(file_hash, peer, chunk_index)
                    })
                    .map(|(peer, _)| *peer)
                    .collect();
                if available.is_empty() {
                    None
                } else {
                    Some((chunk_index, available))
                }
            })
            .collect();

        // 희귀한 청크 먼저 (피어 수 오름차순), 동률은 인덱스 오름차순
        candidates.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then(a.0.cmp(&b.0)));
        candidates
    }
}

// ── BlacklistSet ──────────────────────────────────────────────────────────────

/// 청크 블랙리스트 및 피어 전체 차단 목록.
///
/// 세션 내에서만 유지 (앱 재시작 시 초기화).
#[derive(Debug, Default)]
pub struct BlacklistSet {
    /// (file_hash, PeerId, chunk_index) → 실패 횟수.
    chunk_failures: HashMap<([u8; 32], PeerId, u32), u32>,
    /// 전체 차단된 피어 (3회 누적 시).
    blocked_peers: std::collections::HashSet<PeerId>,
}

/// 피어 차단 기준 실패 횟수.
const MAX_CHUNK_FAILURES: u32 = 3;

impl BlacklistSet {
    /// 청크 해시 검증 실패를 기록한다. 3회 누적 시 피어 전체 차단.
    ///
    /// 반환: 이번 실패로 피어가 차단됐는지 여부.
    pub fn record_failure(&mut self, file_hash: &[u8; 32], peer: PeerId, chunk_index: u32) -> bool {
        let count = self.chunk_failures
            .entry((*file_hash, peer, chunk_index))
            .or_insert(0);
        *count += 1;

        // 같은 피어의 어떤 청크든 총 실패 누적 체크
        let total_failures: u32 = self.chunk_failures
            .iter()
            .filter(|((_, p, _), _)| *p == peer)
            .map(|(_, &c)| c)
            .sum();

        if total_failures >= MAX_CHUNK_FAILURES {
            self.blocked_peers.insert(peer);
            return true;
        }
        false
    }

    /// 해당 피어의 해당 청크가 블랙리스트에 있는지 확인.
    pub fn is_chunk_blacklisted(
        &self,
        file_hash: &[u8; 32],
        peer: &PeerId,
        chunk_index: u32,
    ) -> bool {
        self.blocked_peers.contains(peer)
            || self.chunk_failures
                .get(&(*file_hash, *peer, chunk_index))
                .copied()
                .unwrap_or(0)
                > 0
    }

    /// 피어가 전체 차단됐는지 확인.
    pub fn is_peer_blocked(&self, peer: &PeerId) -> bool {
        self.blocked_peers.contains(peer)
    }
}

// ── .bf 파일 경로 ─────────────────────────────────────────────────────────────

/// 다운로드 파일의 `.bf` 파일 경로 계산.
pub fn bf_path(download_path: &Path) -> PathBuf {
    let mut p = download_path.to_path_buf();
    let ext = p.extension()
        .map(|e| format!("{}.bf", e.to_string_lossy()))
        .unwrap_or_else(|| "bf".to_string());
    p.set_extension(&ext);
    p
}
