use libp2p::Multiaddr;

/// 네트워크 모드.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkMode {
    /// 인터넷 모드: 외부 부트스트랩 피어 + 공개 DHT
    Internet,
    /// 인트라넷 모드: 외부 부트스트랩 비활성화, 내부망 DHT + mDNS
    Intranet,
}

/// 네트워크 레이어 초기화 설정.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    /// 리스닝 포트 (0이면 OS 자동 배정)
    pub port: u16,
    /// 최대 동시 연결 수
    pub max_connections: u32,
    /// 인터넷 모드: 부트스트랩 피어 주소 목록
    pub bootstrap_peers: Vec<Multiaddr>,
    /// 인트라넷 모드: 수동 피어 주소 목록
    pub manual_peers: Vec<Multiaddr>,
}

impl NetworkConfig {
    /// 인터넷 모드 기본 설정. libp2p 공개 부트스트랩 노드 사용.
    pub fn internet_default(port: u16) -> Self {
        Self {
            mode: NetworkMode::Internet,
            port,
            max_connections: 50,
            bootstrap_peers: default_bootstrap_peers(),
            manual_peers: vec![],
        }
    }

    /// 인트라넷 모드 기본 설정. 외부 부트스트랩 없음.
    pub fn intranet_default(port: u16) -> Self {
        Self {
            mode: NetworkMode::Intranet,
            port,
            max_connections: 50,
            bootstrap_peers: vec![],
            manual_peers: vec![],
        }
    }
}

/// libp2p 공개 부트스트랩 노드 (IPFS 인프라).
fn default_bootstrap_peers() -> Vec<Multiaddr> {
    // 공식 IPFS 부트스트랩 노드
    [
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    ]
    .iter()
    .filter_map(|s| s.parse().ok())
    .collect()
}
