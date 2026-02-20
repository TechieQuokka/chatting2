use libp2p::{gossipsub, identify, kad, PeerId, request_response, Multiaddr};

use super::codec::{AppRequest, AppResponse};

/// 네트워크 레이어에서 앱 레이어로 올라오는 이벤트.
///
/// App 태스크와 Transfer 태스크가 이 이벤트를 수신해 처리한다.
#[derive(Debug)]
pub enum NetworkEvent {
    // ── 피어 연결 ─────────────────────────────────────────────────────────────
    PeerConnected { peer_id: PeerId, addr: Multiaddr },
    PeerDisconnected(PeerId),

    // ── mDNS 발견 ─────────────────────────────────────────────────────────────
    MdnsDiscovered(Vec<(PeerId, Multiaddr)>),
    MdnsExpired(Vec<(PeerId, Multiaddr)>),

    // ── Identify ──────────────────────────────────────────────────────────────
    PeerIdentified {
        peer_id: PeerId,
        info: Box<identify::Info>,
    },

    // ── Kademlia ──────────────────────────────────────────────────────────────
    /// DHT PUT 완료
    KadPutRecordOk { key: kad::RecordKey },
    /// DHT GET 결과
    KadGetRecordResult {
        key: kad::RecordKey,
        result: Result<Vec<u8>, kad::GetRecordError>,
    },
    /// Provider Records 등록 완료
    KadStartProvidingOk { key: kad::RecordKey },
    /// Provider Records 조회 결과
    KadGetProvidersResult {
        key: kad::RecordKey,
        providers: Vec<PeerId>,
    },

    // ── GossipSub ─────────────────────────────────────────────────────────────
    GossipMessage {
        topic: gossipsub::TopicHash,
        source: Option<PeerId>,
        data: Vec<u8>,
    },

    // ── Request-Response ──────────────────────────────────────────────────────
    InboundRequest {
        peer: PeerId,
        request: AppRequest,
        channel: request_response::ResponseChannel<AppResponse>,
    },
    InboundResponse {
        peer: PeerId,
        response: AppResponse,
    },
    OutboundFailure {
        peer: PeerId,
        error: String,
    },

    // ── Ping ──────────────────────────────────────────────────────────────────
    PingResult {
        peer: PeerId,
        rtt_ms: Option<u64>,
    },
}

/// 앱 레이어 → 네트워크 레이어 커맨드.
pub enum NetworkCommand {
    // ── GossipSub ─────────────────────────────────────────────────────────────
    /// 방 토픽 구독
    Subscribe { topic: gossipsub::IdentTopic },
    /// 방 토픽 구독 해제
    Unsubscribe { topic: gossipsub::IdentTopic },
    /// GossipSub 메시지 발행
    Publish {
        topic: gossipsub::IdentTopic,
        data: Vec<u8>,
    },

    // ── Kademlia ──────────────────────────────────────────────────────────────
    /// DHT에 값 저장 (초대 코드 등)
    PutRecord {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    /// DHT에서 값 조회
    GetRecord { key: Vec<u8> },
    /// Provider Record 등록 (방 입장 시 자신을 멤버로 광고)
    StartProviding { key: Vec<u8> },
    /// Provider 조회
    GetProviders { key: Vec<u8> },

    // ── Request-Response ──────────────────────────────────────────────────────
    /// 피어에게 요청 전송
    SendRequest {
        peer: PeerId,
        request: AppRequest,
    },
    /// 인바운드 요청에 응답
    SendResponse {
        channel: request_response::ResponseChannel<AppResponse>,
        response: AppResponse,
    },

    // ── 연결 관리 ─────────────────────────────────────────────────────────────
    /// 수동 피어 주소 연결
    DialPeer { addr: libp2p::Multiaddr },
    /// PeerId로 직접 연결 (토렌트 방식: DHT provider 자동 연결용)
    DialPeerId { peer: PeerId },
    /// Kademlia에 피어 주소 추가
    AddKadAddress {
        peer_id: PeerId,
        addr: libp2p::Multiaddr,
    },
    /// Kademlia 모드 전환 (Server/Client)
    SetKadMode { server: bool },
}
