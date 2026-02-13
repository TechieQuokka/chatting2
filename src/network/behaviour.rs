use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use libp2p::{
    connection_limits::{self, ConnectionLimits},
    dcutr, gossipsub, identify, kad, mdns, ping, relay,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
    StreamProtocol,
};

use super::codec::AppCodec;

/// 프로젝트의 모든 libp2p behaviour를 합성한 구조체.
///
/// - gossipsub: 방별 채팅/파일 이벤트 pub/sub (StrictSign)
/// - kademlia: DHT — Provider Records (방 멤버), PUT/GET (초대 코드)
/// - mdns: 같은 서브넷 피어 자동 발견 (인트라넷 모드에서 주 사용)
/// - identify: 프로토콜 버전 교환
/// - relay: NAT hole punching용 circuit relay 클라이언트
/// - dcutr: relay를 통한 direct connection upgrade
/// - ping: 연결 유지 확인
/// - request_response: ChunkRequest/Response, BitfieldRequest/Response, InviteRequest/Response
/// - connection_limits: 최대 동시 연결 수 제한
#[derive(NetworkBehaviour)]
pub struct AppBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
    pub relay: relay::client::Behaviour,
    pub dcutr: dcutr::Behaviour,
    pub ping: ping::Behaviour,
    pub request_response: request_response::Behaviour<AppCodec>,
    pub connection_limits: connection_limits::Behaviour,
}

impl AppBehaviour {
    /// Behaviour 구성 (relay client behaviour는 SwarmBuilder에서 주입).
    pub fn new(
        keypair: &libp2p::identity::Keypair,
        protocol_version: &str,
        relay_behaviour: relay::client::Behaviour,
        max_connections: u32,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let peer_id = keypair.public().to_peer_id();

        // ── GossipSub ────────────────────────────────────────────────────────
        // message_id: 발신자 PeerId + 시퀀스 번호 해시 → 동일 내용 중복 방지
        let message_id_fn = |msg: &gossipsub::Message| {
            let mut s = DefaultHasher::new();
            msg.source.hash(&mut s);
            msg.sequence_number.hash(&mut s);
            gossipsub::MessageId::from(s.finish().to_string())
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            // StrictSign: 모든 메시지는 발신자 서명 필수 (문서 02-security.md)
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .max_transmit_size(512 * 1024) // 512 KiB (청크 256KiB + 오버헤드)
            .build()
            .map_err(|s| format!("gossipsub config error: {s}"))?;

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .map_err(|s| format!("gossipsub init error: {s}"))?;

        // ── Kademlia DHT ──────────────────────────────────────────────────────
        let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));

        // ── mDNS ─────────────────────────────────────────────────────────────
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

        // ── Identify ─────────────────────────────────────────────────────────
        let identify = identify::Behaviour::new(identify::Config::new(
            protocol_version.to_string(),
            keypair.public(),
        ));

        // ── DCUtR ────────────────────────────────────────────────────────────
        let dcutr = dcutr::Behaviour::new(peer_id);

        // ── Ping ─────────────────────────────────────────────────────────────
        let ping = ping::Behaviour::default();

        // ── Request-Response ─────────────────────────────────────────────────
        let request_response = request_response::Behaviour::with_codec(
            AppCodec,
            [(
                StreamProtocol::new("/chatting2/rpc/1.0.0"),
                ProtocolSupport::Full,
            )],
            request_response::Config::default(),
        );

        // ── Connection Limits ─────────────────────────────────────────────────
        let limits = ConnectionLimits::default()
            .with_max_established(Some(max_connections))
            .with_max_pending_outgoing(Some(32));
        let connection_limits = connection_limits::Behaviour::new(limits);

        Ok(Self {
            gossipsub,
            kademlia,
            mdns,
            identify,
            relay: relay_behaviour,
            dcutr,
            ping,
            request_response,
            connection_limits,
        })
    }
}
