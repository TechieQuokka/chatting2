use std::io;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::request_response;
use serde::{Deserialize, Serialize};

// ── RPC 메시지 타입 ───────────────────────────────────────────────────────────

/// request-response 프로토콜로 주고받는 요청 메시지.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppRequest {
    /// 청크 요청 (파일 해시 + 청크 인덱스)
    ChunkRequest {
        file_hash: [u8; 32],
        chunk_index: u32,
    },
    /// 방 입장 시 파일 보유 현황 요청
    BitfieldRequest {
        room_id: [u8; 32],
    },
    /// 초대 요청 (방 내부 ID + 코드 생성자 PeerId bytes)
    InviteRequest {
        room_id: [u8; 32],
        requester_peer_id: Vec<u8>,
    },
}

/// request-response 프로토콜로 주고받는 응답 메시지.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppResponse {
    /// 청크 데이터 응답 (방 키로 AES-256-GCM 암호화된 nonce||ciphertext)
    ChunkResponse {
        chunk_index: u32,
        encrypted_data: Vec<u8>,
    },
    /// 파일 보유 현황 응답
    BitfieldResponse {
        /// (file_hash, bitfield_bytes) 목록
        files: Vec<([u8; 32], Vec<u8>)>,
    },
    /// 초대 수락 (방 키 + 방 이름 포함, Noise 암호화로 전송)
    InviteAccepted {
        /// AES-256-GCM으로 암호화된 방 키 (nonce||ciphertext)
        encrypted_room_key: Vec<u8>,
        /// 방 이름 (피초대자가 rooms.enc에 저장할 때 사용)
        room_name: String,
    },
    /// 초대 거절
    InviteRejected {
        reason: RejectReason,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RejectReason {
    /// 멤버가 거절
    Declined,
    /// TTL 만료
    Expired,
    /// 오입력 횟수 초과
    TooManyAttempts,
}

// ── Codec ─────────────────────────────────────────────────────────────────────

/// 메시지 최대 크기: 512 KiB (청크 256KiB + GCM 태그 + 직렬화 오버헤드)
const MAX_MSG_SIZE: usize = 512 * 1024;

/// 길이-접두사 프레임 형식으로 AppRequest/AppResponse를 직렬화한다.
///
/// 포맷: `length(u32 LE) || bincode_bytes`
#[derive(Clone)]
pub struct AppCodec;

#[async_trait]
impl request_response::Codec for AppCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = AppRequest;
    type Response = AppResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<AppRequest>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_message(io).await.and_then(|bytes| {
            bincode::deserialize(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<AppResponse>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_message(io).await.and_then(|bytes| {
            bincode::deserialize(&bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: AppRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_message(io, &bytes).await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: AppResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&res)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_message(io, &bytes).await
    }
}

// ── 프레임 헬퍼 ───────────────────────────────────────────────────────────────

async fn read_message<T: AsyncRead + Unpin>(io: &mut T) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_MSG_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {len} > {MAX_MSG_SIZE}"),
        ));
    }

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_message<T: AsyncWrite + Unpin>(io: &mut T, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_MSG_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} > {MAX_MSG_SIZE}", data.len()),
        ));
    }
    let len = (data.len() as u32).to_le_bytes();
    io.write_all(&len).await?;
    io.write_all(data).await?;
    io.flush().await?;
    Ok(())
}
