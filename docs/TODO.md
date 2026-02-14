# TODO

핵심 구현 항목 체크리스트. 모듈 레이어 순서로 정렬.

---

## 암호화 (crypto)

- [x] Argon2id 키 유도 (계정 PW → 암호화 키)
- [x] AES-256-GCM 암호화 / 복호화 (CSPRNG nonce, `nonce(12B) || ciphertext` 포맷)
- [x] `.enc` 파일 읽기 / 쓰기 헬퍼
- [x] `zeroize` 크레이트 적용 — 방 키 메모리 해제 시 0 덮어쓰기

---

## 계정 / 신원 (account, identity, config)

- [x] Ed25519 키쌍 생성 → PeerId 파생 → `identity.enc` 저장
- [x] `users.json` 읽기 / 쓰기 (ID 목록 평문 관리)
- [x] PID lock 파일 생성 / 해제 / stale 감지 및 교체
- [x] 계정 등록 (ID 중복 검사, enc 파일 초기 생성)
- [x] 로그인 (PW 검증, 전체 enc 파일 복호화)
- [x] 계정 삭제 (`users/{id}/` 전체 제거, `users.json` 갱신)
- [x] 비밀번호 변경 — `*.enc.new` 임시 파일 + 원자적 rename 교체
- [x] 비밀번호 변경 중 크래시 복구 (`*.enc.new` 감지 → 자동 삭제)
- [x] 닉네임 변경 (`config.enc` 갱신)
- [x] `config.enc` 저장 / 로드 (설정 항목 전체)

---

## 네트워크 (network)

- [x] libp2p Swarm 구성 (Transport: TCP + Noise + Yamux + relay transport)
- [x] GossipSub 설정 (StrictSign 모드, 방별 토픽 구독/해제)
- [x] mDNS (같은 서브넷 피어 자동 발견)
- [x] Kademlia DHT
  - [x] 인터넷 모드: 부트스트랩 피어 연결 + 공개 DHT
  - [x] 인트라넷 모드: 외부 부트스트랩 비활성화, 내부망 DHT 구성
  - [x] Provider Records 등록 / 갱신 / 조회 (방 멤버)
  - [x] 일반 PUT / GET (초대 코드 등록 / 조회)
- [x] relay + DCUtR (NAT hole punching)
- [x] identify 프로토콜
- [x] request-response 프로토콜 핸들러
- [x] 인터넷 / 인트라넷 모드 런타임 전환
- [x] 최대 동시 연결 피어 수 제한
- [x] 연결 타임아웃 처리 (idle_connection_timeout 60s)
- [x] 수동 피어 주소 등록 및 연결

---

## 프로토콜 메시지 (protocol)

### GossipSub

- [x] `ChatMessage` — 발신자 PeerId + 닉네임 + 암호화 메시지
- [x] `FileAnnounce` — 파일/폴더 메타데이터 전체 (방 키 암호화)
- [x] `FileRemove` — 공유 철회 알림
- [x] `BitfieldUpdate` — 청크 완료 즉시 1회 브로드캐스트 (HAVE 방식)

### Request-Response

- [x] `ChunkRequest` — 파일 해시 + 청크 인덱스
- [x] `ChunkResponse` — 청크 데이터 + 청크 인덱스 (방 키 암호화)
- [x] `BitfieldRequest` — 방 입장 시 파일 상태 요청
- [x] `BitfieldResponse` — 전체 파일 목록 + 청크 보유 현황
- [x] `InviteRequest` — 방 내부 ID + 코드 생성자 PeerId
- [x] `InviteResponse` — 수락(방 키 포함) / 거절(사유)

---

## 방 (room)

- [x] 방 생성 — 랜덤 고유 ID + 방 키 생성 + 수명 설정
- [x] `rooms.enc` 저장 / 로드 (방 키, 내부 ID, 생성 시각, 수명, 파일 메타데이터)
- [x] DHT Provider Records 등록 (방 생성 / 재입장 시)
- [x] DHT Provider Records 주기적 republish
- [x] 방 수명 만료 체크
  - [x] 앱 실행 시
  - [x] 방 목록 진입 시
  - [x] 채팅/파일 화면 입장 중 1분 간격
- [x] 만료 방 자동 삭제 (rooms.enc에서 제거, 채팅 로그 보존)
- [x] 방 퇴장 처리 — 방 키 zeroize, 시딩/다운로드 자동 일시정지
- [x] 방 삭제 (수동)
- [x] 방 재활성화 — rooms.enc에서 방 정보 복원 + DHT 재등록
- [x] 오프라인 입장 — rooms.enc 캐시 기준 파일 목록 표시
- [x] 방 입장 시 동기화 — BitfieldRequest → BitfieldResponse → rooms.enc 갱신

---

## 초대 (invite)

- [x] 초대 코드 생성 — `hash(코드)` DHT PUT + Ed25519 서명
- [x] 초대 코드 조회 — DHT GET + 서명 검증 + 방 내부 ID 확인
- [x] TTL 관리 — 코드 입력 시점부터 3분 카운트
- [x] 오입력 카운터 — 누적 3회 실패 시 차단 (거절은 카운트 제외)
- [x] 승인 팝업 — 온라인 전체 멤버에게 표시, 선착순 처리
- [x] 승인 결정 GossipSub 브로드캐스트 (나머지 팝업 자동 닫힘)
- [x] 방 키 전달 — InviteResponse (Noise 암호화)
- [x] 방 키 전달 실패 폴백
  - [x] 10초 내 수신 확인 없으면 다른 멤버에게 재연결
  - [x] 매 시도 전 TTL 체크
  - [x] GossipSub 승인 기록 보유 시 즉시 전달 / 미수신 시 재승인 팝업
- [x] 동시 승인 중복 처리 — 피초대자 측 멱등성 (첫 수신 후 이후 무시)
- [x] mDNS 탐색 목록 초대 (인트라넷 모드)
- [x] 친구 초대 — PeerId로 DHT 조회 → 직접 연결
- [x] 초대 수신 처리 — 방 미입장(오버레이) / 방 입장(피드 알림)
- [x] 방 미입장 중 수락 후 나머지 초대 피드 이전

---

## 채팅 (chat)

- [x] ChatMessage 암호화 / 복호화 (방 키 AES-256-GCM)
- [x] GossipSub 발행 / 수신 처리
- [x] 채팅 로그 저장 (방별 파일, `logs/` 디렉토리)
- [x] 명령어 파싱 및 라우팅
- [x] 명령어 히스토리 관리

---

## 파일 공유 (file_meta, transfer, seeding, bitfield)

### 메타데이터

- [x] 256KB 청크 분할
- [x] 청크별 SHA-256 해시 계산
- [x] 전체 파일 SHA-256 해시 계산
- [x] `FileAnnounce` 메타데이터 생성 (파일 / 폴더)
- [x] `rooms.enc`에 파일 메타데이터 영속 저장

### Bitfield

- [x] 피어별 bitfield 관리 (누가 어떤 청크를 갖고 있는지)
- [x] `.bf` 파일 저장 / 로드 (청크 완료 즉시 플러시)
- [x] `.bf` 쓰기 순서 보장 — `청크 기록 → 메모리 업데이트 → 디스크 플러시`

### 다운로드

- [x] Rarest-first 청크 선택
- [x] 여러 피어에게 동시 ChunkRequest 전송
- [x] ChunkResponse 수신 → SHA-256 해시 검증
- [x] 검증 통과 → 디스크 offset 직접 기록
- [x] 전체 청크 완료 → 전체 파일 해시 최종 검증
- [x] 청크 해시 검증 실패 처리
  - [x] 해당 피어의 해당 청크 블랙리스트
  - [x] 다른 피어에게 즉시 재요청
  - [x] 동일 피어 누적 3회 실패 → 피어 연결 차단 (성공해도 카운터 유지)
  - [x] 재요청 가능 피어 없음 → `[⏳]` 대기 상태 전환
- [x] 다운로드 우선순위 관리 (목록 순서 기반)
- [x] 최대 동시 다운로드 수 제한
- [x] 다운로드 일시정지 / 재개 / 취소
- [x] 앱 재시작 시 `.bf` 기반 이어받기 복원 (이전 우선순위 순, 상위 N개만 자동 재개)
- [x] `FileRemove` 수신 처리 — 해당 피어 bitfield 제거, 다운로드 계속 진행
- [x] 폴더 선택적 다운로드

### 시딩

- [x] 방 입장 시 로컬 파일 자동 시딩 활성화
- [x] ChunkRequest 수신 → ChunkResponse 전송 (방 키 암호화)
- [x] 업로드 속도 제한
- [x] 시딩 상태 관리 (활성 / 자동 일시정지 / 수동 일시정지 / 중단)
- [x] 방 퇴장 시 자동 일시정지, 재입장 시 자동 재개
- [x] 수동 일시정지 (`/seed-pause`) 상태는 재입장 시 유지

---

## 친구 (friends)

- [x] `friends.enc` 저장 / 로드
- [x] 자동 친구 등록 (승인자 기준, InviteRequest의 코드 생성자 PeerId 활용)
- [x] 수동 친구 추가 (`/add <번호>`)
- [x] 친구 삭제
- [x] 닉네임 자동 갱신 (ChatMessage 수신 시 최신 닉네임으로 갱신)

---

## TUI (cli)

- [x] ratatui 전체 화면 관리 (매 프레임 전체 재렌더링)
- [x] 피어 식별 표시 — 닉네임 중복 시 `#코드` 자동 추가, `#코드` 중복 시 IP 자동 추가
- [x] 로그인 화면
- [x] 계정 등록 화면
- [x] 계정 삭제 화면
- [x] 메인 메뉴
- [x] 방 만들기 화면 (이름 유효성 검사, 수명 드롭다운)
- [x] 방 목록 화면
  - [x] 캐시 즉시 표시 + 백그라운드 DHT 조회 갱신
  - [x] `확인 중...` / `peers: N` / `오프라인` / `만료됨` 상태 표시
- [x] 초대 코드로 입장 — URL 입력 → 방 선택(복수 시) → 코드 입력 → 승인 대기
- [x] 친구 목록 화면
- [x] 설정 화면 (프로필 / 네트워크 / 채팅 / 파일 / 방 관리 / 친구 관리 / 언어)
- [x] 초대 알림 오버레이 (방 미입장 상태)
- [x] 파일 선택 화면 (선택적 다운로드, 체크박스 트리)
- [x] 채팅/파일 화면 (메인)
  - [x] 상태바 (방 이름, peers, 업/다운 속도, 만료됨, 마지막 동기화)
  - [x] 활성 전송 요약 최대 3줄 (초과 시 요약 + `/downloads` 안내)
  - [x] 통합 피드 (채팅 + 파일 이벤트 + 시스템 알림 + 초대 알림)
  - [x] 입력창 비활성화 처리 (방 수명 만료 시)
  - [x] 피드 스크롤 (입력창 비어있을 때 ↑↓)
  - [x] 명령어 히스토리 탐색 (입력 중 ↑↓)

---

## 앱 코어 (app)

- [x] Task 간 채널 구성 (CLI ↔ App, CLI ↔ Transfer, App ↔ Network, Transfer ↔ Network)
- [x] 이벤트 라우팅 — Network 이벤트 → App / Transfer 분기 처리
- [x] 방 입장/퇴장 시 GossipSub 토픽 구독/해제 연동
- [x] 입력값 유효성 검사 (방 이름, 포트 범위, 경로 등)
- [x] Graceful shutdown — lock 파일 삭제, 방 키 zeroize, 시딩 중단
- [x] 언어 설정 (i18n, 변경 즉시 적용)

---

## 미구현 / 결함 (버그 및 설계 불일치)

> 코드에 직접 주석으로 명시된 결함 및 실제 코드 검토를 통해 확인된 미비 사항.

### 계정 삭제 — 비밀번호 미검증 (HIGH)

- [x] `DeleteAccountState`에 `password` 필드 없음 — 현재 빈 문자열(`b""`)로 `delete_account` 호출 (`main.rs:268`)
  - 영향: `delete_account` 내부에서 `login()`으로 PW 검증을 시도하지만, 빈 PW는 틀린 PW로 처리되어 삭제가 항상 실패함
  - 해결: `DeleteAccountState`에 `id_input`, `pw_input` 필드 추가, `handle_delete_account`에서 입력값을 받아 `TuiAction::DoDeleteAccount { id, password }` 로 전달
  - 관련 doc: `03-account.md` "계정 삭제", `13-decisions.md` D-16

### 닉네임 변경 — config.enc 미저장 (HIGH)

- [x] `AppCommand::ChangeNickname` 처리 시 `self.config.nickname`만 메모리 갱신, `config.enc` 파일에 저장하지 않음 (`app/core.rs:256-266`)
  - 영향: 앱 재시작 시 변경된 닉네임이 사라짐
  - 해결: `self.config.save_with_enc_key()`를 호출해 `config.enc` 저장 추가. `users.json`의 nickname 필드도 `change_nickname()` 호출로 동기화 필요
  - 관련 doc: `03-account.md` "닉네임 변경", `11-settings.md`

### 방 목록 — 만료 방 자동 정리 미실행 (MEDIUM)

- [x] `AppCommand::ListRooms` 처리 시 만료 방 체크·제거 로직 없음 (`app/core.rs:286-292`)
  - 영향: 만료된 방이 목록에 계속 표시됨
  - 해결: `room_store.remove_expired(now_ms)` 호출 추가 후 저장, 제거 수를 `AppEvent::RoomList`에 포함
  - 관련 doc: `05-room.md` "방 수명 만료 체크 — 방 목록 진입 시"

### 채팅 입장 중 1분 간격 만료 체크 미구현 (MEDIUM)

- [x] `AppCore::run()` 이벤트 루프에 채팅 화면 입장 중 1분 주기 만료 타이머 없음 (`app/core.rs:133-174`)
  - 영향: 방 입장 중 만료가 발생해도 즉시 감지 불가
  - 해결: `tokio::time::interval(Duration::from_secs(60))` 타이머 추가, tick마다 `active_room`이 있을 경우 `room_store.get()` + `is_expired()` 체크 후 `AppEvent::RoomExpired` 발송
  - 관련 doc: `05-room.md` "채팅/파일 화면 입장 중 1분 간격", `13-decisions.md` D-07

### RoomStore 공개 생성자 부재 (MEDIUM)

- [x] `RoomStore::new(path)` 공개 생성자 없음 — `main.rs:573-584`에서 빈 파일 생성 후 `load()` 호출하는 우회 코드 사용
  - 영향: 코드 의도 불명확, 빈 파일 생성 실패 시 패닉
  - 해결: `room/store.rs`에 `pub fn new(path: PathBuf) -> Self` 추가
  - 관련 doc: `05-room.md`

### account::NetworkMode — 모듈 간 타입 중복 (MEDIUM)

- [x] `account::config::NetworkMode`와 `network::NetworkMode`가 별도로 존재 — `main.rs:553-569`에서 JSON 직렬화 우회로 변환
  - 영향: 취약한 변환 로직 (`mode_str.contains("internet")` 문자열 비교)
  - 해결: 두 타입 중 하나로 통합하거나, `From<account::NetworkMode> for network::NetworkMode` 구현
  - 관련 doc: `07-network.md`, `11-settings.md`

### FriendStore 초기화 — 존재하지 않는 경로 우회 (LOW)

- [x] `main.rs:118-124`에서 `FriendStore::load()` 실패 시 `.enc.missing` 확장자 경로로 재시도하는 취약 우회 코드
  - 영향: 경로 설계 의도 불명확, `panic!` 도달 가능
  - 해결: `FriendStore::new(path)` 또는 `FriendStore::load_or_default(path, key)` 추가
  - 관련 doc: `04-friends.md`

### 파일 선택 화면 — 다중 파일 동시 다운로드 미구현 (MEDIUM)

- [x] `handle_file_select()`에서 선택된 파일 중 첫 번째만 `StartDownload` 커맨드로 전송 (`tui/input.rs:636-645`)
  - 주석에도 "실제로는 여러 파일 StartDownload 필요"라고 명시됨
  - 영향: 폴더 선택적 다운로드 시 첫 파일만 다운로드됨
  - 해결: 선택된 모든 파일에 대해 반복 `StartDownload` 커맨드 발송, 또는 `AppCommand::StartDownloads { files: Vec<...> }` 추가
  - 관련 doc: `10-file-transfer.md` "폴더 선택적 다운로드", `12-tui.md` "파일 선택 화면"

### 파일 선택 화면 — 폴더 토글 미구현 (LOW)

- [x] `handle_file_select()`의 Space 키 처리에서 `is_dir == true`인 항목 선택 시 하위 항목 일괄 토글 없음 (`tui/input.rs:613-623`)
  - 영향: 폴더 체크 시 개별 파일은 선택되지 않음
  - 해결: `item.is_dir`이면 동일 `depth` 이하의 연속된 자식 항목들의 `selected` 상태도 같이 변경
  - 관련 doc: `12-tui.md` "파일 선택 화면 — 폴더 토글"

### ChunkRequest / BitfieldRequest — AppCore에서 미처리 (MEDIUM)

- [x] `AppCore::handle_inbound_request()`에서 `AppRequest::InviteRequest`만 처리, `ChunkRequest`·`BitfieldRequest`는 "transfer 루프에서 처리 예정" 주석만 있고 실제 라우팅 없음 (`app/core.rs:890-892`)
  - 영향: 파일 청크 요청 수신 불가 → 시딩 기능 전체 미작동
  - 해결: `SeedingManager`에 `ChunkRequest` 처리 메서드 연결, `DownloadManager`에 `BitfieldRequest/Response` 처리 연결

### 다운로드 진행률 — TUI 미반영 (LOW)

- [x] `AppEvent::DownloadProgress` 수신 시 `completed_chunks`, `total_chunks`, `status` 값을 사용하지 않음 (`main.rs:484-488`)
  - 영향: 채팅 화면 상단의 활성 다운로드 요약 바(`active_downloads`)가 갱신되지 않음
  - 해결: `AppEvent::DownloadProgress` 수신 시 `ChatState::active_downloads` 목록 갱신

### 방 목록 — 만료됨 / 오프라인 상태 표시 미연결 (LOW)

- [x] `AppEvent::RoomList`의 `peer_count: None` → `PeerStatus::Checking`만 설정; `PeerStatus::Offline`, `PeerStatus::Expired` 전환 로직 없음 (`main.rs:415-429`)
  - 영향: 방 목록에서 "오프라인" / "만료됨" 상태가 표시되지 않음
  - 해결: `AppCommand::ListRooms` 처리 시 만료 방은 `PeerStatus::Expired`, 등록 후 피어가 0인 방은 DHT 조회 후 `PeerStatus::Offline` 설정

### 초대 거절 — TUI 미구현 (LOW)

- [x] 메인 메뉴 초대 오버레이에서 `d` 키로 거절 가능하나 아무 동작 없음 (`tui/input.rs:191-193`, 주석: "현재 미구현")
  - 해결: `AppCommand::DeclineInvite { number }` 추가 및 `InviteResponse::Declined` 전송 처리

### 비밀번호 변경 UI — 입력 방식 불명확 (LOW)

- [x] 설정 화면의 비밀번호 변경 항목에서 `edit_input`을 `"현재PW:새PW"` 포맷으로 입력해야 하나, UI에서 이 포맷을 사용자에게 안내하지 않음 (`tui/input.rs:553-560`)
  - 해결: 비밀번호 변경 전용 2단계 입력(현재 PW → 새 PW)으로 분리하거나, 입력 힌트 표시

### PID lock 파일 — Graceful shutdown 시 삭제 미처리 (LOW)

- [x] `AppCore::shutdown()`에서 PID 파일 삭제 호출 없음 (`app/core.rs:774-786`)
  - `account::pid` 모듈이 존재하나 shutdown 경로에서 미호출
  - 영향: 정상 종료 후에도 PID 파일이 잔존, 다음 실행 시 stale 감지 불필요하게 발생
  - 해결: `shutdown()` 내에서 `account::pid::release(&self.paths.pid_file(&self.user_id))` 호출
