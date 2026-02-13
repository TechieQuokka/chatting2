# 프로젝트 개요

## 목적

회사 내/외에서 소규모부터 대규모(100명 이상도 지원) 그룹이 사용하는 P2P 파일 공유 + 채팅 CLI 도구.
핵심 목적은 **데이터 송수신**이며, 채팅은 보조 기능이다.

## 기술 스택

| 항목 | 내용 |
|------|------|
| 언어 | Rust |
| UI | ratatui 기반 TUI |
| 네트워크 | libp2p 기반 완전 P2P |
| 파일 공유 | 토렌트 방식 (청크 분할, 병렬 전송, Rarest-first) |

## 핵심 원칙

- 완전 탈중앙 P2P — 방장 없음, 서버 없음
- 네트워크를 뚫어도 키 없이는 데이터를 읽을 수 없음
- 오직 초대 기반으로만 방 접근 가능

## 아키텍처

```
CLI ←→ App ←→ Network
 ↕      ↕        ↕
 └─── Transfer ──┘
```

| Task | 역할 |
|------|------|
| CLI | 사용자 입력, TUI 렌더링 |
| App | 채팅, 방 관리, 설정 (파일 공유 명령은 Transfer에 위임) |
| Transfer | 다운로드 매니저, 시딩, 청크 스케줄링 (Network와 직접 통신) |
| Network | libp2p swarm 구동 |

- App과 Transfer는 독립적으로 Network와 통신
- Transfer 진행 상황은 CLI에 직접 전달
- CLI 파일 공유 명령은 Transfer로 전달

## 모듈 구조

| 영역 | 모듈 | 설명 |
|------|------|------|
| 계정/인증 | account | 계정 등록, 로그인, 삭제, PW 변경 |
| | identity | Ed25519 키쌍, PeerId, 닉네임 |
| | config | 사용자별 암호화 config 관리 |
| 네트워크 | network | libp2p swarm, GossipSub, mDNS, Kademlia |
| | protocol | request-response 프로토콜 정의 |
| 채팅 | room | 방 상태, 입장/퇴장 |
| | chat | 채팅 메시지 처리 |
| 파일 공유 | file_meta | 파일 메타데이터, 청크 분할, 해시 계산 |
| | transfer | 다운로드 매니저, 청크 스케줄링, 병렬 다운로드 |
| | seeding | 시딩 상태 관리 |
| | bitfield | 피어별 청크 보유 현황 관리 |
| 소셜 | friends | 친구 목록 관리 |
| | invite | 초대 코드 생성, 수락/거절 |
| 앱 코어 | app | 전체 오케스트레이션 |
| | types | 공유 타입, 메시지 정의 |
| | validation | 방 이름, 입력값, 프로토콜 메시지 유효성 검사 |
| UI | cli | TUI 화면 |
| | logger | 채팅 로그 저장 |
| 암호화 | crypto | AES-256-GCM, Argon2id, config 암호화 |
