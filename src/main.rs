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
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::{mpsc, oneshot};

use account::session::{AccountPaths, delete_account, login, recover_stale_tmp, register};
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

    // ── Phase 1: 로그인 전 화면 ────────────────────────────────────────────────
    let pre_login_result = run_pre_login(&mut terminal, &paths, &data_root).await;

    let (user_id, identity, config, enc_key) = match pre_login_result {
        Some(r) => r,
        None => {
            cleanup_terminal(&mut terminal);
            return;
        }
    };

    // ── Phase 2: 스웜 + AppCore 시작 ─────────────────────────────────────────
    let net_config = build_network_config(&config);

    // 저장소 로드
    let rooms_path = paths.user_dir(&user_id).join("rooms.enc");
    let friends_path = paths.user_dir(&user_id).join("friends.enc");
    let room_store = RoomStore::load(&rooms_path, &enc_key).unwrap_or_else(|_| {
        RoomStore::load(&rooms_path, &enc_key).unwrap_or_else(|_| {
            // 파일이 없거나 복호화 실패 시 빈 저장소
            RoomStore::load(&paths.user_dir(&user_id).join("nonexistent_rooms.enc"), &enc_key)
                .unwrap_or_else(|_| new_empty_room_store(rooms_path.clone(), &enc_key))
        })
    });
    // FriendStore::load는 파일이 없으면 빈 목록을 반환하므로 직접 load 사용.
    // 복호화 실패 시(잘못된 키) 빈 목록으로 fallback.
    let friend_store = FriendStore::load(&friends_path, &enc_key)
        .unwrap_or_else(|_| {
            // 파일이 없거나 복호화 실패 — 빈 저장소 생성을 위해 존재하지 않는 경로로 load
            let empty_path = friends_path.with_extension("enc.missing");
            FriendStore::load(&empty_path, &enc_key)
                .unwrap_or_else(|_| panic!("FriendStore 초기화 실패"))
        });
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
    cleanup_terminal(&mut terminal);
}

// ── Phase 1: 로그인 전 TUI ────────────────────────────────────────────────────

/// 로그인/등록/삭제 화면을 처리하고 성공 시 (user_id, Identity, Config, enc_key)를 반환.
async fn run_pre_login(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    paths: &AccountPaths,
    data_root: &PathBuf,
) -> Option<(String, account::Identity, account::Config, [u8; 32])> {
    let mut screen = Screen::Login(LoginState::default());

    loop {
        // 렌더링
        terminal.draw(|f| render(f, &screen)).ok();

        // 키 입력 대기 (100ms 타임아웃)
        if !event::poll(Duration::from_millis(100)).unwrap_or(false) {
            continue;
        }

        let ev = match event::read() {
            Ok(Event::Key(k)) => k,
            _ => continue,
        };

        // 화면별 특수 처리 (screen 전환이 필요한 경우)
        match &screen {
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
                                screen = Screen::Login(LoginState {
                                    id_input: id,
                                    error: Some("등록 완료! 로그인하세요.".into()),
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
                    TuiAction::Command(AppCommand::Shutdown) | TuiAction::Quit => {
                        screen = Screen::Login(LoginState::default());
                    }
                    _ => {}
                }
            }
            Screen::DeleteAccount(_) => {
                let action = handle_key(&mut screen, ev);
                match action {
                    TuiAction::DoDeleteAccount { id } => {
                        // 비밀번호 재확인 없이 삭제 (현재 구조상 PW 없이 삭제 불가 — 설계 결함)
                        // [논리적 결함 보고] DeleteAccountState에 password 필드 없음
                        // 임시: 삭제 확인만 하고 PW는 빈 문자열 시도
                        match delete_account(paths, &id, b"") {
                            Ok(()) => {
                                screen = Screen::Login(LoginState {
                                    error: Some("계정 삭제됨".into()),
                                    ..Default::default()
                                });
                            }
                            Err(_) => {
                                screen = Screen::Login(LoginState {
                                    error: Some("계정 삭제 실패 (비밀번호 확인 필요)".into()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    TuiAction::Command(AppCommand::Shutdown) | TuiAction::Quit => {
                        screen = Screen::Login(LoginState::default());
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

    loop {
        // 렌더링
        terminal.draw(|f| render(f, &screen)).ok();

        // AppEvent 비동기 수신 (non-blocking)
        while let Ok(event) = app_rx.try_recv() {
            handle_app_event(&mut screen, event);
        }

        // 키 입력 (50ms 타임아웃)
        if !event::poll(Duration::from_millis(50)).unwrap_or(false) {
            // AppEvent만 처리하고 계속
            continue;
        }

        let ev = match event::read() {
            Ok(Event::Key(k)) => k,
            _ => continue,
        };

        let action = handle_key(&mut screen, ev);
        match action {
            TuiAction::Command(cmd) => {
                let quit = matches!(cmd, AppCommand::Shutdown);
                cmd_tx.send(cmd).await.ok();
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
                            Some(n) => PeerStatus::Online(n),
                            None => PeerStatus::Checking,
                        },
                        lifetime_display: String::new(),
                    }
                }).collect();
            }
        }

        AppEvent::InviteCodeGenerated { code } => {
            if let Screen::Chat(s) = screen {
                use crate::room::RoomStore;
                s.feed.push(FeedItem {
                    timestamp_ms: RoomStore::now_ms(),
                    content: FeedContent::System(format!("[초대 코드] {code} (3분간 유효)")),
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

        AppEvent::Error(msg) => {
            add_system_feed(screen, format!("! {msg}"));
        }

        AppEvent::Notice(msg) => {
            add_system_feed(screen, msg);
        }

        AppEvent::DownloadProgress { file_hash: _, completed_chunks, total_chunks, status } => {
            // 활성 다운로드 요약 갱신은 실제로 DownloadManager 상태를 직접 읽어야 함.
            // 여기서는 진행률 계산만 반영.
            let _ = (completed_chunks, total_chunks, status);
        }

        AppEvent::DownloadComplete { file_name, .. } => {
            add_system_feed(screen, format!("[✓] '{}' 다운로드 완료", file_name));
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
    // account::Config의 NetworkMode와 network::NetworkConfig 간 변환.
    // account::config::NetworkMode는 private이므로 문자열 직렬화/역직렬화로 구분.
    // [논리적 결함 보고] account::Config의 NetworkMode를 pub으로 노출하거나
    //   network::NetworkMode로 통합하는 것이 더 명확함.
    let mode_str = serde_json::to_string(&config.network_mode)
        .unwrap_or_else(|_| "\"internet\"".to_string());
    if mode_str.contains("internet") {
        let mut nc = NetworkConfig::internet_default(config.port);
        nc.max_connections = config.max_connections;
        nc
    } else {
        let mut nc = NetworkConfig::intranet_default(config.port);
        nc.max_connections = config.max_connections;
        nc
    }
}

// ── 헬퍼 ──────────────────────────────────────────────────────────────────────

fn new_empty_room_store(path: PathBuf, _key: &[u8; 32]) -> RoomStore {
    // 파일이 없을 때 빈 RoomStore 생성.
    // RoomStore::load가 존재하지 않는 경로에 대해 빈 저장소를 반환하므로
    // 임시 경로로 load 후 실제 경로로 재설정할 수 없어
    // 직접 생성 가능한 공개 생성자가 필요하지만 현재 없음.
    // 임시 해결: 빈 파일을 먼저 생성하고 load.
    // [논리적 결함 보고] RoomStore에 new(path) 생성자 없음 — load만 공개됨
    let _ = std::fs::File::create(&path); // 빈 파일 생성 (load 실패 방지)
    RoomStore::load(&path, _key).unwrap_or_else(|_| {
        // 최후 수단: 패닉 대신 프로세스 종료
        panic!("rooms.enc 생성 실패")
    })
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 { return None; }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
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
