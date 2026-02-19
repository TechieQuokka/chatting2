use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    kad, mdns, noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, Swarm,
};
use tokio::sync::mpsc;

use crate::account::Identity;

use super::{
    behaviour::AppBehaviour,
    config::{NetworkConfig, NetworkMode},
    event::{NetworkCommand, NetworkEvent},
};

/// 프로토콜 식별자 (버전 포함).
const PROTOCOL_VERSION: &str = "/filetalk/1.0.0";

/// libp2p Swarm을 구성하고 반환한다.
///
/// relay transport는 behaviour에서 생성된 것을 Swarm 레벨에서 연결해야 하는데,
/// libp2p 0.56에서는 SwarmBuilder에 with_relay_client()를 사용하거나
/// 수동으로 transport에 relay transport를 추가한다.
///
/// 현재 구현은 TCP + Noise + Yamux 기본 transport를 사용하고,
/// relay는 behaviour 레벨에서만 처리한다 (DCUtR 지원).
pub fn build_swarm(
    identity: &Identity,
    config: &NetworkConfig,
) -> Result<Swarm<AppBehaviour>, Box<dyn std::error::Error>> {
    let keypair = identity.keypair().clone();

    // with_relay_client()는 relay transport를 TCP 위에 합성하고,
    // relay::client::Behaviour를 with_behaviour 클로저에 주입한다.
    let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|key, relay_client| {
            AppBehaviour::new(key, PROTOCOL_VERSION, relay_client, config.max_connections)
        })?
        .with_swarm_config(|cfg| {
            cfg.with_idle_connection_timeout(Duration::from_secs(60))
        })
        .build();

    Ok(swarm)
}

/// 네트워크 이벤트 루프.
///
/// - `swarm`: libp2p Swarm 인스턴스
/// - `config`: 네트워크 설정
/// - `cmd_rx`: 앱 → 네트워크 커맨드 수신채널
/// - `event_tx`: 네트워크 → 앱 이벤트 발신채널
pub async fn run_event_loop(
    mut swarm: Swarm<AppBehaviour>,
    config: NetworkConfig,
    mut cmd_rx: mpsc::Receiver<NetworkCommand>,
    event_tx: mpsc::Sender<NetworkEvent>,
) {
    // ── 리스닝 시작 ───────────────────────────────────────────────────────────
    let listen_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", config.port)
        .parse()
        .expect("valid listen addr");
    swarm.listen_on(listen_addr).expect("listen failed");

    // ── Kademlia 모드 설정 ────────────────────────────────────────────────────
    // 인터넷 모드: Server (라우팅 테이블 참여)
    // 인트라넷 모드: Client (외부 DHT 비참여)
    match config.mode {
        NetworkMode::Internet => {
            swarm
                .behaviour_mut()
                .kademlia
                .set_mode(Some(kad::Mode::Server));

            // 부트스트랩 피어 연결
            for addr in &config.bootstrap_peers {
                swarm.dial(addr.clone()).ok();
            }

            // DHT 부트스트랩 시작
            swarm.behaviour_mut().kademlia.bootstrap().ok();
        }
        NetworkMode::Intranet => {
            // 인트라넷에서도 DHT 레코드를 서빙해야 하므로 Server 모드 사용.
            // Client 모드에서는 PutRecord한 레코드를 다른 피어가 GetRecord로 찾을 수 없다.
            swarm
                .behaviour_mut()
                .kademlia
                .set_mode(Some(kad::Mode::Server));

            // 수동 피어 연결
            for addr in &config.manual_peers {
                swarm.dial(addr.clone()).ok();
            }
        }
    }

    // ── 이벤트 루프 ───────────────────────────────────────────────────────────
    loop {
        tokio::select! {
            // 네트워크 이벤트 처리
            event = swarm.select_next_some() => {
                // mDNS로 발견한 피어를 Kademlia 라우팅 테이블에 등록해야
                // DHT PutRecord/GetRecord가 그 피어를 경유해 동작한다.
                if let SwarmEvent::Behaviour(super::behaviour::AppBehaviourEvent::Mdns(
                    mdns::Event::Discovered(ref peers)
                )) = event {
                    for (peer_id, addr) in peers {
                        swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
                    }
                }
                handle_swarm_event(event, &event_tx).await;
            }

            // 앱 커맨드 처리
            Some(cmd) = cmd_rx.recv() => {
                handle_command(cmd, &mut swarm);
            }
        }
    }
}

// ── Swarm 이벤트 → NetworkEvent 변환 ─────────────────────────────────────────

async fn handle_swarm_event(
    event: SwarmEvent<super::behaviour::AppBehaviourEvent>,
    tx: &mpsc::Sender<NetworkEvent>,
) {
    use super::behaviour::AppBehaviourEvent;
    use libp2p::{gossipsub, identify, mdns, ping, request_response};

    let net_event = match event {
        // 연결 이벤트
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            Some(NetworkEvent::PeerConnected(peer_id))
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            Some(NetworkEvent::PeerDisconnected(peer_id))
        }

        // Behaviour 이벤트
        SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
            // mDNS
            AppBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
                Some(NetworkEvent::MdnsDiscovered(peers))
            }
            AppBehaviourEvent::Mdns(mdns::Event::Expired(peers)) => {
                Some(NetworkEvent::MdnsExpired(peers))
            }

            // Identify
            AppBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                Some(NetworkEvent::PeerIdentified {
                    peer_id,
                    info: Box::new(info),
                })
            }

            // Kademlia
            AppBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                result, ..
            }) => match result {
                kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                    Some(NetworkEvent::KadPutRecordOk { key })
                }
                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
                    kad::PeerRecord { record, .. },
                ))) => Some(NetworkEvent::KadGetRecordResult {
                    key: record.key.clone(),
                    result: Ok(record.value),
                }),
                kad::QueryResult::GetRecord(Err(e)) => {
                    let key = e.key().clone();
                    Some(NetworkEvent::KadGetRecordResult {
                        key,
                        result: Err(e),
                    })
                }
                kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                    Some(NetworkEvent::KadStartProvidingOk { key })
                }
                kad::QueryResult::GetProviders(Ok(
                    kad::GetProvidersOk::FoundProviders { key, providers, .. },
                )) => Some(NetworkEvent::KadGetProvidersResult {
                    key,
                    providers: providers.into_iter().collect(),
                }),
                _ => None,
            },

            // GossipSub
            AppBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                message,
                propagation_source: _,
                ..
            }) => Some(NetworkEvent::GossipMessage {
                topic: message.topic,
                source: message.source,
                data: message.data,
            }),

            // Request-Response
            AppBehaviourEvent::RequestResponse(request_response::Event::Message {
                peer,
                message,
                ..
            }) => match message {
                request_response::Message::Request {
                    request, channel, ..
                } => Some(NetworkEvent::InboundRequest {
                    peer,
                    request,
                    channel,
                }),
                request_response::Message::Response { response, .. } => {
                    Some(NetworkEvent::InboundResponse { peer, response })
                }
            },

            AppBehaviourEvent::RequestResponse(
                request_response::Event::OutboundFailure { peer, error, .. },
            ) => Some(NetworkEvent::OutboundFailure {
                peer,
                error: error.to_string(),
            }),

            // Ping
            AppBehaviourEvent::Ping(ping::Event { peer, result, .. }) => {
                let rtt_ms = result.ok().map(|d| d.as_millis() as u64);
                Some(NetworkEvent::PingResult { peer, rtt_ms })
            }

            // ConnectionLimits: 초과 시 연결이 자동 거부됨 (별도 이벤트 없음)
            AppBehaviourEvent::ConnectionLimits(_) => None,

            _ => None,
        },

        _ => None,
    };

    if let Some(ev) = net_event {
        tx.send(ev).await.ok();
    }
}

// ── NetworkCommand 처리 ───────────────────────────────────────────────────────

fn handle_command(cmd: NetworkCommand, swarm: &mut Swarm<AppBehaviour>) {
    use super::event::NetworkCommand::*;

    match cmd {
        Subscribe { topic } => {
            swarm.behaviour_mut().gossipsub.subscribe(&topic).ok();
        }
        Unsubscribe { topic } => {
            let _ = swarm.behaviour_mut().gossipsub.unsubscribe(&topic);
        }
        Publish { topic, data } => {
            swarm.behaviour_mut().gossipsub.publish(topic, data).ok();
        }

        PutRecord { key, value } => {
            let record = kad::Record {
                key: kad::RecordKey::new(&key),
                value,
                publisher: None,
                expires: None,
            };
            swarm
                .behaviour_mut()
                .kademlia
                .put_record(record, kad::Quorum::One)
                .ok();
        }
        GetRecord { key } => {
            swarm
                .behaviour_mut()
                .kademlia
                .get_record(kad::RecordKey::new(&key));
        }
        StartProviding { key } => {
            swarm
                .behaviour_mut()
                .kademlia
                .start_providing(kad::RecordKey::new(&key))
                .ok();
        }
        GetProviders { key } => {
            swarm
                .behaviour_mut()
                .kademlia
                .get_providers(kad::RecordKey::new(&key));
        }

        SendRequest { peer, request } => {
            swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer, request);
        }
        SendResponse { channel, response } => {
            swarm
                .behaviour_mut()
                .request_response
                .send_response(channel, response)
                .ok();
        }

        DialPeer { addr } => {
            swarm.dial(addr).ok();
        }
        AddKadAddress { peer_id, addr } => {
            swarm
                .behaviour_mut()
                .kademlia
                .add_address(&peer_id, addr);
        }
        SetKadMode { server } => {
            let mode = if server {
                kad::Mode::Server
            } else {
                kad::Mode::Client
            };
            swarm
                .behaviour_mut()
                .kademlia
                .set_mode(Some(mode));
        }
    }
}
