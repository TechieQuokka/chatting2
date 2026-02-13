use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::PeerId;

use super::code::{INVITE_TTL_MS, MAX_ATTEMPTS};

// ── 수신 측 (피초대자) 세션 ───────────────────────────────────────────────────

/// 피초대자 측 초대 세션.
///
/// 사용자가 코드를 입력한 시점부터 TTL 카운트 시작.
/// TTL 초과 또는 오입력 3회 시 세션 종료.
#[derive(Debug)]
pub struct IncomingSession {
    /// sha256(입력한 코드) — DHT 조회 키.
    pub code_hash: [u8; 32],
    /// 세션 시작 시각 (Unix ms).
    pub started_ms: u64,
    /// 잘못된 코드 입력 횟수.
    pub bad_attempts: u32,
    /// 방 키 수신 완료 여부 (멱등성: 첫 수신 후 이후 InviteAccepted 무시).
    pub accepted: bool,
}

impl IncomingSession {
    pub fn new(code_hash: [u8; 32]) -> Self {
        Self {
            code_hash,
            started_ms: now_ms(),
            bad_attempts: 0,
            accepted: false,
        }
    }

    /// TTL이 만료됐는지 확인.
    pub fn is_expired(&self) -> bool {
        now_ms().saturating_sub(self.started_ms) >= INVITE_TTL_MS
    }

    /// 남은 TTL (밀리초). 만료됐으면 0.
    pub fn remaining_ttl_ms(&self) -> u64 {
        let elapsed = now_ms().saturating_sub(self.started_ms);
        INVITE_TTL_MS.saturating_sub(elapsed)
    }

    /// 잘못된 코드 입력 처리. 3회 누적 시 차단됨을 반환.
    pub fn record_bad_attempt(&mut self) -> bool {
        self.bad_attempts += 1;
        self.bad_attempts >= MAX_ATTEMPTS
    }

    /// 차단됐는지 확인.
    pub fn is_blocked(&self) -> bool {
        self.bad_attempts >= MAX_ATTEMPTS
    }
}

// ── 발신 측 (승인 대기) 세션 ─────────────────────────────────────────────────

/// 승인 대기 상태 (현재 방 멤버 입장에서).
///
/// 피초대자로부터 InviteRequest를 받은 순간 생성.
/// GossipSub 브로드캐스트로 전체 멤버에게 승인 팝업 표시.
/// 선착순 처리 — 첫 번째 응답이 채택.
#[derive(Debug)]
pub struct PendingApproval {
    /// 피초대자 PeerId.
    pub invitee: PeerId,
    /// 코드 생성자 PeerId (InviteRequest에서 수신).
    pub code_creator: PeerId,
    /// 대상 방 ID.
    pub room_id: [u8; 32],
    /// 요청 수신 시각 (Unix ms).
    pub received_ms: u64,
    /// 이미 결정됐는지 (GossipSub로 다른 멤버가 먼저 처리한 경우).
    pub decided: bool,
}

impl PendingApproval {
    pub fn new(invitee: PeerId, code_creator: PeerId, room_id: [u8; 32]) -> Self {
        Self {
            invitee,
            code_creator,
            room_id,
            received_ms: now_ms(),
            decided: false,
        }
    }
}

// ── InviteManager ─────────────────────────────────────────────────────────────

/// 진행 중인 초대 세션 전체를 관리.
#[derive(Debug, Default)]
pub struct InviteManager {
    /// 피초대자 세션 (피초대자 PeerId → 세션).
    pub incoming: Option<IncomingSession>,
    /// 승인 대기 목록 (피초대자 PeerId → 대기 정보).
    pub pending_approvals: HashMap<PeerId, PendingApproval>,
}

impl InviteManager {
    /// 피초대자로서 새 초대 세션을 시작.
    pub fn start_incoming(&mut self, code_hash: [u8; 32]) {
        self.incoming = Some(IncomingSession::new(code_hash));
    }

    /// 피초대자로서 방 키를 수신했을 때 멱등성 처리.
    ///
    /// 이미 수락됐으면 `false` (무시), 처음이면 `true` (처리).
    pub fn accept_room_key(&mut self) -> bool {
        if let Some(session) = &mut self.incoming {
            if session.accepted {
                return false;
            }
            session.accepted = true;
            return true;
        }
        false
    }

    /// 피초대자 세션 종료 (TTL 만료, 차단, 입장 완료 후).
    pub fn clear_incoming(&mut self) {
        self.incoming = None;
    }

    /// 승인 대기 항목 추가.
    pub fn add_pending(&mut self, approval: PendingApproval) {
        self.pending_approvals.insert(approval.invitee, approval);
    }

    /// GossipSub으로 결정 브로드캐스트를 수신했을 때 해당 팝업을 닫음.
    pub fn mark_decided(&mut self, invitee: &PeerId) {
        if let Some(a) = self.pending_approvals.get_mut(invitee) {
            a.decided = true;
        }
    }

    /// 결정 완료된 항목 정리.
    pub fn cleanup_decided(&mut self) {
        self.pending_approvals.retain(|_, a| !a.decided);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
