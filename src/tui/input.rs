use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::channels::AppCommand;
use crate::room::RoomLifetime;

use super::screen::*;

// ── TUI 액션 ─────────────────────────────────────────────────────────────────

/// 키 입력 처리 결과.
pub enum TuiAction {
    /// 아무 동작 없음.
    None,
    /// AppCore로 전달할 커맨드 (선택적 화면 전환 포함).
    Command(AppCommand),
    /// 화면 전환만 (커맨드 없음).
    Goto(Screen),
    /// 커맨드 + 화면 전환.
    CommandAndGoto(AppCommand, Screen),
    /// 로그인 (main.rs에서 직접 처리).
    DoLogin { id: String, password: String },
    /// 계정 등록 (main.rs에서 직접 처리).
    DoRegister { id: String, nickname: String, password: String },
    /// 계정 삭제 (main.rs에서 직접 처리).
    DoDeleteAccount { id: String },
    /// 앱 종료.
    Quit,
}

// ── 메인 키 핸들러 ────────────────────────────────────────────────────────────

/// 현재 화면 상태에 따라 키 입력을 처리하고 TuiAction을 반환한다.
///
/// Screen 상태 변경이 필요할 때 TuiAction::Goto 또는 CommandAndGoto를 반환하며,
/// 호출자(main.rs)가 screen을 교체한다.
pub fn handle_key(screen: &mut Screen, key: KeyEvent) -> TuiAction {
    match screen {
        Screen::Login(s) => handle_login(s, key),
        Screen::Register(s) => handle_register(s, key),
        Screen::DeleteAccount(s) => handle_delete_account(s, key),
        Screen::MainMenu(s) => handle_main_menu(s, key),
        Screen::RoomList(s) => handle_room_list(s, key),
        Screen::CreateRoom(s) => handle_create_room(s, key),
        Screen::InviteEntry(s) => handle_invite_entry(s, key),
        Screen::FriendList(s) => handle_friend_list(s, key),
        Screen::Settings(s) => handle_settings(s, key),
        Screen::FileSelect(s) => handle_file_select(s, key),
        Screen::Chat(s) => handle_chat(s, key),
    }
}

// ── 공통 텍스트 입력 처리 ─────────────────────────────────────────────────────

fn push_char(buf: &mut String, c: char) {
    if buf.len() < 64 { buf.push(c); }
}

fn pop_char(buf: &mut String) {
    buf.pop();
}

// ── 로그인 화면 ───────────────────────────────────────────────────────────────

fn handle_login(s: &mut LoginState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Tab => {
            s.focused = match s.focused {
                LoginField::Id => LoginField::Pw,
                LoginField::Pw => LoginField::Id,
            };
        }
        KeyCode::Backspace => match s.focused {
            LoginField::Id => pop_char(&mut s.id_input),
            LoginField::Pw => pop_char(&mut s.pw_input),
        },
        KeyCode::Char(c) => {
            // 숫자 키로 메뉴 선택 (ID 입력창이 비어 있을 때)
            if s.id_input.is_empty() && s.pw_input.is_empty() {
                match c {
                    '2' => return TuiAction::Goto(Screen::Register(RegisterState::default())),
                    '3' => return TuiAction::Goto(Screen::DeleteAccount(DeleteAccountState::default())),
                    'q' | 'Q' => return TuiAction::Quit,
                    _ => {}
                }
            }
            match s.focused {
                LoginField::Id => push_char(&mut s.id_input, c),
                LoginField::Pw => push_char(&mut s.pw_input, c),
            }
        }
        KeyCode::Enter => {
            if !s.id_input.is_empty() && !s.pw_input.is_empty() {
                let id = s.id_input.clone();
                let password = s.pw_input.clone();
                s.pw_input.clear();
                return TuiAction::DoLogin { id, password };
            }
        }
        KeyCode::F(1) if s.id_input.is_empty() => {
            // F1: 로그인 모드 (기본, 현재 화면 유지)
        }
        KeyCode::F(2) if s.id_input.is_empty() => {
            return TuiAction::Goto(Screen::Register(RegisterState::default()));
        }
        _ => {}
    }
    TuiAction::None
}

// ── 계정 등록 화면 ────────────────────────────────────────────────────────────

fn handle_register(s: &mut RegisterState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Tab => {
            s.focused = match s.focused {
                RegisterField::Id => RegisterField::Nickname,
                RegisterField::Nickname => RegisterField::Pw,
                RegisterField::Pw => RegisterField::PwConfirm,
                RegisterField::PwConfirm => RegisterField::Id,
            };
        }
        KeyCode::Backspace => match s.focused {
            RegisterField::Id => pop_char(&mut s.id_input),
            RegisterField::Nickname => pop_char(&mut s.nickname_input),
            RegisterField::Pw => pop_char(&mut s.pw_input),
            RegisterField::PwConfirm => pop_char(&mut s.pw_confirm),
        },
        KeyCode::Char(c) => match s.focused {
            RegisterField::Id => push_char(&mut s.id_input, c),
            RegisterField::Nickname => push_char(&mut s.nickname_input, c),
            RegisterField::Pw => push_char(&mut s.pw_input, c),
            RegisterField::PwConfirm => push_char(&mut s.pw_confirm, c),
        },
        KeyCode::Enter => {
            if s.pw_input != s.pw_confirm {
                s.error = Some("비밀번호가 일치하지 않습니다.".into());
                return TuiAction::None;
            }
            if s.id_input.is_empty() || s.nickname_input.is_empty() || s.pw_input.is_empty() {
                s.error = Some("모든 항목을 입력하세요.".into());
                return TuiAction::None;
            }
            let id = s.id_input.clone();
            let nickname = s.nickname_input.clone();
            let password = s.pw_input.clone();
            return TuiAction::DoRegister { id, nickname, password };
        }
        KeyCode::Esc => {
            return TuiAction::Command(AppCommand::Shutdown); // Login 화면으로는 Screen 전환 필요
        }
        _ => {}
    }
    TuiAction::None
}

// ── 계정 삭제 화면 ────────────────────────────────────────────────────────────

fn handle_delete_account(s: &mut DeleteAccountState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if !s.id.is_empty() {
                return TuiAction::DoDeleteAccount { id: s.id.clone() };
            }
        }
        KeyCode::Esc => {
            return TuiAction::Command(AppCommand::Shutdown); // Login으로 복귀는 Screen 전환 필요
        }
        _ => {}
    }
    TuiAction::None
}

// ── 메인 메뉴 ─────────────────────────────────────────────────────────────────

fn handle_main_menu(s: &mut MainMenuState, key: KeyEvent) -> TuiAction {
    // 초대 오버레이가 열려 있는 경우
    if s.show_invite_overlay && !s.pending_invites.is_empty() {
        match key.code {
            KeyCode::Up => {
                if s.invite_cursor > 0 { s.invite_cursor -= 1; }
            }
            KeyCode::Down => {
                if s.invite_cursor + 1 < s.pending_invites.len() {
                    s.invite_cursor += 1;
                }
            }
            KeyCode::Enter => {
                let number = s.pending_invites[s.invite_cursor].number;
                return TuiAction::Command(AppCommand::AcceptInvite { number: Some(number) });
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // 초대 거절 (현재 미구현)
            }
            KeyCode::Esc => {
                s.show_invite_overlay = false;
            }
            _ => {}
        }
        return TuiAction::None;
    }

    match key.code {
        KeyCode::Char('1') => {
            return TuiAction::Goto(Screen::CreateRoom(CreateRoomState::default()));
        }
        KeyCode::Char('2') => {
            return TuiAction::CommandAndGoto(
                AppCommand::ListRooms,
                Screen::RoomList(RoomListState::default()),
            );
        }
        KeyCode::Char('3') => {
            return TuiAction::Goto(Screen::InviteEntry(InviteEntryState::default()));
        }
        KeyCode::Char('4') => {
            return TuiAction::Goto(Screen::FriendList(FriendListState::default()));
        }
        KeyCode::Char('5') => {
            return TuiAction::Goto(Screen::Settings(SettingsState::default()));
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            return TuiAction::Command(AppCommand::Shutdown);
        }
        _ => {}
    }
    TuiAction::None
}

// ── 방 목록 화면 ─────────────────────────────────────────────────────────────

fn handle_room_list(s: &mut RoomListState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Up => {
            if s.cursor > 0 { s.cursor -= 1; }
        }
        KeyCode::Down => {
            if s.cursor + 1 < s.rooms.len() { s.cursor += 1; }
        }
        KeyCode::Enter => {
            if let Some(room) = s.rooms.get(s.cursor) {
                let room_id = room.room_id;
                let room_name = room.name.clone();
                return TuiAction::CommandAndGoto(
                    AppCommand::JoinRoom { room_id },
                    Screen::Chat(ChatState { room_id, room_name, ..Default::default() }),
                );
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            s.confirm_delete = Some(s.cursor);
        }
        KeyCode::Esc => {
            return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
        }
        _ => {}
    }
    TuiAction::None
}

// ── 방 만들기 화면 ────────────────────────────────────────────────────────────

fn handle_create_room(s: &mut CreateRoomState, key: KeyEvent) -> TuiAction {
    match s.focused {
        CreateRoomField::Name => match key.code {
            KeyCode::Tab => s.focused = CreateRoomField::Lifetime,
            KeyCode::Backspace => pop_char(&mut s.name_input),
            KeyCode::Char(c) => push_char(&mut s.name_input, c),
            KeyCode::Enter => {
                if s.name_input.is_empty() {
                    s.error = Some("방 이름을 입력하세요.".into());
                } else {
                    let name = s.name_input.clone();
                    let lifetime = s.lifetime;
                    return TuiAction::CommandAndGoto(
                        AppCommand::CreateRoom { name, lifetime },
                        Screen::MainMenu(MainMenuState::default()),
                    );
                }
            }
            KeyCode::Esc => {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
            _ => {}
        },
        CreateRoomField::Lifetime => match key.code {
            KeyCode::Tab => s.focused = CreateRoomField::Name,
            KeyCode::Up | KeyCode::Down => {
                s.lifetime = cycle_lifetime(s.lifetime, key.code == KeyCode::Down);
            }
            KeyCode::Enter => {
                if s.name_input.is_empty() {
                    s.error = Some("방 이름을 입력하세요.".into());
                    s.focused = CreateRoomField::Name;
                } else {
                    let name = s.name_input.clone();
                    let lifetime = s.lifetime;
                    return TuiAction::CommandAndGoto(
                        AppCommand::CreateRoom { name, lifetime },
                        Screen::MainMenu(MainMenuState::default()),
                    );
                }
            }
            KeyCode::Esc => {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
            _ => {}
        },
    }
    TuiAction::None
}

fn cycle_lifetime(current: RoomLifetime, forward: bool) -> RoomLifetime {
    let list = [
        RoomLifetime::OneDay,
        RoomLifetime::ThreeDays,
        RoomLifetime::SevenDays,
        RoomLifetime::Unlimited,
    ];
    let pos = list.iter().position(|l| *l == current).unwrap_or(0);
    if forward {
        list[(pos + 1) % list.len()]
    } else {
        list[(pos + list.len() - 1) % list.len()]
    }
}

// ── 초대 코드 입장 화면 ───────────────────────────────────────────────────────

fn handle_invite_entry(s: &mut InviteEntryState, key: KeyEvent) -> TuiAction {
    match &s.step {
        InviteStep::CodeInput => match key.code {
            KeyCode::Backspace => { pop_char(&mut s.code_input); }
            KeyCode::Char(c) => { push_char(&mut s.code_input, c); }
            KeyCode::Enter => {
                if !s.code_input.is_empty() {
                    let code = s.code_input.clone().to_uppercase();
                    s.step = InviteStep::Waiting;
                    return TuiAction::Command(AppCommand::EnterInviteCode { code });
                }
            }
            KeyCode::Esc => {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
            _ => {}
        },
        InviteStep::Waiting => {
            if key.code == KeyCode::Esc {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
        }
        InviteStep::Failed(_) => match key.code {
            KeyCode::Enter => {
                s.step = InviteStep::CodeInput;
                s.code_input.clear();
            }
            KeyCode::Esc => {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
            _ => {}
        },
        _ => {
            if key.code == KeyCode::Esc {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
        }
    }
    TuiAction::None
}

// ── 친구 목록 화면 ────────────────────────────────────────────────────────────

fn handle_friend_list(s: &mut FriendListState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Up => {
            if s.cursor > 0 { s.cursor -= 1; }
        }
        KeyCode::Down => {
            if s.cursor + 1 < s.friends.len() { s.cursor += 1; }
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if let Some(f) = s.friends.get(s.cursor) {
                let bytes = f.peer_id_bytes.clone();
                return TuiAction::Command(AppCommand::RemoveFriend { peer_id_bytes: bytes });
            }
        }
        KeyCode::Esc => {
            return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
        }
        _ => {}
    }
    TuiAction::None
}

// ── 설정 화면 ─────────────────────────────────────────────────────────────────

fn handle_settings(s: &mut SettingsState, key: KeyEvent) -> TuiAction {
    match s.category {
        SettingsCategory::Select => match key.code {
            KeyCode::Up => {
                if s.cursor > 0 { s.cursor -= 1; }
            }
            KeyCode::Down => {
                let max = 6; // 카테고리 수 - 1
                if s.cursor < max { s.cursor += 1; }
            }
            KeyCode::Enter => {
                s.category = index_to_category(s.cursor);
                s.cursor = 0;
            }
            KeyCode::Esc => {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
            _ => {}
        },
        _ => match key.code {
            KeyCode::Enter if s.editing => {
                // 편집 완료 → 커맨드 전송
                let action = apply_setting_edit(s);
                s.editing = false;
                s.edit_input.clear();
                return action;
            }
            KeyCode::Char(c) if s.editing => {
                push_char(&mut s.edit_input, c);
            }
            KeyCode::Backspace if s.editing => {
                pop_char(&mut s.edit_input);
            }
            KeyCode::Esc if s.editing => {
                s.editing = false;
                s.edit_input.clear();
            }
            KeyCode::Enter => {
                s.editing = true;
                s.edit_input.clear();
            }
            KeyCode::Esc => {
                s.category = SettingsCategory::Select;
                s.cursor = 0;
            }
            _ => {}
        },
    }
    TuiAction::None
}

fn index_to_category(idx: usize) -> SettingsCategory {
    match idx {
        0 => SettingsCategory::Profile,
        1 => SettingsCategory::Network,
        2 => SettingsCategory::Chat,
        3 => SettingsCategory::File,
        4 => SettingsCategory::RoomManage,
        5 => SettingsCategory::FriendManage,
        _ => SettingsCategory::Language,
    }
}

fn apply_setting_edit(s: &SettingsState) -> TuiAction {
    match s.category {
        SettingsCategory::Profile => {
            TuiAction::Command(AppCommand::ChangeNickname { new_nickname: s.edit_input.clone() })
        }
        _ => TuiAction::None,
    }
}

// ── 파일 선택 화면 ────────────────────────────────────────────────────────────

fn handle_file_select(s: &mut FileSelectState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Up => {
            if s.cursor > 0 { s.cursor -= 1; }
        }
        KeyCode::Down => {
            if s.cursor + 1 < s.items.len() { s.cursor += 1; }
        }
        KeyCode::Char(' ') => {
            // 선택 토글
            if let Some(item) = s.items.get_mut(s.cursor) {
                item.selected = !item.selected;
                if item.selected {
                    s.selected_size += item.size;
                } else {
                    s.selected_size = s.selected_size.saturating_sub(item.size);
                }
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            // 전체 선택/해제
            let all_selected = s.items.iter().all(|i| i.selected);
            let new_val = !all_selected;
            s.selected_size = 0;
            for item in &mut s.items {
                item.selected = new_val;
                if new_val { s.selected_size += item.size; }
            }
        }
        KeyCode::Enter => {
            // 선택된 파일들 다운로드 시작
            let selected: Vec<_> = s.items.iter().filter(|i| i.selected && !i.is_dir).collect();
            if !selected.is_empty() {
                // 첫 번째 선택 파일만 예시 — 실제로는 여러 파일 StartDownload 필요
                if let Some(first) = selected.first() {
                    let cmd = AppCommand::StartDownload {
                        file_hash: first.file_hash,
                        file_name: first.name.clone(),
                        chunk_count: (first.size / 262144) as u32 + 1,
                    };
                    return TuiAction::CommandAndGoto(cmd, Screen::MainMenu(MainMenuState::default()));
                }
            }
        }
        KeyCode::Esc => {
            // 채팅방 화면으로 복귀 (room_id/name은 이전 상태 복원 불가로 기본값 사용)
            return TuiAction::Goto(Screen::Chat(ChatState::default()));
        }
        _ => {}
    }
    TuiAction::None
}

// ── 채팅/파일 화면 ────────────────────────────────────────────────────────────

fn handle_chat(s: &mut ChatState, key: KeyEvent) -> TuiAction {
    // Ctrl+C → 방 나가기
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return TuiAction::CommandAndGoto(
            AppCommand::LeaveRoom,
            Screen::MainMenu(MainMenuState::default()),
        );
    }

    if s.input_disabled {
        // 만료 시 /quit만 허용
        if key.code == KeyCode::Enter && s.input.trim() == "/quit" {
            s.input.clear();
            return TuiAction::CommandAndGoto(
                AppCommand::LeaveRoom,
                Screen::MainMenu(MainMenuState::default()),
            );
        }
        match key.code {
            KeyCode::Backspace => pop_char(&mut s.input),
            KeyCode::Char(c) => push_char(&mut s.input, c),
            _ => {}
        }
        return TuiAction::None;
    }

    match key.code {
        KeyCode::Enter => {
            let text = s.input.trim().to_string();
            s.input.clear();
            if text.is_empty() { return TuiAction::None; }

            // 커맨드 파싱
            return parse_chat_command(text, s);
        }
        KeyCode::Backspace => pop_char(&mut s.input),
        KeyCode::Char(c) => push_char(&mut s.input, c),
        KeyCode::PageUp | KeyCode::Up => {
            if s.feed_scroll > 0 { s.feed_scroll -= 1; }
        }
        KeyCode::PageDown | KeyCode::Down => {
            s.feed_scroll += 1;
        }
        _ => {}
    }
    TuiAction::None
}

fn parse_chat_command(text: String, s: &mut ChatState) -> TuiAction {
    use crate::chat::command::{parse, Command};

    match parse(&text) {
        Ok(Command::Quit) => {
            return TuiAction::CommandAndGoto(
                AppCommand::LeaveRoom,
                Screen::MainMenu(MainMenuState::default()),
            );
        }
        Ok(Command::Invite) => {
            return TuiAction::Command(AppCommand::GenerateInviteCode);
        }
        Ok(Command::Share { path }) => {
            return TuiAction::Command(AppCommand::ShareFile { path });
        }
        Ok(Command::Download { target, .. }) => {
            // 파일 이름으로 StartDownload (file_hash와 chunk_count는 FileAnnounce에서 가져와야 함)
            let msg = format!("다운로드 요청: {} (파일 목록에서 선택하세요)", target);
            use crate::room::RoomStore;
            s.feed.push(FeedItem {
                timestamp_ms: RoomStore::now_ms(),
                content: FeedContent::Command(msg),
            });
        }
        Ok(Command::Pause { number }) => {
            return TuiAction::Command(AppCommand::PauseDownload { number });
        }
        Ok(Command::Resume { number }) => {
            return TuiAction::Command(AppCommand::ResumeDownload { number });
        }
        Ok(Command::Cancel { number }) => {
            return TuiAction::Command(AppCommand::CancelDownload { number });
        }
        Ok(Command::Top { number }) => {
            return TuiAction::Command(AppCommand::MoveDownloadTop { number });
        }
        Ok(Command::Up { number }) => {
            return TuiAction::Command(AppCommand::MoveDownloadUp { number });
        }
        Ok(Command::Down { number }) => {
            return TuiAction::Command(AppCommand::MoveDownloadDown { number });
        }
        Ok(Command::SeedPause { number }) => {
            return TuiAction::Command(AppCommand::SeedPause { number });
        }
        Ok(Command::SeedResume { number }) => {
            return TuiAction::Command(AppCommand::SeedResume { number });
        }
        Ok(Command::Remove { number }) => {
            return TuiAction::Command(AppCommand::RemoveSeed { number, delete_file: false });
        }
        Ok(Command::RemoveAll { number }) => {
            return TuiAction::Command(AppCommand::RemoveSeed { number, delete_file: true });
        }
        Ok(Command::Peers) => {
            return TuiAction::Command(AppCommand::ListPeers);
        }
        Ok(Command::Nick { nickname }) => {
            return TuiAction::Command(AppCommand::ChangeNickname { new_nickname: nickname });
        }
        Ok(Command::Accept { number }) => {
            return TuiAction::Command(AppCommand::AcceptInvite { number });
        }
        Ok(Command::Message { text }) => {
            return TuiAction::Command(AppCommand::SendMessage { text });
        }
        Ok(_) => {} // Help, List, Downloads, Seed 등
        Err(e) => {
            // 파싱 오류를 피드에 표시
            use crate::room::RoomStore;
            s.feed.push(FeedItem {
                timestamp_ms: RoomStore::now_ms(),
                content: FeedContent::System(format!("! {e}")),
            });
        }
    }
    TuiAction::None
}
