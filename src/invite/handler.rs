//! 초대 플로우 핸들러.
//!
//! ## 초대 수신 측 (방 멤버)
//!
//! 1. `InviteRequest` 도착 → `on_invite_request()` 호출
//!    - `InviteManager`에 `PendingApproval` 등록
//!    - TUI에 승인 팝업 표시 요청 (`AppEvent::InviteReceived`)
//!
//! 2. 사용자가 수락/거절 → `approve()` 또는 `reject()` 호출
//!    - 수락: `InviteAccepted` 전송 (방 키 포함) + `InviteApproval` 브로드캐스트
//!    - 거절: `InviteRejected` 전송 + `InviteApproval` 브로드캐스트
//!    - 브로드캐스트 수신: 나머지 팝업 자동 닫힘
//!
//! ## 초대 발신 측 (피초대자)
//!
//! 1. `InviteAccepted` 수신 → `on_invite_accepted()` 호출
//!    - 방 키 추출 → `AppEvent::JoinedRoom` 트리거

use libp2p::PeerId;
use tokio::sync::mpsc;

use crate::app::channels::{AppEvent, AppEventTx};
use crate::invite::session::{InviteManager, PendingApproval};
use crate::network::codec::{AppResponse, RejectReason};
use crate::network::event::NetworkCommand;
use crate::protocol::gossip::{self, GossipPayload, InviteApproval};
use crate::room::RoomKey;


/// 인바운드 `InviteRequest` 수신 처리.
///
/// 코드 해시를 이미 알고 있는 경우(이 멤버가 코드 생성자인 경우 또는
/// DHT를 통해 검증된 경우) 승인 팝업을 표시한다.
pub async fn on_invite_request(
    invite_manager: &mut InviteManager,
    from_peer: PeerId,
    room_id: [u8; 32],
    _code_creator_peer_id: Vec<u8>,
    app_tx: &AppEventTx,
    invite_number: u32,
    room_name: String,
    from_display: String,
) {
    // 중복 요청 무시 (이미 처리 중인 피어)
    if invite_manager.pending_approvals.contains_key(&from_peer) {
        return;
    }

    let approval = PendingApproval {
        invitee: from_peer,
        code_creator: from_peer, // InviteRequest에서 코드 생성자 = 요청자 (단순화)
        room_id,
        received_ms: crate::room::store::RoomStore::now_ms(),
        decided: false,
    };
    invite_manager.add_pending(approval);

    // TUI에 승인 팝업 표시 요청
    app_tx
        .send(AppEvent::InviteReceived {
            from_peer,
            from_nickname: from_display,
            room_name,
            number: invite_number,
        })
        .await
        .ok();
}

/// 초대 수락 처리.
///
/// 1. `InviteAccepted` 응답 전송 (방 키 포함)
/// 2. 방 GossipSub 토픽으로 `InviteApproval(accepted=true)` 브로드캐스트
/// 3. `PendingApproval` decided 마킹
pub async fn approve(
    invite_manager: &mut InviteManager,
    invitee: PeerId,
    room_key: &RoomKey,
    room_topic: &libp2p::gossipsub::IdentTopic,
    my_peer_id_bytes: Vec<u8>,
    code_hash: [u8; 32],
    response_channel: libp2p::request_response::ResponseChannel<crate::network::codec::AppResponse>,
    net_tx: &mpsc::Sender<NetworkCommand>,
) {
    // 방 키를 직접 InviteAccepted에 담는다.
    // Noise 전송 계층이 채널을 암호화하므로 추가 암호화 불필요.
    let encrypted_room_key = room_key.0.to_vec();

    net_tx
        .send(NetworkCommand::SendResponse {
            channel: response_channel,
            response: AppResponse::InviteAccepted { encrypted_room_key },
        })
        .await
        .ok();

    // GossipSub 브로드캐스트 (승인 결정)
    broadcast_decision(
        net_tx,
        room_topic,
        room_key,
        code_hash,
        invitee.to_bytes(),
        my_peer_id_bytes,
        true,
    )
    .await;

    invite_manager.mark_decided(&invitee);
}

/// 초대 거절 처리.
///
/// 1. `InviteRejected` 응답 전송
/// 2. `InviteApproval(accepted=false)` 브로드캐스트
pub async fn reject(
    invite_manager: &mut InviteManager,
    invitee: PeerId,
    reason: RejectReason,
    room_key: &RoomKey,
    room_topic: &libp2p::gossipsub::IdentTopic,
    my_peer_id_bytes: Vec<u8>,
    code_hash: [u8; 32],
    response_channel: libp2p::request_response::ResponseChannel<crate::network::codec::AppResponse>,
    net_tx: &mpsc::Sender<NetworkCommand>,
) {
    net_tx
        .send(NetworkCommand::SendResponse {
            channel: response_channel,
            response: AppResponse::InviteRejected { reason },
        })
        .await
        .ok();

    broadcast_decision(
        net_tx,
        room_topic,
        room_key,
        code_hash,
        invitee.to_bytes(),
        my_peer_id_bytes,
        false,
    )
    .await;

    invite_manager.mark_decided(&invitee);
}

/// `InviteApproval` GossipSub 브로드캐스트.
///
/// 다른 멤버들이 이 메시지를 수신하면 해당 승인 팝업을 자동으로 닫는다.
async fn broadcast_decision(
    net_tx: &mpsc::Sender<NetworkCommand>,
    topic: &libp2p::gossipsub::IdentTopic,
    room_key: &RoomKey,
    code_hash: [u8; 32],
    invitee_peer_id: Vec<u8>,
    decided_by: Vec<u8>,
    accepted: bool,
) {
    let payload = GossipPayload::InviteApproval(InviteApproval {
        code_hash,
        invitee_peer_id,
        accepted,
        decided_by,
    });

    if let Ok(data) = gossip::encode(&payload, &room_key.0) {
        net_tx
            .send(NetworkCommand::Publish {
                topic: topic.clone(),
                data,
            })
            .await
            .ok();
    }
}

/// `InviteAccepted` 수신 처리 (피초대자 측).
///
/// 방 키를 추출해 `RoomKey`로 변환한 후 반환.
/// 호출자는 이 키로 방에 입장한다.
pub fn on_invite_accepted(encrypted_room_key: Vec<u8>) -> Option<RoomKey> {
    if encrypted_room_key.len() != 32 {
        return None;
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&encrypted_room_key);
    Some(RoomKey(key_bytes))
}

/// GossipSub `InviteApproval` 수신 처리 (다른 멤버 측).
///
/// 이미 결정된 초대이므로 pending approval을 닫는다.
/// TUI에 팝업 닫힘 신호를 보낸다.
pub async fn on_invite_approval_received(
    invite_manager: &mut InviteManager,
    approval: &InviteApproval,
    app_tx: &AppEventTx,
) {
    let Ok(invitee) = PeerId::from_bytes(&approval.invitee_peer_id) else {
        return;
    };
    let Ok(decided_by) = PeerId::from_bytes(&approval.decided_by) else {
        return;
    };

    invite_manager.mark_decided(&invitee);

    app_tx
        .send(AppEvent::InviteDecision {
            accepted: approval.accepted,
            by_peer: decided_by,
        })
        .await
        .ok();
}
