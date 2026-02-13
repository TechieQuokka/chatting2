//! 방 키 전달 실패 폴백 로직.
//!
//! ## 전달 흐름
//!
//! 1. 피초대자가 초대 코드를 제출 → 승인자에게 `InviteRequest` 전송
//! 2. 승인자는 10초 내에 `InviteAccepted`를 응답해야 함
//! 3. 10초 경과 시:
//!    a. TTL 만료 확인 → 만료면 실패 종료
//!    b. 다른 멤버(GossipSub InviteApproval 기록 보유자)에게 재연결 시도
//!    c. 재연결 가능한 멤버 없음 → 재승인 팝업 요청
//!
//! ## 설계 노트
//!
//! `DeliveryTracker`는 피초대자 측에서 보유한다.
//! `attempted_peers`에 이미 시도한 피어를 기록해 반복 연결을 방지한다.

use std::collections::HashSet;

use libp2p::PeerId;

use crate::invite::code::INVITE_TTL_MS;

/// 방 키 전달 시도 추적기 (피초대자 측).
pub struct DeliveryTracker {
    /// 초대 코드 SHA-256 해시.
    pub code_hash: [u8; 32],
    /// 초대 TTL 시작 시각 (ms).
    pub ttl_started_ms: u64,
    /// 현재 시도 중인 피어.
    pub current_peer: Option<PeerId>,
    /// 현재 시도 시작 시각 (ms).
    pub attempt_started_ms: u64,
    /// 이미 시도한 피어 집합.
    pub attempted_peers: HashSet<PeerId>,
    /// GossipSub InviteApproval을 수신한 피어 (이 피어에게 재연결 가능).
    pub approved_peers: Vec<PeerId>,
}

/// 배달 재시도 타임아웃: 10초.
pub const DELIVERY_TIMEOUT_MS: u64 = 10_000;

impl DeliveryTracker {
    pub fn new(code_hash: [u8; 32], ttl_started_ms: u64, first_peer: PeerId) -> Self {
        let now = now_ms();
        let mut attempted = HashSet::new();
        attempted.insert(first_peer);
        Self {
            code_hash,
            ttl_started_ms,
            current_peer: Some(first_peer),
            attempt_started_ms: now,
            attempted_peers: attempted,
            approved_peers: Vec::new(),
        }
    }

    /// 현재 전달 TTL이 만료됐는지 확인.
    pub fn is_ttl_expired(&self) -> bool {
        let elapsed = now_ms().saturating_sub(self.ttl_started_ms);
        elapsed >= INVITE_TTL_MS
    }

    /// 현재 시도가 10초 타임아웃에 걸렸는지 확인.
    pub fn is_attempt_timed_out(&self) -> bool {
        let elapsed = now_ms().saturating_sub(self.attempt_started_ms);
        elapsed >= DELIVERY_TIMEOUT_MS
    }

    /// GossipSub으로 승인 기록을 받은 피어 등록.
    pub fn record_approved_peer(&mut self, peer: PeerId) {
        if !self.approved_peers.contains(&peer) {
            self.approved_peers.push(peer);
        }
    }

    /// 다음 시도 피어 선택.
    ///
    /// - 승인 기록이 있는 미시도 피어 우선
    /// - 없으면 `None` (재승인 팝업 필요)
    pub fn next_peer(&mut self) -> Option<PeerId> {
        // 승인 기록 보유 + 미시도 피어 우선
        for peer in &self.approved_peers {
            if !self.attempted_peers.contains(peer) {
                let peer = *peer;
                self.attempted_peers.insert(peer);
                self.current_peer = Some(peer);
                self.attempt_started_ms = now_ms();
                return Some(peer);
            }
        }
        // 재연결 가능한 피어 없음
        self.current_peer = None;
        None
    }

    /// 전달 성공 처리 (tracker 해제 신호).
    pub fn mark_delivered(&mut self) {
        self.current_peer = None;
    }
}

// ── 초대 수신 처리 헬퍼 ───────────────────────────────────────────────────────

/// 초대 수신 컨텍스트 — 방 입장 여부에 따라 처리 방식이 다름.
pub enum InviteReceiveContext {
    /// 방에 입장하지 않은 상태 → 오버레이 알림.
    NotInRoom,
    /// 방에 입장 중인 상태 → 피드 알림.
    InRoom { room_id: [u8; 32] },
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
