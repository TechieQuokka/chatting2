//! 파일 전송 핵심 루프.
//!
//! - `dispatch_chunk_requests` — 활성 다운로드에 대해 여러 피어에 동시 청크 요청 전송
//! - `handle_chunk_response`   — ChunkResponse 복호화 → SHA-256 검증 → 디스크 기록
//! - `handle_chunk_request`    — (시딩 측) ChunkRequest 수신 → 암호화 → 응답 전송
//! - `verify_complete_file`    — 전체 청크 완료 시 전체 파일 SHA-256 최종 검증

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

use libp2p::PeerId;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::crypto::{decrypt, encrypt, EncryptedData};
use crate::network::codec::{AppRequest, AppResponse};
use crate::network::event::NetworkCommand;

use super::bitfield::PeerBitfields;
use super::download::{DownloadManager, DownloadStatus};
use super::meta::{verify_chunk, CHUNK_SIZE};
use super::seeding::SeedingManager;

// ── 동시 요청 제한 ────────────────────────────────────────────────────────────

/// 파일 1개당 동시 인플라이트 청크 요청 최대 수.
const MAX_IN_FLIGHT_PER_FILE: usize = 4;

// ── 다운로드: 청크 요청 전송 ──────────────────────────────────────────────────

/// 활성 다운로드에 대해 rarest-first 기준으로 청크 요청을 여러 피어에 전송한다.
///
/// `net_tx`로 `NetworkCommand::SendRequest`를 보내는 방식이므로
/// 실제 전송은 네트워크 태스크가 담당한다.
pub async fn dispatch_chunk_requests(
    download_manager: &mut DownloadManager,
    peer_bitfields: &PeerBitfields,
    net_tx: &mpsc::Sender<NetworkCommand>,
) {
    for entry in &mut download_manager.entries {
        if entry.status != DownloadStatus::Active {
            continue;
        }

        let slots = MAX_IN_FLIGHT_PER_FILE.saturating_sub(entry.in_flight.len());
        if slots == 0 {
            continue;
        }

        // rarest-first: (chunk_index, vec of available peers)
        let candidates = peer_bitfields.rarest_first(
            &entry.file_hash,
            &entry.bitfield,
            &download_manager.blacklist,
        );

        let mut dispatched = 0;
        for (chunk_index, peers) in candidates {
            if dispatched >= slots {
                break;
            }
            if entry.in_flight.contains(&chunk_index) {
                continue;
            }

            // 가장 앞의 피어에게 요청
            if let Some(peer) = peers.into_iter().next() {
                entry.in_flight.insert(chunk_index);
                net_tx
                    .send(NetworkCommand::SendRequest {
                        peer,
                        request: AppRequest::ChunkRequest {
                            file_hash: entry.file_hash,
                            chunk_index,
                        },
                    })
                    .await
                    .ok();
                dispatched += 1;
            }
        }

        // 요청할 청크가 없고 in_flight도 비어있으면 → Waiting 상태
        if dispatched == 0 && entry.in_flight.is_empty() {
            entry.status = DownloadStatus::Waiting;
        }
    }
}

// ── 다운로드: ChunkResponse 처리 ──────────────────────────────────────────────

/// ChunkResponse 수신 시 호출.
///
/// 1. AES-256-GCM 복호화 (방 키) — `nonce(12B) || ciphertext` 포맷
/// 2. SHA-256 해시 검증 (파일 메타데이터의 청크 해시와 비교)
/// 3. 디스크 offset 직접 기록
/// 4. 비트필드 업데이트 및 .bf 플러시
/// 5. 전체 청크 완료 시 전체 파일 SHA-256 최종 검증
///
/// 반환: `Ok(true)` = 다운로드 완료, `Ok(false)` = 진행 중, `Err(_)` = 오류
pub fn handle_chunk_response(
    download_manager: &mut DownloadManager,
    from_peer: PeerId,
    file_hash: &[u8; 32],
    chunk_index: u32,
    encrypted_data: Vec<u8>,
    room_key: &[u8; 32],
    chunk_hashes: &[[u8; 32]],
    expected_file_hash: &[u8; 32],
) -> Result<bool, ChunkError> {
    // 1. 복호화 (nonce || ciphertext → EncryptedData)
    let enc = EncryptedData::from_bytes(encrypted_data);
    let plaintext = decrypt(room_key, &enc).map_err(|_| ChunkError::DecryptFailed)?;

    // 2. SHA-256 해시 검증
    let expected_chunk_hash = chunk_hashes
        .get(chunk_index as usize)
        .ok_or(ChunkError::InvalidChunkIndex)?;

    if !verify_chunk(&plaintext, expected_chunk_hash) {
        // 검증 실패 → 실패 기록 (피어 차단 여부 반환됨)
        let blocked = download_manager.record_chunk_failure(file_hash, from_peer, chunk_index);
        return Err(if blocked {
            ChunkError::PeerBlocked
        } else {
            ChunkError::HashMismatch
        });
    }

    // 3. 디스크 offset 직접 기록
    let local_path = {
        let entry = download_manager
            .entries
            .iter()
            .find(|e| &e.file_hash == file_hash)
            .ok_or(ChunkError::EntryNotFound)?;
        entry.local_path.clone()
    };

    let offset = chunk_index as u64 * CHUNK_SIZE;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(&local_path)
        .map_err(ChunkError::Io)?;

    file.seek(SeekFrom::Start(offset)).map_err(ChunkError::Io)?;
    file.write_all(&plaintext).map_err(ChunkError::Io)?;

    // 4. 비트필드 업데이트 및 .bf 플러시
    //    쓰기 순서: 청크 데이터(위에서 완료) → mark_chunk_done → .bf 플러시
    let completed = download_manager
        .mark_chunk_done(file_hash, chunk_index)
        .map_err(ChunkError::Io)?;

    if completed {
        // 5. 전체 파일 SHA-256 최종 검증
        verify_complete_file(&local_path, expected_file_hash)
            .map_err(|_| ChunkError::FileHashMismatch)?;
        return Ok(true);
    }

    Ok(false)
}

// ── 다운로드: 전체 파일 해시 검증 ────────────────────────────────────────────

/// 다운로드 완료 후 전체 파일 SHA-256 해시를 검증한다.
pub fn verify_complete_file(
    path: &std::path::Path,
    expected_hash: &[u8; 32],
) -> Result<(), ChunkError> {
    let mut file = File::open(path).map_err(ChunkError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 8 * 1024];

    loop {
        let n = file.read(&mut buf).map_err(ChunkError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let hash: [u8; 32] = hasher.finalize().into();
    if &hash != expected_hash {
        return Err(ChunkError::FileHashMismatch);
    }
    Ok(())
}

// ── 시딩: ChunkRequest 처리 ───────────────────────────────────────────────────

/// ChunkRequest 수신 시 시딩 측에서 호출.
///
/// 1. 시딩 중인 파일 + 청크 보유 확인
/// 2. 청크 데이터 읽기 (offset 직접 접근)
/// 3. AES-256-GCM 암호화 (방 키, CSPRNG nonce 자동 생성)
///
/// 반환: 암호화된 청크 bytes (`nonce || ciphertext` 포맷)
pub fn read_chunk_for_seeding(
    seeding_manager: &SeedingManager,
    file_hash: &[u8; 32],
    chunk_index: u32,
    room_key: &[u8; 32],
) -> Result<Vec<u8>, ChunkError> {
    // 시딩 가능 여부 확인
    if !seeding_manager.can_serve(file_hash, chunk_index) {
        return Err(ChunkError::NotAvailable);
    }

    let local_path = seeding_manager
        .local_path(file_hash)
        .ok_or(ChunkError::EntryNotFound)?;

    // 청크 데이터 읽기
    let offset = chunk_index as u64 * CHUNK_SIZE;
    let mut file = File::open(local_path).map_err(ChunkError::Io)?;
    file.seek(SeekFrom::Start(offset)).map_err(ChunkError::Io)?;

    let mut buf = vec![0u8; CHUNK_SIZE as usize];
    let n = file.read(&mut buf).map_err(ChunkError::Io)?;
    buf.truncate(n);

    // AES-256-GCM 암호화 (encrypt 내부에서 CSPRNG nonce 생성)
    let enc = encrypt(room_key, &buf).map_err(|_| ChunkError::DecryptFailed)?;

    // EncryptedData는 이미 nonce || ciphertext 포맷
    Ok(enc.0)
}

// ── 에러 타입 ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ChunkError {
    DecryptFailed,
    HashMismatch,
    FileHashMismatch,
    InvalidChunkIndex,
    EntryNotFound,
    NotAvailable,
    PeerBlocked,
    Io(std::io::Error),
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkError::DecryptFailed => write!(f, "청크 복호화 실패"),
            ChunkError::HashMismatch => write!(f, "청크 해시 불일치"),
            ChunkError::FileHashMismatch => write!(f, "전체 파일 해시 불일치"),
            ChunkError::InvalidChunkIndex => write!(f, "잘못된 청크 인덱스"),
            ChunkError::EntryNotFound => write!(f, "다운로드 항목 없음"),
            ChunkError::NotAvailable => write!(f, "청크 미보유"),
            ChunkError::PeerBlocked => write!(f, "피어 차단됨"),
            ChunkError::Io(e) => write!(f, "IO 오류: {e}"),
        }
    }
}

impl std::error::Error for ChunkError {}
