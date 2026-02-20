
use crate::chat::log::{ChatLog, LogEntry, LogEntryKind};
use crate::network::event::NetworkEvent;
use crate::protocol::gossip::{self, GossipPayload};
use crate::room::RoomKey;

use super::channels::{AppEvent, AppEventTx, NetworkCommandTx};

/// NetworkEvent를 받아 App 이벤트로 변환하고 TUI 채널로 전송.
///
/// 방 키가 필요한 이벤트는 현재 입장한 방 키로 복호화한다.
/// `chat_log`가 Some이면 수신된 채팅/파일 이벤트를 디스크에 저장한다.
pub async fn route_network_event(
    event: NetworkEvent,
    current_room_key: Option<&RoomKey>,
    chat_log: Option<&ChatLog>,
    app_tx: &AppEventTx,
    _net_tx: &NetworkCommandTx,
) {
    match event {
        // ── 피어 연결/해제 ────────────────────────────────────────────────────
        NetworkEvent::PeerConnected { peer_id, .. } => {
            app_tx.send(AppEvent::PeerJoined {
                peer_id,
                nickname: String::new(), // Identify 수신 전까지 빈 닉네임
            }).await.ok();
        }
        NetworkEvent::PeerDisconnected(peer_id) => {
            app_tx.send(AppEvent::PeerLeft { peer_id }).await.ok();
        }

        // ── GossipSub 메시지 ─────────────────────────────────────────────────
        NetworkEvent::GossipMessage { data, source, .. } => {
            let Some(room_key) = current_room_key else { return };
            let Ok(payload) = gossip::decode(&data, &room_key.0) else { return };

            match payload {
                GossipPayload::Chat(msg) => {
                    let peer_short = source
                        .map(|p| {
                            let bytes = p.to_bytes();
                            bytes.iter().rev().take(4).rev()
                                .map(|b| format!("{b:02x}"))
                                .collect::<String>()
                        })
                        .unwrap_or_default();

                    let entry = LogEntry {
                        timestamp_ms: msg.timestamp_ms,
                        kind: LogEntryKind::Chat {
                            sender_nickname: msg.nickname,
                            sender_peer_short: peer_short,
                            text: msg.text,
                        },
                    };
                    // 수신 메시지를 디스크에 저장
                    if let Some(log) = chat_log {
                        log.append(&entry).ok();
                    }
                    app_tx.send(AppEvent::FeedEntry(entry)).await.ok();
                }

                GossipPayload::FileAnnounce(announce) => {
                    // main.rs에서 FileAnnounced 처리 시 피드 추가 + 로그 기록하므로 여기서는 전달만
                    app_tx.send(AppEvent::FileAnnounced { announce: announce.clone() }).await.ok();
                }

                GossipPayload::FileRemove(remove) => {
                    let msg = format!("[파일] 공유 철회됨");
                    let file_entry = LogEntry::file_event(&msg);
                    if let Some(log) = chat_log {
                        log.append(&file_entry).ok();
                    }
                    app_tx.send(AppEvent::FileRemoved { file_hash: remove.file_hash }).await.ok();
                    app_tx.send(AppEvent::FeedEntry(file_entry)).await.ok();
                }

                GossipPayload::BitfieldUpdate(update) => {
                    // BitfieldUpdate는 Transfer 레이어의 PeerBitfields 갱신에 사용.
                    // 피어의 청크 보유 현황을 피드에 표시하지 않고 내부적으로 전달.
                    // AppEvent에 BitfieldUpdate 이벤트가 없으므로 로그만 남김.
                    // TODO: AppEvent::BitfieldUpdate 이벤트를 추가하면 transfer 루프와 연결 가능.
                    let _ = update; // Transfer 이벤트 루프가 분리 구현되면 여기서 전달
                }

                GossipPayload::InviteApproval(approval) => {
                    // InviteApproval 브로드캐스트: 승인/거절 결과를 TUI에 전달.
                    let accepted = approval.accepted;
                    let by_peer = source.unwrap_or(libp2p::PeerId::random());
                    app_tx.send(AppEvent::InviteDecision { accepted, by_peer }).await.ok();
                }
            }
        }

        // ── Kademlia ──────────────────────────────────────────────────────────
        NetworkEvent::KadGetRecordResult { key: _, result } => {
            if let Err(e) = result {
                app_tx.send(AppEvent::Error(format!("DHT 조회 실패: {e:?}"))).await.ok();
            }
        }
        NetworkEvent::KadPutRecordOk { .. } | NetworkEvent::KadStartProvidingOk { .. } => {}
        NetworkEvent::KadGetProvidersResult { .. } => {}

        // ── Ping ──────────────────────────────────────────────────────────────
        NetworkEvent::PingResult { .. } => {}

        // ── 나머지 ───────────────────────────────────────────────────────────
        _ => {}
    }
}
