#![allow(dead_code)]

mod account;
mod app;
mod chat;
mod crypto;
mod friends;
mod i18n;
mod invite;
mod network;
mod protocol;
mod room;
mod transfer;
mod tui;

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::{mpsc, oneshot};

use account::session::{AccountPaths, delete_account, login, recover_stale_tmp, register};
use account::PidLock;
use app::channels::{AppCommand, AppCommandTx, AppEvent, AppEventRx};
use app::core::AppCore;
use friends::FriendStore;
use network::config::NetworkConfig;
use network::event::{NetworkCommand, NetworkEvent};
use network::swarm::{build_swarm, run_event_loop};
use room::RoomStore;
use transfer::{DownloadManager, SeedingManager};
use tui::screen::*;
use tui::{TuiAction, handle_key, render};
use i18n::Lang;

// ── 터미널 초기화 / 복구 ───────────────────────────────────────────────────────

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
}

// ── 기본 데이터 디렉토리 ──────────────────────────────────────────────────────

fn default_data_root() -> PathBuf {
    dirs_home().join(".chatapp")
}

fn dirs_home() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOMEDRIVE").and_then(|d| {
                std::env::var("HOMEPATH").map(|p| format!("{d}{p}"))
            }))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Default"))
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}

// ── 진입점 ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let data_root = default_data_root();
    std::fs::create_dir_all(&data_root).expect("데이터 디렉토리 생성 실패");

    let paths = AccountPaths::new(data_root.clone());

    let mut terminal = setup_terminal().expect("터미널 초기화 실패");

    // ── Phase 1: 로그인 전 화면 (언어는 항상 한국어) ──────────────────────────
    let pre_login_result = run_pre_login(&mut terminal, &paths, &data_root).await;

    let (user_id, identity, config, enc_key) = match pre_login_result {
        Some(r) => r,
        None => {
            cleanup_terminal(&mut terminal);
            return;
        }
    };

    // ── Phase 2: 스웜 + AppCore 시작 ─────────────────────────────────────────
    // 03-account.md: 로그인 후 PID lock 획득 — 정상 종료 시 RAII Drop으로 자동 삭제
    let pid_lock = match PidLock::acquire(&paths.pid_file(&user_id)) {
        Ok(lock) => lock,
        Err(e) => {
            cleanup_terminal(&mut terminal);
            eprintln!("앱이 이미 실행 중입니다: {e}");
            return;
        }
    };

    let net_config = build_network_config(&config);

    // 저장소 로드
    let rooms_path = paths.user_dir(&user_id).join("rooms.enc");
    let friends_path = paths.user_dir(&user_id).join("friends.enc");
    // RoomStore::load: 파일이 없으면 빈 저장소 반환, 복호화 실패 시 빈 저장소로 fallback
    let room_store = RoomStore::load(&rooms_path, &enc_key)
        .unwrap_or_else(|_| RoomStore::new(rooms_path.clone()));
    // FriendStore::load: 파일이 없으면 빈 목록 반환, 복호화 실패 시 빈 저장소로 fallback
    let friend_store = FriendStore::load(&friends_path, &enc_key)
        .unwrap_or_else(|_| FriendStore::new(friends_path.clone()));
    let download_manager = DownloadManager::new(config.max_concurrent_downloads as usize);
    let seeding_manager = SeedingManager::new();

    // 채널 생성
    let (cmd_tx, cmd_rx) = mpsc::channel::<AppCommand>(256);
    let (app_tx, app_rx) = mpsc::channel::<AppEvent>(256);
    let (net_cmd_tx, net_cmd_rx) = mpsc::channel::<NetworkCommand>(256);
    let (net_event_tx, net_event_rx) = mpsc::channel::<NetworkEvent>(256);

    // 스웜 시작
    let swarm = match build_swarm(&identity, &net_config) {
        Ok(s) => s,
        Err(e) => {
            cleanup_terminal(&mut terminal);
            eprintln!("스웜 빌드 실패: {e}");
            return;
        }
    };
    tokio::spawn(run_event_loop(swarm, net_config, net_cmd_rx, net_event_tx));

    // AppCore 시작
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let nickname = config.nickname.clone();
    let app_core = AppCore::new(
        identity,
        config,
        paths.clone(),
        user_id,
        enc_key,
        room_store,
        friend_store,
        download_manager,
        seeding_manager,
        cmd_rx,
        app_tx,
        net_cmd_tx,
        net_event_rx,
    );
    tokio::spawn(app_core.run(shutdown_rx));

    // ── Phase 3: 메인 TUI 루프 ────────────────────────────────────────────────
    run_tui_loop(&mut terminal, cmd_tx, app_rx, &nickname).await;

    // ── 종료 ──────────────────────────────────────────────────────────────────
    shutdown_tx.send(()).ok();
    // pid_lock을 명시적으로 여기까지 보존하여 정상 종료 시 RAII Drop이 PID 파일을 삭제
    drop(pid_lock);
    cleanup_terminal(&mut terminal);
}

// ── Phase 1: 로그인 전 TUI ────────────────────────────────────────────────────

/// 로그인/등록/삭제 화면을 처리하고 성공 시 (user_id, Identity, Config, enc_key)를 반환.
async fn run_pre_login(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    paths: &AccountPaths,
    data_root: &PathBuf,
) -> Option<(String, account::Identity, account::Config, [u8; 32])> {
    let mut screen = Screen::Welcome(WelcomeState::default());

    loop {
        // 로그인 전은 항상 한국어
        terminal.draw(|f| render(f, &screen, Lang::Korean)).ok();

        // 키 입력 대기 (100ms 타임아웃)
        if !event::poll(Duration::from_millis(100)).unwrap_or(false) {
            continue;
        }

        let ev = match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => k,
            _ => continue,
        };

        // 화면별 특수 처리 (screen 전환이 필요한 경우)
        match &screen {
            Screen::Welcome(_) => {
                let action = handle_key(&mut screen, ev);
                match action {
                    TuiAction::Quit => return None,
                    TuiAction::Goto(new_screen) => { screen = new_screen; }
                    _ => {}
                }
            }
            Screen::Login(_) => {
                let action = handle_key(&mut screen, ev);
                match action {
                    TuiAction::Quit => return None,
                    TuiAction::Goto(new_screen) => {
                        screen = new_screen;
                    }
                    TuiAction::DoLogin { id, password } => {
                        let pw_bytes = password.as_bytes();
                        match login(paths, &id, pw_bytes) {
                            Ok((identity, config)) => {
                                // Argon2 파생 키 생성
                                let store = account::UserStore::load(&paths.users_json()).ok()?;
                                let record = store.find(&id)?;
                                let salt = hex_decode(&record.salt_hex)?;
                                let enc_key_zeroize = crypto::derive_key(pw_bytes, &salt).ok()?;
                                let enc_key: [u8; 32] = *enc_key_zeroize;
                                // 크래시 복구
                                recover_stale_tmp(paths, &id);
                                return Some((id, identity, config, enc_key));
                            }
                            Err(e) => {
                                if let Screen::Login(s) = &mut screen {
                                    s.error = Some(format!("로그인 실패: {e}"));
                                    s.pw_input.clear();
                                }
                            }
                        }
                    }
                    TuiAction::Command(AppCommand::Shutdown) => return None,
                    _ => {}
                }
            }
            Screen::Register(_) => {
                let action = handle_key(&mut screen, ev);
                match action {
                    TuiAction::DoRegister { id, nickname, password } => {
                        let pw_bytes = password.as_bytes();
                        let dl_path = data_root.join("downloads").to_string_lossy().to_string();
                        let log_path = data_root.join("users").join(&id).join("logs").to_string_lossy().to_string();
                        std::fs::create_dir_all(data_root.join("users").join(&id).join("logs")).ok();
                        match register(paths, &id, &nickname, pw_bytes, &dl_path, &log_path) {
                            Ok(()) => {
                                // 등록 완료 → 로그인 화면으로 (ID 미리 채움)
                                screen = Screen::Login(LoginState {
                                    id_input: id,
                                    error: Some("등록 완료! 비밀번호를 입력하고 Enter 로그인.".into()),
                                    ..Default::default()
                                });
                            }
                            Err(e) => {
                                if let Screen::Register(s) = &mut screen {
                                    s.error = Some(format!("등록 실패: {e}"));
                                }
                            }
                        }
                    }
                    TuiAction::Goto(new_screen) => { screen = new_screen; }
                    TuiAction::Command(AppCommand::Shutdown) | TuiAction::Quit => {
                        screen = Screen::Welcome(WelcomeState::default());
                    }
                    _ => {}
                }
            }
            Screen::DeleteAccount(_) => {
                let action = handle_key(&mut screen, ev);
                match action {
                    TuiAction::DoDeleteAccount { id, password } => {
                        match delete_account(paths, &id, password.as_bytes()) {
                            Ok(()) => {
                                screen = Screen::Welcome(WelcomeState {
                                    message: Some("계정이 삭제되었습니다.".into()),
                                });
                            }
                            Err(e) => {
                                if let Screen::DeleteAccount(s) = &mut screen {
                                    s.error = Some(format!("삭제 실패: {e}"));
                                    s.pw_input.clear();
                                }
                            }
                        }
                    }
                    TuiAction::Goto(new_screen) => {
                        screen = new_screen;
                    }
                    TuiAction::Command(AppCommand::Shutdown) | TuiAction::Quit => {
                        screen = Screen::Welcome(WelcomeState::default());
                    }
                    _ => {}
                }
            }
            _ => break, // 예상치 못한 화면 — 루프 탈출
        }
    }
    None
}


// ── Phase 3: 메인 TUI 루프 ────────────────────────────────────────────────────

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    cmd_tx: AppCommandTx,
    mut app_rx: AppEventRx,
    nickname: &str,
) {
    let mut screen = Screen::MainMenu(MainMenuState {
        nickname: nickname.to_string(),
        ..Default::default()
    });
    let mut lang = Lang::Korean;

    loop {
        // 렌더링
        terminal.draw(|f| render(f, &screen, lang)).ok();

        // InviteEntry TTL 카운트다운 갱신
        if let Screen::InviteEntry(s) = &mut screen {
            if s.step == InviteStep::Waiting && s.waiting_start_ms > 0 {
                let now = crate::room::RoomStore::now_ms();
                let elapsed = now.saturating_sub(s.waiting_start_ms);
                s.ttl_remaining_ms = (3 * 60 * 1000u64).saturating_sub(elapsed);
            }
        }

        // AppEvent 비동기 수신 (non-blocking)
        while let Ok(event) = app_rx.try_recv() {
            // ConfigSnapshot에서 언어 설정 추출
            if let AppEvent::ConfigSnapshot { ref language, .. } = event {
                lang = if language == "English" { Lang::English } else { Lang::Korean };
            }
            handle_app_event(&mut screen, event);
        }

        // 키 입력 (50ms 타임아웃)
        if !event::poll(Duration::from_millis(50)).unwrap_or(false) {
            // AppEvent만 처리하고 계속
            continue;
        }

        let ev = match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => k,
            _ => continue,
        };

        let action = handle_key(&mut screen, ev);
        match action {
            TuiAction::Command(cmd) => {
                let quit = matches!(cmd, AppCommand::Shutdown);
                let is_invite_code = matches!(cmd, AppCommand::EnterInviteCode { .. });
                cmd_tx.send(cmd).await.ok();
                if is_invite_code {
                    if let Screen::InviteEntry(s) = &mut screen {
                        s.waiting_start_ms = crate::room::RoomStore::now_ms();
                    }
                }
                if quit { break; }
            }
            TuiAction::Goto(new_screen) => {
                screen = new_screen;
            }
            TuiAction::CommandAndGoto(cmd, new_screen) => {
                let quit = matches!(cmd, AppCommand::Shutdown);
                cmd_tx.send(cmd).await.ok();
                screen = new_screen;
                if quit { break; }
            }
            TuiAction::Quit => {
                cmd_tx.send(AppCommand::Shutdown).await.ok();
                break;
            }
            TuiAction::DoLogin { .. } | TuiAction::DoRegister { .. } | TuiAction::DoDeleteAccount { .. } => {
                // 메인 루프에서는 처리 불필요 (로그인 전 화면에서만 사용)
            }
            TuiAction::None => {}
        }
    }
}

/// AppEvent를 수신해 Screen 상태를 갱신한다.
fn handle_app_event(screen: &mut Screen, event: AppEvent) {
    match event {
        AppEvent::FeedEntry(entry) => {
            if let Screen::Chat(s) = screen {
                use crate::tui::screen::{FeedContent, FeedItem};
                use crate::chat::log::LogEntryKind;
                let content = match entry.kind {
                    LogEntryKind::Chat { sender_nickname, sender_peer_short, text } => {
                        FeedContent::Chat {
                            peer_display: format!("{sender_nickname}#{sender_peer_short}"),
                            text,
                        }
                    }
                    LogEntryKind::FileEvent { message } => FeedContent::FileEvent(message),
                    LogEntryKind::System { message } => FeedContent::System(message),
                };
                s.feed.push(FeedItem {
                    timestamp_ms: entry.timestamp_ms,
                    content,
                });
                // 자동 스크롤 (피드 끝에 도달 시)
                s.feed_scroll = s.feed.len().saturating_sub(1);
            }
        }

        AppEvent::JoinedRoom { room_id, name } => {
            *screen = Screen::Chat(ChatState {
                room_id,
                room_name: name,
                ..Default::default()
            });
        }

        AppEvent::LeftRoom => {
            *screen = Screen::MainMenu(MainMenuState::default());
        }

        AppEvent::RoomExpired => {
            if let Screen::Chat(s) = screen {
                s.expired = true;
                s.input_disabled = true;
            }
        }

        AppEvent::PeerJoined { peer_id, .. } => {
            if let Screen::Chat(s) = screen {
                s.peer_count += 1;
                let _ = peer_id;
            }
        }

        AppEvent::PeerLeft { .. } => {
            if let Screen::Chat(s) = screen {
                s.peer_count = s.peer_count.saturating_sub(1);
            }
        }

        AppEvent::RoomList { rooms } => {
            if let Screen::RoomList(s) = screen {
                s.rooms = rooms.into_iter().map(|(room_id, name, peer_count)| {
                    RoomListEntry {
                        room_id,
                        name,
                        peer_status: match peer_count {
                            Some(0) => PeerStatus::Offline,
                            Some(n) => PeerStatus::Online(n),
                            None => PeerStatus::Checking,
                        },
                        lifetime_display: String::new(),
                    }
                }).collect();
            }
        }

        AppEvent::RoomPeerCount { room_id, count } => {
            // 05-room.md: 배경 DHT GetProviders 완료 → 방 목록 피어 수 갱신
            if let Screen::RoomList(s) = screen {
                if let Some(entry) = s.rooms.iter_mut().find(|e| e.room_id == room_id) {
                    entry.peer_status = if count == 0 {
                        PeerStatus::Offline
                    } else {
                        PeerStatus::Online(count)
                    };
                }
            }
        }

        AppEvent::InviteCodeGenerated { code, my_id } => {
            if let Screen::Chat(s) = screen {
                use crate::room::RoomStore;
                s.feed.push(FeedItem {
                    timestamp_ms: RoomStore::now_ms(),
                    content: FeedContent::System(
                        format!("[초대 코드] {code}  |  내 ID: {my_id}  (3분간 유효)")
                    ),
                });
            }
        }

        AppEvent::InviteReceived { from_nickname, room_name, number, from_peer } => {
            match screen {
                Screen::Chat(s) => {
                    s.pending_invites.push(PendingInviteInfo {
                        from_peer,
                        from_display: from_nickname,
                        room_name,
                        number,
                    });
                    s.show_invite_overlay = true;
                }
                Screen::MainMenu(s) => {
                    s.pending_invites.push(PendingInviteInfo {
                        from_peer,
                        from_display: from_nickname,
                        room_name,
                        number,
                    });
                    s.show_invite_overlay = true;
                }
                _ => {}
            }
        }

        AppEvent::FileAnnounced { announce } => {
            if let Screen::Chat(s) = screen {
                use crate::room::RoomStore;
                s.feed.push(FeedItem {
                    timestamp_ms: RoomStore::now_ms(),
                    content: FeedContent::FileEvent(
                        format!("[파일] {} 공유됨 ({})", announce.name, format_size(announce.total_size))
                    ),
                });
            }
        }

        AppEvent::UrlRooms { rooms } => {
            if let Screen::InviteEntry(s) = screen {
                if rooms.len() == 1 {
                    s.selected_room = Some(rooms[0].0);
                    s.step = InviteStep::CodeInput;
                } else {
                    s.room_candidates = rooms;
                    s.room_cursor = 0;
                    s.step = InviteStep::RoomSelect;
                }
            }
        }

        AppEvent::UrlNotFound => {
            if let Screen::InviteEntry(s) = screen {
                s.step = InviteStep::Failed(
                    format!("'{}' 을(를) 찾을 수 없습니다.\n상대방이 초대 코드를 먼저 생성해야 합니다.", s.url_input)
                );
            }
        }

        AppEvent::Error(msg) => {
            if let Screen::InviteEntry(s) = screen {
                s.step = InviteStep::Failed(msg);
            } else {
                add_system_feed(screen, format!("! {msg}"));
            }
        }

        AppEvent::Notice(msg) => {
            if let Screen::InviteEntry(s) = screen {
                s.error = Some(msg);
            } else {
                add_system_feed(screen, msg);
            }
        }

        AppEvent::DownloadProgress { file_hash, completed_chunks, total_chunks, status } => {
            // 채팅 화면 상단 활성 다운로드 요약 바 갱신 (12-tui.md)
            if let Screen::Chat(s) = screen {
                use crate::tui::screen::DownloadSummary;
                let pct = if total_chunks > 0 {
                    completed_chunks as f32 / total_chunks as f32 * 100.0
                } else {
                    0.0
                };
                // 기존 항목 갱신 또는 새 항목 추가 (최대 3개 표시)
                if let Some(entry) = s.active_downloads.iter_mut().find(|d| {
                    // file_hash로 매핑할 이름을 찾기 어려우므로 상태로 찾음
                    d.pct < 100.0 && matches!(d.status, crate::transfer::DownloadStatus::Active)
                }) {
                    entry.pct = pct;
                    entry.status = status;
                } else if s.active_downloads.len() < 3 {
                    s.active_downloads.push(DownloadSummary {
                        file_name: format!("{}", hex_short(&file_hash)),
                        pct,
                        bps: 0,
                        status,
                    });
                }
            }
        }

        AppEvent::DownloadComplete { file_name, .. } => {
            add_system_feed(screen, format!("[✓] '{}' 다운로드 완료", file_name));
        }

        AppEvent::PeerList { peers } => {
            if peers.is_empty() {
                add_system_feed(screen, "접속 피어: 0명 (mDNS 탐색된 로컬 피어 없음)".into());
            } else {
                add_system_feed(screen, format!("접속 피어: {}명", peers.len()));
                for (id, addr) in &peers {
                    let id_str = id.to_string();
                    let short = &id_str[id_str.len().saturating_sub(12)..];
                    add_system_feed(screen, format!("  ...{} ({})", short, addr));
                }
            }
        }

        AppEvent::FileRemoved { .. } => {
            // router.rs에서 이미 FeedEntry로 "파일 공유 철회됨"을 표시함
        }

        AppEvent::InviteDecision { accepted, by_peer } => {
            let id_str = by_peer.to_string();
            let short = &id_str[id_str.len().saturating_sub(12)..];
            if accepted {
                add_system_feed(screen, format!("[초대] ...{} 님이 초대를 수락했습니다.", short));
            } else {
                add_system_feed(screen, format!("[초대] ...{} 님이 초대를 거절했습니다.", short));
            }
        }

        AppEvent::ConfigSnapshot {
            user_id, nickname, network_mode, port, max_connections,
            download_path, max_concurrent_dl, max_upload_kbps, max_download_kbps,
            log_path, language,
        } => {
            if let Screen::Settings(s) = screen {
                s.config.user_id = user_id;
                s.config.nickname = nickname;
                s.config.network_mode = network_mode;
                s.config.port = port;
                s.config.max_connections = max_connections;
                s.config.download_path = download_path;
                s.config.max_concurrent_dl = max_concurrent_dl;
                s.config.max_upload_kbps = max_upload_kbps;
                s.config.max_download_kbps = max_download_kbps;
                s.config.log_path = log_path;
                s.config.language = language;
            }
        }

        _ => {} // LoginSuccess/Failed 등 로그인 전 이벤트 무시
    }
}

fn add_system_feed(screen: &mut Screen, msg: String) {
    if let Screen::Chat(s) = screen {
        use crate::room::RoomStore;
        s.feed.push(FeedItem {
            timestamp_ms: RoomStore::now_ms(),
            content: FeedContent::System(msg),
        });
    }
}

// ── NetworkConfig 변환 ────────────────────────────────────────────────────────

fn build_network_config(config: &account::Config) -> NetworkConfig {
    // account::NetworkMode → network::NetworkMode 타입 안전 변환 (From 구현 사용)
    use network::config::NetworkMode as NetMode;
    let net_mode = NetMode::from(&config.network_mode);
    let mut nc = match net_mode {
        NetMode::Internet => NetworkConfig::internet_default(config.port),
        NetMode::Intranet => NetworkConfig::intranet_default(config.port),
    };
    nc.max_connections = config.max_connections;
    nc
}

// ── 헬퍼 ──────────────────────────────────────────────────────────────────────


fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 { return None; }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

fn hex_short(hash: &[u8; 32]) -> String {
    hash.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
