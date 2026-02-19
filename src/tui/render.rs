use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::i18n::{Key, Lang, t};

use super::screen::*;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── 라우터 ───────────────────────────────────────────────────────────────────

pub fn render(frame: &mut Frame, screen: &Screen, lang: Lang) {
    match screen {
        Screen::Welcome(s) => render_welcome(frame, s, lang),
        Screen::Login(s) => render_login(frame, s, lang),
        Screen::Register(s) => render_register(frame, s, lang),
        Screen::DeleteAccount(s) => render_delete_account(frame, s, lang),
        Screen::MainMenu(s) => render_main_menu(frame, s, lang),
        Screen::RoomList(s) => render_room_list(frame, s, lang),
        Screen::CreateRoom(s) => render_create_room(frame, s, lang),
        Screen::InviteEntry(s) => render_invite_entry(frame, s, lang),
        Screen::FriendList(s) => render_friend_list(frame, s, lang),
        Screen::Settings(s) => render_settings(frame, s, lang),
        Screen::FileSelect(s) => render_file_select(frame, s, lang),
        Screen::Chat(s) => render_chat(frame, s, lang),
    }
}

// ── 공통 헬퍼 ────────────────────────────────────────────────────────────────

fn center_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn error_line(err: &str) -> Paragraph<'_> {
    Paragraph::new(Span::styled(
        format!("! {err}"),
        Style::default().fg(Color::Red),
    ))
}

// ── 시작 화면 ────────────────────────────────────────────────────────────────

fn render_welcome(frame: &mut Frame, state: &WelcomeState, lang: Lang) {
    let area = center_rect(42, 14, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" FileTalk");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 헤더
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // [1]
            Constraint::Length(1), // [2]
            Constraint::Length(1), // [3]
            Constraint::Length(1), // [Q]
            Constraint::Min(0),    // 여백
            Constraint::Length(1), // 메시지
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("P2P Chat & File Transfer  v{APP_VERSION}")),
        chunks[0],
    );

    let menu = [
        format!("[1] {}", t(lang, Key::Login)),
        format!("[2] {}", t(lang, Key::Register)),
        format!("[3] {}", t(lang, Key::DeleteAccount)),
        format!("[Q] {}", t(lang, Key::Quit)),
    ];
    for (i, item) in menu.iter().enumerate() {
        frame.render_widget(Paragraph::new(item.as_str()), chunks[2 + i]);
    }

    if let Some(msg) = &state.message {
        frame.render_widget(
            Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(Color::Green))),
            chunks[7],
        );
    }
}

// ── 로그인 화면 ───────────────────────────────────────────────────────────────

fn render_login(frame: &mut Frame, state: &LoginState, lang: Lang) {
    let title = format!(" FileTalk — {} ", t(lang, Key::Login));
    let area = center_rect(42, 10, frame.area());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // ID
            Constraint::Length(1), // PW
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // 힌트
            Constraint::Min(0),    // 여백
            Constraint::Length(1), // 오류
        ])
        .split(inner);

    let id_style = if state.focused == LoginField::Id {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let pw_style = if state.focused == LoginField::Pw {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    frame.render_widget(
        Paragraph::new(Span::styled(format!("{} : [{}]", t(lang, Key::Id), state.id_input), id_style)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{} : [{}]", t(lang, Key::Password), "*".repeat(state.pw_input.len())),
            pw_style,
        )),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(t(lang, Key::HintTabFocusEnterLoginEscBack)),
        chunks[3],
    );

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[5]);
    }
}

// ── 계정 등록 화면 ────────────────────────────────────────────────────────────

fn render_register(frame: &mut Frame, state: &RegisterState, lang: Lang) {
    let title = format!(" {} ", t(lang, Key::Register));
    let area = center_rect(46, 14, frame.area());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let id_lbl = t(lang, Key::Id);
    let nick_lbl = t(lang, Key::Nickname);
    let pw_lbl = t(lang, Key::Password);
    let pwc_lbl = t(lang, Key::PasswordConfirm);

    frame.render_widget(Paragraph::new(format!("{id_lbl:<16} : [{}]", state.id_input)), chunks[0]);
    frame.render_widget(Paragraph::new(format!("{nick_lbl:<16} : [{}]", state.nickname_input)), chunks[1]);
    frame.render_widget(
        Paragraph::new(format!("{pw_lbl:<16} : [{}]", "*".repeat(state.pw_input.len()))),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(format!("{pwc_lbl:<16} : [{}]", "*".repeat(state.pw_confirm.len()))),
        chunks[3],
    );
    frame.render_widget(Paragraph::new(t(lang, Key::HintTabNextEnterRegisterEscCancel)), chunks[4]);

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[6]);
    }
}

// ── 계정 삭제 화면 ────────────────────────────────────────────────────────────

fn render_delete_account(frame: &mut Frame, state: &DeleteAccountState, lang: Lang) {
    use super::screen::DeleteField;

    let title = format!(" {} ", t(lang, Key::DeleteAccount));
    let area = center_rect(48, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 경고
            Constraint::Length(1), // 경고2
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // ID
            Constraint::Length(1), // PW
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // 안내
            Constraint::Length(1), // 오류
        ])
        .split(inner);

    let warn1 = if lang == Lang::English {
        "! This operation cannot be undone."
    } else {
        "! 이 작업은 되돌릴 수 없습니다."
    };
    let warn2 = if lang == Lang::English {
        "(Downloaded files are kept)"
    } else {
        "(다운로드 파일은 유지됩니다)"
    };

    frame.render_widget(
        Paragraph::new(Span::styled(warn1, Style::default().fg(Color::Yellow))),
        chunks[0],
    );
    frame.render_widget(Paragraph::new(warn2), chunks[1]);

    let id_style = if state.focused == DeleteField::Id {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{} : [{}]", t(lang, Key::Id), state.id_input),
            id_style,
        )),
        chunks[3],
    );

    let pw_style = if state.focused == DeleteField::Pw {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{} : [{}]", t(lang, Key::Password), "*".repeat(state.pw_input.len())),
            pw_style,
        )),
        chunks[4],
    );

    frame.render_widget(
        Paragraph::new(t(lang, Key::HintTabMoveEnterDeleteConfirm)),
        chunks[6],
    );

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[7]);
    }
}

// ── 메인 메뉴 ─────────────────────────────────────────────────────────────────

fn render_main_menu(frame: &mut Frame, state: &MainMenuState, lang: Lang) {
    let area = center_rect(44, 14, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" FileTalk");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let greeting = if lang == Lang::English {
        format!("Hello, {}", state.nickname)
    } else {
        format!("안녕하세요, {}님", state.nickname)
    };
    frame.render_widget(Paragraph::new(greeting), chunks[0]);

    let items = [
        format!("[1] {}", t(lang, Key::CreateRoom)),
        format!("[2] {}", t(lang, Key::RoomList)),
        format!("[3] {}", t(lang, Key::JoinByInvite)),
        format!("[4] {}", t(lang, Key::FriendList)),
        format!("[5] {}", t(lang, Key::Settings)),
        format!("[Q] {}", t(lang, Key::Quit)),
    ];
    for (i, item) in items.iter().enumerate() {
        frame.render_widget(Paragraph::new(item.as_str()), chunks[2 + i]);
    }

    if state.show_invite_overlay && !state.pending_invites.is_empty() {
        render_invite_overlay(frame, &state.pending_invites, state.invite_cursor, lang);
    }
}

fn render_invite_overlay(frame: &mut Frame, invites: &[PendingInviteInfo], cursor: usize, lang: Lang) {
    let title = if lang == Lang::English {
        format!(" Invites ({}) ", invites.len())
    } else {
        format!(" 초대 알림 ({건}) ", 건 = invites.len())
    };
    let area = center_rect(48, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    let items: Vec<ListItem> = invites
        .iter()
        .enumerate()
        .map(|(i, inv)| {
            let marker = if i == cursor { "▶" } else { " " };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{marker} {}. {} → {}", i + 1, inv.from_display, inv.room_name)),
            ]))
        })
        .collect();

    let list = List::new(items);
    let list_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    frame.render_widget(list, list_rect);

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(
        Paragraph::new(t(lang, Key::HintAcceptRejectLater)),
        hint_rect,
    );
}

// ── 방 목록 화면 ─────────────────────────────────────────────────────────────

fn render_room_list(frame: &mut Frame, state: &RoomListState, lang: Lang) {
    let title = format!(" {} ", t(lang, Key::RoomList));
    let area = frame.area();
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    if state.expired_cleaned > 0 {
        let msg = if lang == Lang::English {
            format!("[Notice] {} expired room(s) cleaned up.", state.expired_cleaned)
        } else {
            format!("[알림] 만료된 방 {}개가 정리됐습니다.", state.expired_cleaned)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(Color::Yellow))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    let list_y = inner.y + if state.expired_cleaned > 0 { 1 } else { 0 };
    let list_h = inner.height.saturating_sub(2);

    let items: Vec<ListItem> = state.rooms.iter().enumerate().map(|(i, room)| {
        let marker = if i == state.cursor { "▶" } else { " " };
        let peer_str = match &room.peer_status {
            PeerStatus::Checking => t(lang, Key::Checking).to_string(),
            PeerStatus::Online(n) => format!("peers: {n}"),
            PeerStatus::Offline => t(lang, Key::Offline).to_string(),
            PeerStatus::Expired => t(lang, Key::Expired).to_string(),
        };
        ListItem::new(format!("{marker} {:<22} {:<14} {}", room.name, peer_str, room.lifetime_display))
    }).collect();

    let list = List::new(items);
    frame.render_widget(list, Rect::new(inner.x, list_y, inner.width, list_h));

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    if let Some(idx) = state.confirm_delete {
        let name = state.rooms.get(idx).map(|r| r.name.as_str()).unwrap_or("?");
        let confirm_msg = if lang == Lang::English {
            format!("Delete \"{name}\"?  Enter/Y Confirm  other key Cancel")
        } else {
            format!("「{name}」 삭제?  Enter/Y 확인  다른 키 취소")
        };
        frame.render_widget(
            Paragraph::new(Span::styled(confirm_msg, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
            hint_rect,
        );
    } else {
        frame.render_widget(
            Paragraph::new(t(lang, Key::HintMoveJoinDeleteBack)),
            hint_rect,
        );
    }
}

// ── 방 만들기 화면 ────────────────────────────────────────────────────────────

fn render_create_room(frame: &mut Frame, state: &CreateRoomState, lang: Lang) {
    let title = format!(" {} ", t(lang, Key::CreateRoom));
    let area = center_rect(46, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let name_lbl = t(lang, Key::RoomName);
    let lifetime_lbl = if lang == Lang::English { "Lifetime" } else { "수명" };
    frame.render_widget(
        Paragraph::new(format!("{name_lbl} : [{}]", state.name_input)),
        chunks[0],
    );

    let lifetime_str = match state.lifetime {
        crate::room::RoomLifetime::OneDay => {
            if lang == Lang::English { "1 day (default)" } else { "1일 (기본)" }
        }
        crate::room::RoomLifetime::ThreeDays => t(lang, Key::RoomLifetimeThreeDays),
        crate::room::RoomLifetime::SevenDays => t(lang, Key::RoomLifetimeSevenDays),
        crate::room::RoomLifetime::Unlimited => t(lang, Key::RoomLifetimeUnlimited),
    };
    frame.render_widget(
        Paragraph::new(format!("{lifetime_lbl} : [{:<12}▼]", lifetime_str)),
        chunks[1],
    );

    let hint_style = Style::default().fg(Color::DarkGray);
    let (fmt_hint, ex_hint) = if lang == Lang::English {
        ("※ Format: name.suffix", "※ Example: dev.team, project.work")
    } else {
        ("※ 형식 : name.suffix", "※ 예시 : dev.team, project.work")
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(fmt_hint, hint_style)])),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(ex_hint, hint_style)])),
        chunks[3],
    );
    frame.render_widget(Paragraph::new(t(lang, Key::HintEnterCreateEscCancel)), chunks[4]);

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[5]);
    }
}

// ── 초대 코드 입장 화면 ───────────────────────────────────────────────────────

fn render_invite_entry(frame: &mut Frame, state: &InviteEntryState, lang: Lang) {
    let title = format!(" {} ", t(lang, Key::JoinByInvite));
    let area = center_rect(46, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let enter_confirm = if lang == Lang::English { "Enter Confirm  Esc Cancel" } else { "Enter 확인  Esc 취소" };
    let _enter_connect = if lang == Lang::English { "Enter Connect  Esc Cancel" } else { "Enter 연결  Esc 취소" };
    let enter_retry = if lang == Lang::English { "Enter Retry  Esc Cancel" } else { "Enter 다시 시도  Esc 취소" };
    let code_lbl = t(lang, Key::InviteCode);
    let url_lbl = if lang == Lang::English { "Room URL" } else { "방 URL" };
    let waiting_lbl = if lang == Lang::English { "Waiting for approval... (remaining: {}s)" } else { "승인 대기 중... (남은 시간 :{}s)" };
    let fail_lbl = if lang == Lang::English { "! Connection failed ({})" } else { "! 연결 실패 ({})" };

    match &state.step {
        InviteStep::UrlInput => {
            frame.render_widget(
                Paragraph::new(format!("{url_lbl} : [{}]", state.url_input)),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            let hint = if lang == Lang::English {
                "e.g. 192.168.1.1:40000  (empty = skip)"
            } else {
                "예: 192.168.1.1:40000  (비우면 건너뜀)"
            };
            frame.render_widget(
                Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray))),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new(enter_confirm),
                Rect::new(inner.x, inner.y + 3, inner.width, 1),
            );
        }
        InviteStep::RoomSelect => {
            let items: Vec<ListItem> = state.room_candidates.iter().enumerate().map(|(i, (id, ident))| {
                let marker = if i == state.room_cursor { "▶" } else { " " };
                let id_hex: String = id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                ListItem::new(format!("{marker} [{id_hex}]  #{ident}"))
            }).collect();
            frame.render_widget(List::new(items), inner);
        }
        InviteStep::CodeInput => {
            frame.render_widget(
                Paragraph::new(format!("{url_lbl} : {}", state.url_input)),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new(format!("{code_lbl} : [{}]", state.code_input)),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
            );
            let esc_back = if lang == Lang::English { "Enter Connect  Esc Back" } else { "Enter 연결  Esc 뒤로" };
            frame.render_widget(
                Paragraph::new(esc_back),
                Rect::new(inner.x, inner.y + 4, inner.width, 1),
            );
        }
        InviteStep::Waiting => {
            let secs = state.ttl_remaining_ms / 1000;
            let msg = waiting_lbl.replace("{}", &secs.to_string()).replace("{}s", &format!("{secs}s"));
            frame.render_widget(
                Paragraph::new(msg),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
        }
        InviteStep::Failed(reason) => {
            let msg = fail_lbl.replace("{}", reason);
            frame.render_widget(
                Paragraph::new(Span::styled(msg, Style::default().fg(Color::Red))),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new(enter_retry),
                Rect::new(inner.x, inner.y + 3, inner.width, 1),
            );
        }
    }
}

// ── 친구 목록 화면 ────────────────────────────────────────────────────────────

fn render_friend_list(frame: &mut Frame, state: &FriendListState, lang: Lang) {
    let title = format!(" {} ", t(lang, Key::FriendList));
    let area = frame.area();
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    let items: Vec<ListItem> = state.friends.iter().enumerate().map(|(i, f)| {
        let marker = if i == state.cursor { "▶" } else { " " };
        ListItem::new(format!("{marker} {:<30} {}", f.display_name, f.connected_date))
    }).collect();

    let list_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    frame.render_widget(List::new(items), list_rect);

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(Paragraph::new(t(lang, Key::HintMoveDeleteBack)), hint_rect);
}

// ── 설정 화면 ─────────────────────────────────────────────────────────────────

fn render_settings(frame: &mut Frame, state: &SettingsState, lang: Lang) {
    let area = frame.area();
    let title = match state.category {
        SettingsCategory::Select => format!(" {} ", t(lang, Key::Settings)),
        SettingsCategory::Profile => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatProfile)),
        SettingsCategory::Network => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatNetwork)),
        SettingsCategory::Chat => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatChat)),
        SettingsCategory::File => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatFile)),
        SettingsCategory::RoomManage => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatRoomManage)),
        SettingsCategory::FriendManage => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatFriendManage)),
        SettingsCategory::Language => format!(" {} > {} ", t(lang, Key::Settings), t(lang, Key::CatLanguage)),
    };
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    let list_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);

    match state.category {
        SettingsCategory::Select => {
            let categories = [
                t(lang, Key::CatProfile),
                t(lang, Key::CatNetwork),
                t(lang, Key::CatChat),
                t(lang, Key::CatFile),
                t(lang, Key::CatRoomManage),
                t(lang, Key::CatFriendManage),
                t(lang, Key::CatLanguage),
            ];
            let items: Vec<ListItem> = categories.iter().enumerate().map(|(i, cat)| {
                let marker = if i == state.cursor { "▶" } else { " " };
                ListItem::new(format!("{marker} {cat}"))
            }).collect();
            frame.render_widget(List::new(items), list_rect);
            frame.render_widget(Paragraph::new(t(lang, Key::HintMoveSelectBack)), hint_rect);
        }
        SettingsCategory::Profile => {
            let pw_hint = match state.pw_change_step {
                1 => {
                    if lang == Lang::English { "(enter current password)".to_string() }
                    else { "(현재 비밀번호 입력)".to_string() }
                }
                2 => {
                    if lang == Lang::English { "(enter new password)".to_string() }
                    else { "(새 비밀번호 입력)".to_string() }
                }
                _ => String::new(),
            };
            let read_only_note = t(lang, Key::ReadOnly);
            let fields: Vec<(&str, String, bool)> = vec![
                ("ID", state.config.user_id.clone(), true),
                (t(lang, Key::Nickname), state.config.nickname.clone(), false),
                (t(lang, Key::PasswordChange), pw_hint, false),
            ];
            let items: Vec<ListItem> = fields.iter().enumerate().map(|(i, (label, val, read_only))| {
                let marker = if i == state.cursor { "▶" } else { " " };
                let editing_str = if *read_only {
                    format!(" {val}  {read_only_note}")
                } else if state.editing && i == state.cursor {
                    if i == 2 {
                        format!("[{}█]", "*".repeat(state.edit_input.chars().count()))
                    } else {
                        format!("[{}█]", state.edit_input)
                    }
                } else if val.is_empty() {
                    "[...]".to_string()
                } else {
                    format!("[{}]", val)
                };
                ListItem::new(format!("{marker} {label:<20} {editing_str}"))
            }).collect();
            frame.render_widget(List::new(items), list_rect);
            if state.editing && state.pw_change_step > 0 {
                let step_hint = if state.pw_change_step == 1 {
                    t(lang, Key::HintCurrentPwEnterNext)
                } else {
                    t(lang, Key::HintNewPwEnterChange)
                };
                frame.render_widget(Paragraph::new(step_hint), hint_rect);
            } else if state.editing {
                frame.render_widget(Paragraph::new(t(lang, Key::HintEditConfirmCancel)), hint_rect);
            } else {
                frame.render_widget(Paragraph::new(t(lang, Key::HintMoveEditBack)), hint_rect);
            }
        }
        SettingsCategory::Network => {
            let toggle_hint = if lang == Lang::English { " (Space/Enter toggle)" } else { " (Space/Enter 토글)" };
            let fields: Vec<(String, &str)> = vec![
                (format!("{}{}", state.config.network_mode, toggle_hint), t(lang, Key::NetworkMode)),
                (state.config.port.clone(), t(lang, Key::Port)),
                (state.config.max_connections.clone(), t(lang, Key::MaxConnections)),
            ];
            let items: Vec<ListItem> = fields.iter().enumerate().map(|(i, (val, label))| {
                let marker = if i == state.cursor { "▶" } else { " " };
                let val_str = if state.editing && i == state.cursor {
                    format!("[{}█]", state.edit_input)
                } else {
                    format!("[{}]", val)
                };
                ListItem::new(format!("{marker} {label:<20} {val_str}"))
            }).collect();
            frame.render_widget(List::new(items), list_rect);
            if state.editing {
                frame.render_widget(Paragraph::new(t(lang, Key::HintEditConfirmCancel)), hint_rect);
            } else {
                frame.render_widget(Paragraph::new(t(lang, Key::HintMoveToggleBack)), hint_rect);
            }
        }
        SettingsCategory::Chat => {
            let val = if state.editing && state.cursor == 0 {
                format!("[{}█]", state.edit_input)
            } else {
                format!("[{}]", state.config.log_path)
            };
            let items = vec![
                ListItem::new(format!("▶ {:<20} {val}", t(lang, Key::LogPath))),
            ];
            frame.render_widget(List::new(items), list_rect);
            if state.editing {
                frame.render_widget(Paragraph::new(t(lang, Key::HintEditConfirmCancel)), hint_rect);
            } else {
                frame.render_widget(Paragraph::new(t(lang, Key::HintMoveEditBack)), hint_rect);
            }
        }
        SettingsCategory::File => {
            let speed_hint = |kbps: &str| -> String {
                if kbps == "0" || kbps.is_empty() { t(lang, Key::SpeedUnlimited).to_string() }
                else { format!("{kbps} {}", t(lang, Key::SpeedUnit)) }
            };
            let fields: Vec<(&str, String)> = vec![
                (t(lang, Key::DownloadPath), state.config.download_path.clone()),
                (t(lang, Key::MaxConcurrentDownloads), state.config.max_concurrent_dl.clone()),
                (t(lang, Key::UploadSpeedLimit), speed_hint(&state.config.max_upload_kbps)),
                (t(lang, Key::DownloadSpeedLimit), speed_hint(&state.config.max_download_kbps)),
            ];
            let unlimited_hint = if lang == Lang::English { " (0=Unlimited, KB/s)" } else { " (0=무제한, KB/s)" };
            let items: Vec<ListItem> = fields.iter().enumerate().map(|(i, (label, val))| {
                let marker = if i == state.cursor { "▶" } else { " " };
                let val_str = if state.editing && i == state.cursor {
                    let suffix = if i >= 2 { unlimited_hint } else { "" };
                    format!("[{}█]{}", state.edit_input, suffix)
                } else {
                    format!("[{}]", val)
                };
                ListItem::new(format!("{marker} {label:<24} {val_str}"))
            }).collect();
            frame.render_widget(List::new(items), list_rect);
            if state.editing {
                frame.render_widget(Paragraph::new(t(lang, Key::HintEditConfirmCancel)), hint_rect);
            } else {
                frame.render_widget(Paragraph::new(t(lang, Key::HintMoveEditBack)), hint_rect);
            }
        }
        SettingsCategory::RoomManage => {
            let (line1, line2) = if lang == Lang::English {
                (
                    "  Manage rooms from the Room List screen (Main Menu [2]).",
                    "  Expired rooms are auto-cleaned when entering the room list.",
                )
            } else {
                (
                    "  방 목록 화면(메인 메뉴 [2])에서 방 입장·삭제를 수행하세요.",
                    "  만료된 방은 방 목록 진입 시 자동으로 정리됩니다.",
                )
            };
            let items = vec![
                ListItem::new(line1),
                ListItem::new(line2),
            ];
            frame.render_widget(List::new(items), list_rect);
            frame.render_widget(Paragraph::new(t(lang, Key::HintEscBack)), hint_rect);
        }
        SettingsCategory::FriendManage => {
            let line = if lang == Lang::English {
                "  Manage friends from the Friends screen (Main Menu [4])."
            } else {
                "  친구 관리는 메인 메뉴 [4] 친구 목록에서 수행하세요."
            };
            let items = vec![ListItem::new(line)];
            frame.render_widget(List::new(items), list_rect);
            frame.render_widget(Paragraph::new(t(lang, Key::HintEscBack)), hint_rect);
        }
        SettingsCategory::Language => {
            let val = state.config.language.as_str();
            let val_str = if state.editing {
                format!("[{}█]", state.edit_input)
            } else {
                let toggle = if lang == Lang::English { "(Enter toggle)" } else { "(Enter 토글)" };
                format!("[{}] {toggle}", val)
            };
            let items = vec![
                ListItem::new(format!("▶ {:<20} {val_str}", t(lang, Key::Language))),
            ];
            frame.render_widget(List::new(items), list_rect);
            frame.render_widget(Paragraph::new(t(lang, Key::HintToggleBack)), hint_rect);
        }
    }

    if let Some(err) = &state.error {
        let err_rect = Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(format!("! {err}"), Style::default().fg(Color::Red))),
            err_rect,
        );
    }
}

// ── 파일 선택 화면 ────────────────────────────────────────────────────────────

fn render_file_select(frame: &mut Frame, state: &FileSelectState, lang: Lang) {
    let title = if lang == Lang::English {
        format!(" Download — {}/ ", state.folder_name)
    } else {
        format!(" 다운로드 파일 선택 — {}/ ", state.folder_name)
    };
    let area = frame.area();
    let block = Block::default().borders(Borders::ALL).title(title.as_str());
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    let items: Vec<ListItem> = state.items.iter().enumerate().map(|(i, item)| {
        let marker = if i == state.cursor { "▶" } else { " " };
        let check = if item.selected { "✓" } else { " " };
        let indent = "  ".repeat(item.depth);
        let dir_mark = if item.is_dir { "/" } else { "" };
        let size_str = format_size(item.size);
        ListItem::new(format!("{marker} {indent}[{check}] {}{dir_mark:<24} {}", item.name, size_str))
    }).collect();

    let list_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
    frame.render_widget(List::new(items), list_rect);

    let sel_size = format_size(state.selected_size);
    let tot_size = format_size(state.total_size);
    let size_label = if lang == Lang::English {
        format!("Selected: {} / {}", sel_size, tot_size)
    } else {
        format!("선택 용량: {} / {}", sel_size, tot_size)
    };
    let info_rect = Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1);
    frame.render_widget(Paragraph::new(size_label), info_rect);

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(
        Paragraph::new(t(lang, Key::HintSpaceSelectAAllEnterStartEscCancel)),
        hint_rect,
    );
}

// ── 채팅/파일 화면 ────────────────────────────────────────────────────────────

fn render_chat(frame: &mut Frame, state: &ChatState, lang: Lang) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // 상태바
            Constraint::Length(3),  // 전송 요약
            Constraint::Min(0),     // 피드
            Constraint::Length(1),  // 입력창
        ])
        .split(area);

    // ── 상태바 ────────────────────────────────────────────────────────────────
    let status_text = if state.expired {
        let expired_lbl = t(lang, Key::Expired);
        format!(" room: {} │ {expired_lbl}", state.room_name)
    } else if state.peer_count == 0 {
        let sync_str = state.last_sync_ms.map(|ms| {
            let elapsed_days = (crate::room::store::RoomStore::now_ms() - ms) / (24 * 60 * 60 * 1000);
            if lang == Lang::English {
                format!("{}: {} {}", t(lang, Key::LastSyncLabel), elapsed_days, t(lang, Key::DaysAgo))
            } else {
                format!("{}: {}{}", t(lang, Key::LastSyncLabel), elapsed_days, t(lang, Key::DaysAgo))
            }
        }).unwrap_or_else(|| t(lang, Key::NoSync).to_string());
        format!(" room: {} │ peers:0 │ {}", state.room_name, sync_str)
    } else {
        let up = format_bps(state.upload_bps);
        let down = format_bps(state.download_bps);
        let up_lbl = t(lang, Key::UploadLabel);
        let dn_lbl = t(lang, Key::DownloadLabel);
        format!(" room: {} │ peers:{} │ {up_lbl}:{up} {dn_lbl}:{down}", state.room_name, state.peer_count)
    };
    frame.render_widget(
        Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray)),
        chunks[0],
    );

    // ── 전송 요약 ─────────────────────────────────────────────────────────────
    let transfer_lines: Vec<Line> = if state.active_downloads.is_empty() {
        vec![Line::raw(""), Line::raw(""), Line::raw("")]
    } else {
        // 12-tui.md: 최대 3개 표시, 3개 초과 시 마지막 줄을 요약으로 대체
        let mut lines: Vec<Line> = state.active_downloads.iter().take(3).map(|dl| {
            let bps = format_bps(dl.bps);
            Line::raw(format!(" [↓] {:<20} {:>5.1}%  {}", dl.file_name, dl.pct, bps))
        }).collect();

        if state.active_downloads.len() > 3 {
            let extra = state.active_downloads.len() - 2;
            let more_lbl = if lang == Lang::English {
                format!(" +{extra} more in progress...  /downloads")
            } else {
                format!(" 외 {extra}개 진행 중...  /downloads")
            };
            lines.truncate(2);
            lines.push(Line::raw(more_lbl));
        } else {
            while lines.len() < 3 {
                lines.push(Line::raw(""));
            }
        }
        lines
    };
    frame.render_widget(Paragraph::new(transfer_lines), chunks[1]);

    // ── 피드 ─────────────────────────────────────────────────────────────────
    let feed_height = chunks[2].height as usize;
    let feed_len = state.feed.len();
    let scroll_offset = if feed_len > feed_height {
        let max_scroll = feed_len - feed_height;
        state.feed_scroll.min(max_scroll)
    } else {
        0
    };

    let visible_feed: Vec<Line> = state.feed.iter()
        .skip(scroll_offset)
        .take(feed_height)
        .map(|item| feed_item_to_line(item))
        .collect();

    frame.render_widget(
        Paragraph::new(visible_feed).wrap(Wrap { trim: false }),
        chunks[2],
    );

    // ── 입력창 ────────────────────────────────────────────────────────────────
    let input_style = if state.input_disabled {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let disabled_lbl = if lang == Lang::English { "(disabled)" } else { "(비활성화)" };
    let prompt = if state.input_disabled { disabled_lbl } else { &state.input };
    frame.render_widget(
        Paragraph::new(format!("> {}", prompt)).style(input_style),
        chunks[3],
    );

    // 초대 오버레이
    if state.show_invite_overlay && !state.pending_invites.is_empty() {
        render_invite_overlay(frame, &state.pending_invites, state.invite_cursor, lang);
    }
}

fn feed_item_to_line(item: &FeedItem) -> Line<'static> {
    let dt = chrono::DateTime::<chrono::Local>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(item.timestamp_ms)
    );
    let time_str = dt.format("%H:%M").to_string();

    match &item.content {
        FeedContent::Chat { peer_display, text } => {
            Line::from(vec![
                Span::styled(format!("{time_str} "), Style::default().fg(Color::DarkGray)),
                Span::styled(peer_display.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" : {text}")),
            ])
        }
        FeedContent::FileEvent(msg) => {
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Green)))
        }
        FeedContent::System(msg) => {
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Yellow)))
        }
        FeedContent::Invite(msg) => {
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Magenta)))
        }
        FeedContent::Command(msg) => {
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Gray)))
        }
    }
}

// ── 유틸 ────────────────────────────────────────────────────────────────────

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

fn format_bps(bps: u64) -> String {
    if bps >= 1024 * 1024 {
        format!("{:.1}MB/s", bps as f64 / (1024.0 * 1024.0))
    } else if bps >= 1024 {
        format!("{:.1}KB/s", bps as f64 / 1024.0)
    } else {
        format!("{bps}B/s")
    }
}
