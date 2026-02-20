/// CLI 명령어 파싱.
///
/// 채팅 입력창에서 `/`로 시작하는 문자열을 파싱한다.
/// 명령어가 아닌 일반 입력은 `Command::Message`로 처리.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ── 채팅/방 ──────────────────────────────────────────────────────────────
    Quit,
    Peers,
    Add { number: u32 },
    Nick { nickname: String },
    Invite,
    Accept { number: Option<u32> },
    Approve { number: Option<u32> },
    Reject { number: Option<u32> },
    Help,
    Refresh,

    // ── 파일 공유 ─────────────────────────────────────────────────────────────
    Share { path: String },
    List,
    Download { target: String, select: bool },
    Downloads,
    Seed,

    // ── 다운로드 관리 ─────────────────────────────────────────────────────────
    Pause { number: u32 },
    Resume { number: u32 },
    Cancel { number: u32 },
    Top { number: u32 },
    Up { number: u32 },
    Down { number: u32 },

    // ── 시딩 관리 ─────────────────────────────────────────────────────────────
    SeedPause { number: u32 },
    SeedResume { number: u32 },
    Remove { number: u32 },
    RemoveAll { number: u32 },

    // ── 일반 메시지 ───────────────────────────────────────────────────────────
    Message { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnknownCommand(String),
    MissingArgument(String),
    InvalidNumber(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCommand(s) => write!(f, "알 수 없는 명령어: /{s}"),
            Self::MissingArgument(s) => write!(f, "인자 누락: {s}"),
            Self::InvalidNumber(s) => write!(f, "숫자가 필요합니다: {s}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// 사용자 입력을 파싱해 `Command`를 반환한다.
pub fn parse(input: &str) -> Result<Command, ParseError> {
    let trimmed = input.trim();

    if !trimmed.starts_with('/') {
        return Ok(Command::Message { text: trimmed.to_string() });
    }

    // `/command arg1 arg2` 분리
    let mut parts = trimmed[1..].splitn(3, ' ');
    let cmd = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();

    match cmd {
        "quit" => Ok(Command::Quit),
        "peers" => Ok(Command::Peers),
        "help" => Ok(Command::Help),
        "invite" => Ok(Command::Invite),
        "list" => Ok(Command::List),
        "downloads" => Ok(Command::Downloads),
        "seed" => Ok(Command::Seed),
        "refresh" => Ok(Command::Refresh),

        "add" => {
            let n = parse_number(rest.first().copied(), "add")?;
            Ok(Command::Add { number: n })
        }
        "nick" => {
            let nick = rest.first().copied().unwrap_or("").trim().to_string();
            if nick.is_empty() {
                return Err(ParseError::MissingArgument("nick <새닉네임>".to_string()));
            }
            Ok(Command::Nick { nickname: nick })
        }
        "accept" => {
            let number = rest.first().copied().and_then(|s| s.trim().parse::<u32>().ok());
            Ok(Command::Accept { number })
        }
        "approve" => {
            let number = rest.first().copied().and_then(|s| s.trim().parse::<u32>().ok());
            Ok(Command::Approve { number })
        }
        "reject" => {
            let number = rest.first().copied().and_then(|s| s.trim().parse::<u32>().ok());
            Ok(Command::Reject { number })
        }
        "share" => {
            let mut path = rest.join(" ").trim().to_string();
            if path.is_empty() {
                return Err(ParseError::MissingArgument("share <경로>".to_string()));
            }
            // 따옴표 제거 (Windows 경로의 공백 처리용)
            if (path.starts_with('"') && path.ends_with('"'))
                || (path.starts_with('\'') && path.ends_with('\'')) {
                path = path[1..path.len()-1].to_string();
            }
            Ok(Command::Share { path })
        }
        "download" => {
            let joined = rest.join(" ");
            let args: Vec<&str> = joined.split_whitespace().collect();
            if args.is_empty() {
                return Err(ParseError::MissingArgument("download <파일>".to_string()));
            }
            let select = args.contains(&"--select");
            let target = args.iter()
                .filter(|&&a| a != "--select")
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            Ok(Command::Download { target, select })
        }
        "pause" => Ok(Command::Pause { number: parse_number(rest.first().copied(), "pause")? }),
        "resume" => Ok(Command::Resume { number: parse_number(rest.first().copied(), "resume")? }),
        "cancel" => Ok(Command::Cancel { number: parse_number(rest.first().copied(), "cancel")? }),
        "top" => Ok(Command::Top { number: parse_number(rest.first().copied(), "top")? }),
        "up" => Ok(Command::Up { number: parse_number(rest.first().copied(), "up")? }),
        "down" => Ok(Command::Down { number: parse_number(rest.first().copied(), "down")? }),
        "seed-pause" => Ok(Command::SeedPause { number: parse_number(rest.first().copied(), "seed-pause")? }),
        "seed-resume" => Ok(Command::SeedResume { number: parse_number(rest.first().copied(), "seed-resume")? }),
        "remove" => Ok(Command::Remove { number: parse_number(rest.first().copied(), "remove")? }),
        "remove-all" => Ok(Command::RemoveAll { number: parse_number(rest.first().copied(), "remove-all")? }),

        other => Err(ParseError::UnknownCommand(other.to_string())),
    }
}

fn parse_number(s: Option<&str>, cmd: &str) -> Result<u32, ParseError> {
    s.ok_or_else(|| ParseError::MissingArgument(cmd.to_string()))?
        .trim()
        .parse::<u32>()
        .map_err(|_| ParseError::InvalidNumber(cmd.to_string()))
}

/// 명령어 히스토리 (↑↓ 탐색).
#[derive(Debug, Default)]
pub struct CommandHistory {
    entries: Vec<String>,
    cursor: Option<usize>,
}

impl CommandHistory {
    /// 새 항목 추가 (빈 문자열·직전 항목과 동일한 경우 무시).
    pub fn push(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.entries.last().map(|s| s.as_str()) == Some(trimmed) {
            self.cursor = None;
            return;
        }
        self.entries.push(trimmed.to_string());
        self.cursor = None;
    }

    /// ↑ 이전 항목.
    pub fn prev(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        self.cursor = Some(match self.cursor {
            None => self.entries.len() - 1,
            Some(0) => 0,
            Some(n) => n - 1,
        });
        self.cursor.map(|i| self.entries[i].as_str())
    }

    /// ↓ 다음 항목. 끝에 도달하면 `None` (빈 입력창 복원).
    pub fn next(&mut self) -> Option<&str> {
        match self.cursor {
            None => None,
            Some(n) if n + 1 >= self.entries.len() => {
                self.cursor = None;
                None
            }
            Some(n) => {
                self.cursor = Some(n + 1);
                Some(self.entries[n + 1].as_str())
            }
        }
    }

    /// 탐색 커서 초기화 (직접 입력 시).
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
    }
}
