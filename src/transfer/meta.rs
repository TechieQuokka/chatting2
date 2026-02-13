use std::fs;
use std::io::{self, BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::protocol::gossip::{DirNode, FileAnnounce, FileEntry, ShareType};

/// 청크 크기: 256KB.
pub const CHUNK_SIZE: u64 = 256 * 1024;

// ── 에러 ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum MetaError {
    Io(io::Error),
    EmptyFile,
    PathNotFound,
}

impl std::fmt::Display for MetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::EmptyFile => write!(f, "empty file"),
            Self::PathNotFound => write!(f, "path not found"),
        }
    }
}

impl std::error::Error for MetaError {}

impl From<io::Error> for MetaError {
    fn from(e: io::Error) -> Self { Self::Io(e) }
}

// ── 청크 수 계산 ──────────────────────────────────────────────────────────────

/// 파일 크기에서 256KB 청크 수를 계산한다.
pub fn chunk_count(file_size: u64) -> u32 {
    ((file_size + CHUNK_SIZE - 1) / CHUNK_SIZE) as u32
}

// ── SHA-256 해시 ──────────────────────────────────────────────────────────────

/// 파일을 읽으면서 청크별 SHA-256 해시와 전체 파일 SHA-256 해시를 한 번에 계산한다.
///
/// 반환: `(chunk_hashes, file_hash)`
pub fn hash_file(path: &Path) -> Result<(Vec<[u8; 32]>, [u8; 32]), MetaError> {
    let file = fs::File::open(path)?;
    let meta = file.metadata()?;
    let file_size = meta.len();

    if file_size == 0 {
        return Err(MetaError::EmptyFile);
    }

    let n = chunk_count(file_size) as usize;
    let mut chunk_hashes = Vec::with_capacity(n);
    let mut file_hasher = Sha256::new();
    let mut reader = BufReader::new(file);
    let mut buf = vec![0u8; CHUNK_SIZE as usize];

    loop {
        let mut total_read = 0usize;
        // 청크 하나를 읽음
        loop {
            let n_read = reader.read(&mut buf[total_read..])?;
            if n_read == 0 {
                break;
            }
            total_read += n_read;
            if total_read >= CHUNK_SIZE as usize {
                break;
            }
        }
        if total_read == 0 {
            break;
        }
        let chunk_data = &buf[..total_read];
        // 청크 해시
        let mut chunk_hasher = Sha256::new();
        chunk_hasher.update(chunk_data);
        chunk_hashes.push(chunk_hasher.finalize().into());
        // 전체 파일 해시에도 포함
        file_hasher.update(chunk_data);
    }

    let file_hash = file_hasher.finalize().into();
    Ok((chunk_hashes, file_hash))
}

// ── FileAnnounce 생성 ─────────────────────────────────────────────────────────

/// 단일 파일의 `FileAnnounce` 메타데이터를 생성한다.
///
/// 실제 파일 데이터는 포함되지 않음 (메타데이터만).
pub fn build_file_announce(path: &Path) -> Result<FileAnnounce, MetaError> {
    if !path.exists() {
        return Err(MetaError::PathNotFound);
    }

    if path.is_file() {
        let entry = build_file_entry(path)?;
        let total_size = entry.size;
        Ok(FileAnnounce {
            share_type: ShareType::File,
            name: path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            total_size,
            dir_structure: None,
            files: vec![entry],
        })
    } else if path.is_dir() {
        build_folder_announce(path)
    } else {
        Err(MetaError::PathNotFound)
    }
}

fn build_file_entry(path: &Path) -> Result<FileEntry, MetaError> {
    let meta = fs::metadata(path)?;
    let size = meta.len();
    let (chunk_hashes, file_hash) = hash_file(path)?;
    Ok(FileEntry {
        name: path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size,
        chunk_count: chunk_count(size),
        chunk_hashes,
        file_hash,
    })
}

fn build_folder_announce(root: &Path) -> Result<FileAnnounce, MetaError> {
    let mut all_files = Vec::new();
    let dir_structure = build_dir_node(root, &mut all_files)?;

    let total_size = all_files.iter().map(|f: &FileEntry| f.size).sum();

    Ok(FileAnnounce {
        share_type: ShareType::Folder,
        name: root.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        total_size,
        dir_structure: Some(dir_structure),
        files: all_files,
    })
}

fn build_dir_node(dir: &Path, all_files: &mut Vec<FileEntry>) -> Result<DirNode, MetaError> {
    let name = dir.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let mut children = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        if path.is_file() {
            all_files.push(build_file_entry(&path)?);
        } else if path.is_dir() {
            children.push(build_dir_node(&path, all_files)?);
        }
    }

    Ok(DirNode { name, children })
}

// ── 청크 해시 검증 ────────────────────────────────────────────────────────────

/// 수신한 청크 데이터의 SHA-256이 예상 해시와 일치하는지 검증.
pub fn verify_chunk(data: &[u8], expected_hash: &[u8; 32]) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash: [u8; 32] = hasher.finalize().into();
    hash == *expected_hash
}
