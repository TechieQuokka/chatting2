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
    DoDeleteAccount { id: String, password: String },
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
        Screen::Welcome(s) => handle_welcome(s, key),
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

// ── 시작 화면 ────────────────────────────────────────────────────────────────

fn handle_welcome(_s: &mut WelcomeState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Char('1') => TuiAction::Goto(Screen::Login(LoginState::default())),
        KeyCode::Char('2') => TuiAction::Goto(Screen::Register(RegisterState::default())),
        KeyCode::Char('3') => TuiAction::Goto(Screen::DeleteAccount(DeleteAccountState::default())),
        KeyCode::Char('q') | KeyCode::Char('Q') => TuiAction::Quit,
        _ => TuiAction::None,
    }
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
        KeyCode::Char(c) => match s.focused {
            LoginField::Id => push_char(&mut s.id_input, c),
            LoginField::Pw => push_char(&mut s.pw_input, c),
        },
        KeyCode::Enter => {
            if !s.id_input.is_empty() && !s.pw_input.is_empty() {
                let id = s.id_input.clone();
                let password = s.pw_input.clone();
                s.pw_input.clear();
                return TuiAction::DoLogin { id, password };
            }
        }
        KeyCode::Esc => {
            return TuiAction::Goto(Screen::Welcome(WelcomeState::default()));
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
            return TuiAction::Goto(Screen::Welcome(WelcomeState::default()));
        }
        _ => {}
    }
    TuiAction::None
}

// ── 계정 삭제 화면 ────────────────────────────────────────────────────────────

fn handle_delete_account(s: &mut DeleteAccountState, key: KeyEvent) -> TuiAction {
    use super::screen::DeleteField;

    match key.code {
        KeyCode::Tab => {
            s.focused = match s.focused {
                DeleteField::Id => DeleteField::Pw,
                DeleteField::Pw => DeleteField::Id,
            };
        }
        KeyCode::Backspace => match s.focused {
            DeleteField::Id => pop_char(&mut s.id_input),
            DeleteField::Pw => pop_char(&mut s.pw_input),
        },
        KeyCode::Char(c) => match s.focused {
            DeleteField::Id => push_char(&mut s.id_input, c),
            DeleteField::Pw => push_char(&mut s.pw_input, c),
        },
        // Enter로 최종 확인 — ID/PW가 모두 입력된 경우만 진행
        KeyCode::Enter => {
            if s.id_input.is_empty() {
                s.error = Some("삭제할 계정 ID를 입력하세요.".into());
                s.focused = DeleteField::Id;
            } else if s.pw_input.is_empty() {
                s.error = Some("비밀번호를 입력하세요.".into());
                s.focused = DeleteField::Pw;
            } else {
                let id = s.id_input.clone();
                let password = s.pw_input.clone();
                s.pw_input.clear(); // 메모리에서 즉시 제거
                return TuiAction::DoDeleteAccount { id, password };
            }
        }
        KeyCode::Esc => {
            return TuiAction::Goto(Screen::Login(LoginState::default()));
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
                let number = s.pending_invites[s.invite_cursor].number;
                return TuiAction::Command(AppCommand::DeclineInvite { number: Some(number) });
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
            return TuiAction::CommandAndGoto(
                AppCommand::EnterSettings,
                Screen::Settings(SettingsState::default()),
            );
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
    // 삭제 확인 모드
    if s.confirm_delete.is_some() {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(idx) = s.confirm_delete.take() {
                    if let Some(room) = s.rooms.get(idx) {
                        let room_id = room.room_id;
                        s.rooms.remove(idx);
                        if s.cursor >= s.rooms.len() && s.cursor > 0 {
                            s.cursor -= 1;
                        }
                        return TuiAction::Command(AppCommand::DeleteRoom { room_id });
                    }
                }
            }
            _ => {
                // 다른 키 → 취소
                s.confirm_delete = None;
            }
        }
        return TuiAction::None;
    }

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
            if !s.rooms.is_empty() {
                s.confirm_delete = Some(s.cursor);
            }
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
        InviteStep::UrlInput => match key.code {
            KeyCode::Backspace => { pop_char(&mut s.url_input); }
            KeyCode::Char(c) => { push_char(&mut s.url_input, c); }
            KeyCode::Enter => {
                if s.url_input.is_empty() {
                    // URL 없이 코드만 입력 (이미 연결된 피어 대상)
                    s.step = InviteStep::CodeInput;
                } else {
                    // URL DHT 조회 시작
                    let url = s.url_input.clone();
                    s.step = InviteStep::UrlLookingUp;
                    return TuiAction::Command(AppCommand::LookupRoomUrl { url });
                }
            }
            KeyCode::Esc => {
                return TuiAction::Goto(Screen::MainMenu(MainMenuState::default()));
            }
            _ => {}
        },
        InviteStep::UrlLookingUp => {
            // 조회 결과 대기 중 — Esc로만 취소
            if key.code == KeyCode::Esc {
                s.step = InviteStep::UrlInput;
            }
        }
        InviteStep::RoomSelect => match key.code {
            KeyCode::Up => {
                if s.room_cursor > 0 { s.room_cursor -= 1; }
            }
            KeyCode::Down => {
                if s.room_cursor + 1 < s.room_candidates.len() { s.room_cursor += 1; }
            }
            KeyCode::Enter => {
                if let Some((id, _)) = s.room_candidates.get(s.room_cursor) {
                    s.selected_room = Some(*id);
                    s.step = InviteStep::CodeInput;
                }
            }
            KeyCode::Esc => {
                s.step = InviteStep::UrlInput;
            }
            _ => {}
        },
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
                s.step = InviteStep::UrlInput;
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
                // 비밀번호 변경: 2단계 입력 처리
                if s.category == SettingsCategory::Profile && s.cursor == 2 {
                    match s.pw_change_step {
                        1 => {
                            // 1단계 완료: 현재 PW 저장 후 2단계로 이동
                            s.pw_current_temp = s.edit_input.clone();
                            s.edit_input.clear();
                            s.pw_change_step = 2;
                            // editing 유지
                        }
                        2 => {
                            // 2단계 완료: ChangePassword 전송
                            let action = TuiAction::Command(AppCommand::ChangePassword {
                                current: s.pw_current_temp.clone(),
                                new_pw: s.edit_input.clone(),
                            });
                            s.pw_current_temp.clear();
                            s.pw_change_step = 0;
                            s.editing = false;
                            s.edit_input.clear();
                            return action;
                        }
                        _ => {}
                    }
                } else {
                    // 일반 설정 편집 완료
                    let action = apply_setting_edit(s);
                    s.editing = false;
                    s.edit_input.clear();
                    return action;
                }
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
                s.pw_change_step = 0;
                s.pw_current_temp.clear();
            }
            KeyCode::Up if !s.editing => {
                if s.cursor > 0 { s.cursor -= 1; }
            }
            KeyCode::Down if !s.editing => {
                let max = category_item_count(s.category).saturating_sub(1);
                if s.cursor < max { s.cursor += 1; }
            }
            // 토글 항목 (네트워크 모드, 언어)은 Space/Enter 로 즉시 전환
            KeyCode::Char(' ') if !s.editing => {
                let action = toggle_setting_field(s);
                return action;
            }
            KeyCode::Enter if !s.editing => {
                // 토글 항목은 Enter로도 전환
                let action = toggle_or_edit(s);
                return action;
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

fn category_item_count(cat: SettingsCategory) -> usize {
    match cat {
        SettingsCategory::Select => 7,
        SettingsCategory::Profile => 3,    // ID(읽기전용), 닉네임, 비밀번호 변경
        SettingsCategory::Network => 3,    // 네트워크 모드, 포트, 최대 연결 수
        SettingsCategory::Chat => 1,       // 로그 경로
        SettingsCategory::File => 4,       // 다운로드 경로, 최대 동시 다운로드, 업로드 속도, 다운로드 속도
        SettingsCategory::RoomManage => 0,
        SettingsCategory::FriendManage => 0,
        SettingsCategory::Language => 1,   // 언어 선택
    }
}

/// 토글 가능한 항목이면 즉시 전환, 아니면 텍스트 편집 모드 진입.
fn toggle_or_edit(s: &mut SettingsState) -> TuiAction {
    match s.category {
        SettingsCategory::Network if s.cursor == 0 => {
            // 네트워크 모드 토글
            let new_val = if s.config.network_mode == "인터넷" { "인트라넷" } else { "인터넷" };
            s.config.network_mode = new_val.to_string();
            TuiAction::Command(AppCommand::UpdateConfigField {
                field: "network_mode".to_string(),
                value: new_val.to_string(),
            })
        }
        SettingsCategory::Language => {
            let new_val = if s.config.language == "Korean" { "English" } else { "Korean" };
            s.config.language = new_val.to_string();
            TuiAction::Command(AppCommand::UpdateConfigField {
                field: "language".to_string(),
                value: new_val.to_string(),
            })
        }
        _ => {
            // Profile > ID(cursor=0): 읽기전용, 편집 불가
            if s.category == SettingsCategory::Profile && s.cursor == 0 {
                return TuiAction::None;
            }
            // 텍스트 편집 모드 진입
            s.editing = true;
            s.edit_input.clear();
            if s.category == SettingsCategory::Profile && s.cursor == 2 {
                // 비밀번호 변경 1단계 시작
                s.pw_change_step = 1;
            } else {
                // 현재 값을 edit_input에 미리 채운다
                s.edit_input = current_field_value(s).to_string();
            }
            TuiAction::None
        }
    }
}

fn toggle_setting_field(s: &mut SettingsState) -> TuiAction {
    toggle_or_edit(s)
}

fn current_field_value(s: &SettingsState) -> &str {
    match s.category {
        SettingsCategory::Profile => match s.cursor {
            0 => &s.config.user_id,  // read-only
            1 => &s.config.nickname,
            _ => "",                 // 비밀번호: pw_change 로직에서 처리
        },
        SettingsCategory::Network => match s.cursor {
            0 => &s.config.network_mode,
            1 => &s.config.port,
            _ => &s.config.max_connections,
        },
        SettingsCategory::Chat => &s.config.log_path,
        SettingsCategory::File => match s.cursor {
            0 => &s.config.download_path,
            1 => &s.config.max_concurrent_dl,
            2 => &s.config.max_upload_kbps,
            _ => &s.config.max_download_kbps,
        },
        SettingsCategory::Language => &s.config.language,
        _ => "",
    }
}

fn apply_setting_edit(s: &SettingsState) -> TuiAction {
    let value = s.edit_input.clone();
    match s.category {
        SettingsCategory::Profile => match s.cursor {
            0 => TuiAction::None, // ID: 읽기전용
            1 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "nickname".to_string(),
                value,
            }),
            _ => TuiAction::None, // 비밀번호: 2단계 Enter 로직에서 처리
        },
        SettingsCategory::Network => match s.cursor {
            0 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "network_mode".to_string(),
                value,
            }),
            1 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "port".to_string(),
                value,
            }),
            2 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "max_connections".to_string(),
                value,
            }),
            _ => TuiAction::None,
        },
        SettingsCategory::Chat => TuiAction::Command(AppCommand::UpdateConfigField {
            field: "log_path".to_string(),
            value,
        }),
        SettingsCategory::File => match s.cursor {
            0 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "download_path".to_string(),
                value,
            }),
            1 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "max_concurrent_dl".to_string(),
                value,
            }),
            2 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "max_upload_kbps".to_string(),
                value,
            }),
            3 => TuiAction::Command(AppCommand::UpdateConfigField {
                field: "max_download_kbps".to_string(),
                value,
            }),
            _ => TuiAction::None,
        },
        SettingsCategory::Language => TuiAction::Command(AppCommand::UpdateConfigField {
            field: "language".to_string(),
            value,
        }),
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
            // 선택 토글 (12-tui.md: 폴더 선택 시 하위 파일 일괄 토글)
            if let Some(item) = s.items.get(s.cursor) {
                let is_dir = item.is_dir;
                let depth = item.depth;
                let new_val = !item.selected;

                if is_dir {
                    // 폴더: 현재 항목 + 더 깊은 depth의 연속 자식 항목 일괄 토글
                    let cursor = s.cursor;
                    let items_len = s.items.len();
                    let mut i = cursor;
                    while i < items_len {
                        let d = s.items[i].depth;
                        if i > cursor && d <= depth {
                            break; // 같은 depth 또는 상위 depth → 자식 범위 종료
                        }
                        let prev = s.items[i].selected;
                        s.items[i].selected = new_val;
                        if new_val && !prev {
                            s.selected_size += s.items[i].size;
                        } else if !new_val && prev {
                            s.selected_size = s.selected_size.saturating_sub(s.items[i].size);
                        }
                        i += 1;
                    }
                } else {
                    // 파일: 단일 항목 토글
                    s.items[s.cursor].selected = new_val;
                    if new_val {
                        s.selected_size += s.items[s.cursor].size;
                    } else {
                        s.selected_size = s.selected_size.saturating_sub(s.items[s.cursor].size);
                    }
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
            // 선택된 파일 전체 다운로드 시작 (12-tui.md: 파일 선택 화면)
            let files: Vec<_> = s.items.iter()
                .filter(|i| i.selected && !i.is_dir)
                .map(|i| (
                    i.file_hash,
                    i.name.clone(),
                    // 마지막 청크가 더 작을 수 있으므로 올림 계산
                    ((i.size + 262143) / 262144) as u32,
                ))
                .collect();
            if !files.is_empty() {
                let cmd = AppCommand::StartDownloads { files };
                return TuiAction::CommandAndGoto(cmd, Screen::Chat(ChatState::default()));
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
    // 초대 오버레이가 열려 있는 경우 먼저 처리
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
                s.pending_invites.remove(s.invite_cursor);
                if s.pending_invites.is_empty() { s.show_invite_overlay = false; }
                if s.invite_cursor > 0 && s.invite_cursor >= s.pending_invites.len() {
                    s.invite_cursor -= 1;
                }
                return TuiAction::Command(AppCommand::AcceptInvite { number: Some(number) });
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                let number = s.pending_invites[s.invite_cursor].number;
                s.pending_invites.remove(s.invite_cursor);
                if s.pending_invites.is_empty() { s.show_invite_overlay = false; }
                if s.invite_cursor > 0 && s.invite_cursor >= s.pending_invites.len() {
                    s.invite_cursor -= 1;
                }
                return TuiAction::Command(AppCommand::DeclineInvite { number: Some(number) });
            }
            KeyCode::Esc => {
                s.show_invite_overlay = false;
            }
            _ => {}
        }
        return TuiAction::None;
    }

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
        KeyCode::Up => {
            if s.feed_scroll > 0 { s.feed_scroll -= 1; }
        }
        KeyCode::Down => {
            s.feed_scroll += 1;
        }
        KeyCode::PageUp => {
            s.feed_scroll = s.feed_scroll.saturating_sub(3);
        }
        KeyCode::PageDown => {
            s.feed_scroll = s.feed_scroll.saturating_add(3);
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
        Ok(Command::Help) => {
            use crate::room::RoomStore;
            let now = RoomStore::now_ms();
            let lines = [
                "─── 도움말 ───────────────────────────────────────────",
                " 채팅 / 방",
                "  /quit              방에서 나가기",
                "  /peers             접속 중인 피어 목록 표시",
                "  /nick <닉네임>     닉네임 변경",
                "  /invite            초대 코드 생성",
                "  /accept [번호]     초대 수락 (번호 생략 시 첫 번째)",
                " 파일 공유",
                "  /share <경로>      파일 또는 폴더 공유 등록",
                "  /list              이 방의 공유 파일 목록 표시",
                "  /download <파일>   파일 다운로드 요청",
                "  /downloads         진행 중인 다운로드 목록",
                "  /seed              시딩 중인 파일 목록",
                " 다운로드 관리",
                "  /pause <번호>      다운로드 일시정지",
                "  /resume <번호>     다운로드 재개",
                "  /cancel <번호>     다운로드 취소",
                "  /top <번호>        다운로드 순서를 맨 위로",
                "  /up <번호>         다운로드 순서를 한 칸 위로",
                "  /down <번호>       다운로드 순서를 한 칸 아래로",
                " 시딩 관리",
                "  /seed-pause <번호>   시딩 일시정지",
                "  /seed-resume <번호>  시딩 재개",
                "  /remove <번호>       시드 제거 (파일 유지)",
                "  /remove-all <번호>   시드 제거 및 파일 삭제",
                " 기타",
                "  /help              이 도움말 표시",
                "  Ctrl+C             방 나가기",
                "─────────────────────────────────────────────────────",
            ];
            for line in lines {
                s.feed.push(FeedItem {
                    timestamp_ms: now,
                    content: FeedContent::System(line.to_string()),
                });
            }
        }
        Ok(Command::List) => {
            return TuiAction::Command(AppCommand::ListFiles);
        }
        Ok(Command::Downloads) => {
            return TuiAction::Command(AppCommand::ListDownloads);
        }
        Ok(Command::Seed) => {
            return TuiAction::Command(AppCommand::ListSeeds);
        }
        Ok(_) => {}
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
