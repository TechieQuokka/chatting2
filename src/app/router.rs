use crate::chat::{LogEntry, LogEntryKind};
use crate::network::event::NetworkEvent;
use crate::protocol::gossip::{self, GossipPayload};
use crate::room::RoomKey;

use super::channels::{AppEvent, AppEventTx, NetworkCommandTx};

/// NetworkEvent를 받아 App 이벤트로 변환하고 TUI 채널로 전송.
///
/// 방 키가 필요한 이벤트는 현재 입장한 방 키로 복호화한다.
pub async fn route_network_event(
    event: NetworkEvent,
    current_room_key: Option<&RoomKey>,
    app_tx: &AppEventTx,
    net_tx: &NetworkCommandTx,
) {
    match event {
        // ── 피어 연결/해제 ────────────────────────────────────────────────────
        NetworkEvent::PeerConnected(peer_id) => {
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
                    app_tx.send(AppEvent::FeedEntry(entry)).await.ok();
                }

                GossipPayload::FileAnnounce(announce) => {
                    let msg = format!("[파일] {} 공유됨 ({} bytes)", announce.name, announce.total_size);
                    app_tx.send(AppEvent::FileAnnounced { announce: announce.clone() }).await.ok();
                    app_tx.send(AppEvent::FeedEntry(LogEntry::file_event(&msg))).await.ok();
                }

                GossipPayload::FileRemove(remove) => {
                    let msg = format!("[파일] 공유 철회됨");
                    app_tx.send(AppEvent::FileRemoved { file_hash: remove.file_hash }).await.ok();
                    app_tx.send(AppEvent::FeedEntry(LogEntry::file_event(&msg))).await.ok();
                }

                GossipPayload::BitfieldUpdate(_update) => {
                    // Transfer 레이어에서 처리 (여기서는 무시)
                }

                GossipPayload::InviteApproval(_approval) => {
                    // invite::handler::on_invite_approval_received에서 처리.
                    // AppCore 레이어에서 InviteManager에 전달해야 함.
                    // 여기서는 라우팅 불가 (InviteManager 없음) → 무시.
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
