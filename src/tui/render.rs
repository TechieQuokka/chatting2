use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};

use super::screen::*;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── 라우터 ───────────────────────────────────────────────────────────────────

pub fn render(frame: &mut Frame, screen: &Screen) {
    match screen {
        Screen::Login(s) => render_login(frame, s),
        Screen::Register(s) => render_register(frame, s),
        Screen::DeleteAccount(s) => render_delete_account(frame, s),
        Screen::MainMenu(s) => render_main_menu(frame, s),
        Screen::RoomList(s) => render_room_list(frame, s),
        Screen::CreateRoom(s) => render_create_room(frame, s),
        Screen::InviteEntry(s) => render_invite_entry(frame, s),
        Screen::FriendList(s) => render_friend_list(frame, s),
        Screen::Settings(s) => render_settings(frame, s),
        Screen::FileSelect(s) => render_file_select(frame, s),
        Screen::Chat(s) => render_chat(frame, s),
    }
}

// ── 공통 헬퍼 ────────────────────────────────────────────────────────────────

fn center_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn header_line(title: &str) -> Paragraph<'_> {
    Paragraph::new(Line::from(vec![
        Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  v{APP_VERSION}")),
    ]))
}

fn error_line(err: &str) -> Paragraph<'_> {
    Paragraph::new(Span::styled(
        format!("! {err}"),
        Style::default().fg(Color::Red),
    ))
}

// ── 로그인 화면 ───────────────────────────────────────────────────────────────

fn render_login(frame: &mut Frame, state: &LoginState) {
    let area = center_rect(42, 14, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" chatting2 ");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 버전
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // ID
            Constraint::Length(1), // PW
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // [1] 로그인
            Constraint::Length(1), // [2] 계정 등록
            Constraint::Length(1), // [3] 계정 삭제
            Constraint::Length(1), // [Q] 종료
            Constraint::Length(1), // 빈 줄
            Constraint::Length(1), // 오류 메시지
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("P2P Chat & File Transfer  v{APP_VERSION}")),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(format!("ID : [{}]", state.id_input)),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(format!("PW : [{}]", "*".repeat(state.pw_input.len()))),
        chunks[3],
    );

    let menu = ["[1] 로그인", "[2] 계정 등록", "[3] 계정 삭제", "[Q] 종료"];
    for (i, item) in menu.iter().enumerate() {
        frame.render_widget(Paragraph::new(*item), chunks[5 + i]);
    }

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[10]);
    }
}

// ── 계정 등록 화면 ────────────────────────────────────────────────────────────

fn render_register(frame: &mut Frame, state: &RegisterState) {
    let area = center_rect(42, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" 계정 등록 ");
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

    frame.render_widget(Paragraph::new(format!("ID : [{}]", state.id_input)), chunks[0]);
    frame.render_widget(
        Paragraph::new(format!("PW : [{}]", "*".repeat(state.pw_input.len()))),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(format!("PW 확인 : [{}]", "*".repeat(state.pw_confirm.len()))),
        chunks[2],
    );
    frame.render_widget(Paragraph::new("Enter 등록  Esc 취소"), chunks[3]);

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[5]);
    }
}

// ── 계정 삭제 화면 ────────────────────────────────────────────────────────────

fn render_delete_account(frame: &mut Frame, state: &DeleteAccountState) {
    let area = center_rect(42, 10, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" 계정 삭제 ");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("! {} 계정을 삭제할까요?", state.id)),
        chunks[0],
    );
    frame.render_widget(Paragraph::new("이 작업은 되돌릴 수 없습니다."), chunks[1]);
    frame.render_widget(Paragraph::new("(다운로드 파일은 유지됩니다)"), chunks[2]);
    frame.render_widget(Paragraph::new("y 삭제  Esc 취소"), chunks[3]);

    if let Some(err) = &state.error {
        let err_rect = Rect::new(inner.x, inner.y + inner.height.saturating_sub(1), inner.width, 1);
        frame.render_widget(error_line(err), err_rect);
    }
}

// ── 메인 메뉴 ─────────────────────────────────────────────────────────────────

fn render_main_menu(frame: &mut Frame, state: &MainMenuState) {
    let area = center_rect(42, 14, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" chatting2 ");
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

    frame.render_widget(
        Paragraph::new(format!("안녕하세요, {}님", state.nickname)),
        chunks[0],
    );

    let items = [
        "[1] 방 만들기",
        "[2] 방 목록",
        "[3] 초대 코드로 입장",
        "[4] 친구 목록",
        "[5] 설정",
        "[Q] 종료",
    ];
    for (i, item) in items.iter().enumerate() {
        frame.render_widget(Paragraph::new(*item), chunks[2 + i]);
    }

    // 초대 알림 오버레이
    if state.show_invite_overlay && !state.pending_invites.is_empty() {
        render_invite_overlay(frame, &state.pending_invites, state.invite_cursor);
    }
}

fn render_invite_overlay(frame: &mut Frame, invites: &[PendingInviteInfo], cursor: usize) {
    let area = center_rect(44, 12, frame.area());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" 초대 알림 ({건}) ", 건 = invites.len()));
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
        Paragraph::new("↑↓ 이동  Enter 수락  D 거절  Esc 나중에"),
        hint_rect,
    );
}

// ── 방 목록 화면 ─────────────────────────────────────────────────────────────

fn render_room_list(frame: &mut Frame, state: &RoomListState) {
    let area = frame.area();
    let block = Block::default().borders(Borders::ALL).title(" 방 목록 ");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    if state.expired_cleaned > 0 {
        let notice = Paragraph::new(
            Span::styled(
                format!("[알림] 만료된 방 {}개가 정리됐습니다.", state.expired_cleaned),
                Style::default().fg(Color::Yellow),
            )
        );
        frame.render_widget(notice, Rect::new(inner.x, inner.y, inner.width, 1));
    }

    let list_y = inner.y + if state.expired_cleaned > 0 { 1 } else { 0 };
    let list_h = inner.height.saturating_sub(2);

    let items: Vec<ListItem> = state.rooms.iter().enumerate().map(|(i, room)| {
        let marker = if i == state.cursor { "▶" } else { " " };
        let peer_str = match &room.peer_status {
            PeerStatus::Checking => "확인 중...".to_string(),
            PeerStatus::Online(n) => format!("peers: {n}"),
            PeerStatus::Offline => "오프라인".to_string(),
            PeerStatus::Expired => "만료됨".to_string(),
        };
        ListItem::new(format!("{marker} {:<22} {:<12} {}", room.name, peer_str, room.lifetime_display))
    }).collect();

    let list = List::new(items);
    frame.render_widget(list, Rect::new(inner.x, list_y, inner.width, list_h));

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(
        Paragraph::new("↑↓ 이동  Enter 입장  D 삭제  Esc 뒤로"),
        hint_rect,
    );
}

// ── 방 만들기 화면 ────────────────────────────────────────────────────────────

fn render_create_room(frame: &mut Frame, state: &CreateRoomState) {
    let area = center_rect(42, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" 방 만들기 ");
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

    frame.render_widget(
        Paragraph::new(format!("이름 : [{}]", state.name_input)),
        chunks[0],
    );

    let lifetime_str = match state.lifetime {
        crate::room::RoomLifetime::OneDay => "1일 (기본)",
        crate::room::RoomLifetime::ThreeDays => "3일",
        crate::room::RoomLifetime::SevenDays => "7일",
        crate::room::RoomLifetime::Unlimited => "무제한",
    };
    frame.render_widget(
        Paragraph::new(format!("수명 : [{:<12}▼]", lifetime_str)),
        chunks[1],
    );

    frame.render_widget(Paragraph::new("형식 : name.suffix"), chunks[2]);
    frame.render_widget(Paragraph::new("예시 : dev.team, project.work"), chunks[3]);
    frame.render_widget(Paragraph::new("Enter 생성  Esc 취소"), chunks[4]);

    if let Some(err) = &state.error {
        frame.render_widget(error_line(err), chunks[5]);
    }
}

// ── 초대 코드 입장 화면 ───────────────────────────────────────────────────────

fn render_invite_entry(frame: &mut Frame, state: &InviteEntryState) {
    let area = center_rect(42, 12, frame.area());
    let block = Block::default().borders(Borders::ALL).title(" 초대 코드로 입장 ");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    match &state.step {
        InviteStep::UrlInput => {
            frame.render_widget(
                Paragraph::new(format!("방 URL : [{}]", state.url_input)),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new("Enter 확인  Esc 취소"),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
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
                Paragraph::new(format!("코드 : [{}]", state.code_input)),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new("Enter 연결  Esc 취소"),
                Rect::new(inner.x, inner.y + 3, inner.width, 1),
            );
        }
        InviteStep::Waiting => {
            let secs = state.ttl_remaining_ms / 1000;
            frame.render_widget(
                Paragraph::new(format!("승인 대기 중... (남은 시간 :{}s)", secs)),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
        }
        InviteStep::Failed(reason) => {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!("! 연결 실패 ({})", reason),
                    Style::default().fg(Color::Red),
                )),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new("Enter 다시 시도  Esc 취소"),
                Rect::new(inner.x, inner.y + 3, inner.width, 1),
            );
        }
    }
}

// ── 친구 목록 화면 ────────────────────────────────────────────────────────────

fn render_friend_list(frame: &mut Frame, state: &FriendListState) {
    let area = frame.area();
    let block = Block::default().borders(Borders::ALL).title(" 친구 목록 ");
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    let items: Vec<ListItem> = state.friends.iter().enumerate().map(|(i, f)| {
        let marker = if i == state.cursor { "▶" } else { " " };
        ListItem::new(format!("{marker} {:<30} {}", f.display_name, f.connected_date))
    }).collect();

    let list_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    frame.render_widget(List::new(items), list_rect);

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(Paragraph::new("↑↓ 이동  D 삭제  Esc 뒤로"), hint_rect);
}

// ── 설정 화면 ─────────────────────────────────────────────────────────────────

fn render_settings(frame: &mut Frame, state: &SettingsState) {
    let area = frame.area();
    let title = match state.category {
        SettingsCategory::Select => " 설정 ",
        SettingsCategory::Profile => " 설정 > 프로필 ",
        SettingsCategory::Network => " 설정 > 네트워크 ",
        SettingsCategory::Chat => " 설정 > 채팅 ",
        SettingsCategory::File => " 설정 > 파일 ",
        SettingsCategory::RoomManage => " 설정 > 방 관리 ",
        SettingsCategory::FriendManage => " 설정 > 친구 관리 ",
        SettingsCategory::Language => " 설정 > 언어 ",
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block, area);

    let inner = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

    if state.category == SettingsCategory::Select {
        let categories = [
            "프로필", "네트워크", "채팅", "파일", "방 관리", "친구 관리", "언어",
        ];
        let items: Vec<ListItem> = categories.iter().enumerate().map(|(i, cat)| {
            let marker = if i == state.cursor { "▶" } else { " " };
            ListItem::new(format!("{marker} {cat}"))
        }).collect();

        let list_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
        frame.render_widget(List::new(items), list_rect);
    }

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(Paragraph::new("↑↓ 이동  Enter 선택  Esc 뒤로"), hint_rect);
}

// ── 파일 선택 화면 ────────────────────────────────────────────────────────────

fn render_file_select(frame: &mut Frame, state: &FileSelectState) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" 다운로드 파일 선택 — {}/ ", state.folder_name));
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
    let info_rect = Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1);
    frame.render_widget(
        Paragraph::new(format!("선택 용량: {} / {}", sel_size, tot_size)),
        info_rect,
    );

    let hint_rect = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    frame.render_widget(
        Paragraph::new("↑↓ 이동  Space 선택  A 전체  Enter 시작  Esc 취소"),
        hint_rect,
    );
}

// ── 채팅/파일 화면 ────────────────────────────────────────────────────────────

fn render_chat(frame: &mut Frame, state: &ChatState) {
    let area = frame.area();

    // 레이아웃: 상태바(1줄) + 전송요약(3줄) + 피드(나머지) + 입력창(1줄)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // 상태바
            Constraint::Length(3),  // 전송 요약 (최대 3줄)
            Constraint::Min(0),     // 피드
            Constraint::Length(1),  // 입력창
        ])
        .split(area);

    // ── 상태바 ────────────────────────────────────────────────────────────────
    let status_text = if state.expired {
        format!(" room: {} │ 만료됨", state.room_name)
    } else if state.peer_count == 0 {
        let sync_str = state.last_sync_ms.map(|ms| {
            let elapsed_days = (crate::room::store::RoomStore::now_ms() - ms) / (24 * 60 * 60 * 1000);
            format!("마지막 동기화: {elapsed_days}일 전")
        }).unwrap_or_else(|| "동기화 없음".to_string());
        format!(" room: {} │ peers:0 │ {}", state.room_name, sync_str)
    } else {
        let up = format_bps(state.upload_bps);
        let down = format_bps(state.download_bps);
        format!(" room: {} │ peers:{} │ ↑{} ↓{}", state.room_name, state.peer_count, up, down)
    };
    frame.render_widget(
        Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray)),
        chunks[0],
    );

    // ── 전송 요약 ─────────────────────────────────────────────────────────────
    let transfer_lines: Vec<Line> = if state.active_downloads.is_empty() {
        vec![Line::raw(""), Line::raw(""), Line::raw("")]
    } else {
        let mut lines: Vec<Line> = state.active_downloads.iter().take(2).map(|dl| {
            let bps = format_bps(dl.bps);
            Line::raw(format!(" [↓] {:<20} {:>5.1}%  {}", dl.file_name, dl.pct, bps))
        }).collect();

        if state.active_downloads.len() > 2 {
            lines.push(Line::raw(format!(
                " 외 {}개 진행 중...           /downloads",
                state.active_downloads.len() - 2
            )));
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
    let prompt = if state.input_disabled { "(비활성화)" } else { &state.input };
    frame.render_widget(
        Paragraph::new(format!("> {}", prompt)).style(input_style),
        chunks[3],
    );
}

fn feed_item_to_line(item: &FeedItem) -> Line<'static> {
    use chrono::prelude::*;
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
