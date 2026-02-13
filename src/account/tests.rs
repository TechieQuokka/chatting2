#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::account::session::{
        change_password, delete_account, login, recover_stale_tmp, register, AccountPaths,
    };
    use crate::account::pid::{PidLock, PidError};

    fn tmp_paths() -> (TempDir, AccountPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AccountPaths::new(dir.path().to_path_buf());
        (dir, paths)
    }

    #[test]
    fn test_register_and_login() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "alice", "Alice", b"pw123", "/downloads", "/logs").unwrap();

        let (identity, config) = login(&paths, "alice", b"pw123").unwrap();
        assert_eq!(config.nickname, "Alice");
        // PeerId는 멀티해시 포맷이므로 32바이트 이상
        assert!(!identity.peer_id.to_bytes().is_empty());
    }

    #[test]
    fn test_register_duplicate_id() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "bob", "Bob", b"pw", "/dl", "/log").unwrap();
        let result = register(&paths, "bob", "Bob2", b"pw2", "/dl", "/log");
        assert!(result.is_err(), "중복 ID는 에러여야 함");
    }

    #[test]
    fn test_login_wrong_password() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "carol", "Carol", b"correct", "/dl", "/log").unwrap();
        let result = login(&paths, "carol", b"wrong");
        assert!(result.is_err(), "잘못된 패스워드는 에러여야 함");
    }

    #[test]
    fn test_login_nonexistent_account() {
        let (_dir, paths) = tmp_paths();
        let result = login(&paths, "nobody", b"pw");
        assert!(result.is_err());
    }

    #[test]
    fn test_change_password() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "dave", "Dave", b"old_pw", "/dl", "/log").unwrap();
        change_password(&paths, "dave", b"old_pw", b"new_pw").unwrap();

        // 새 PW로 로그인 성공
        assert!(login(&paths, "dave", b"new_pw").is_ok());
        // 이전 PW로 로그인 실패
        assert!(login(&paths, "dave", b"old_pw").is_err());
    }

    #[test]
    fn test_change_password_wrong_current() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "eve", "Eve", b"pw", "/dl", "/log").unwrap();
        let result = change_password(&paths, "eve", b"wrong", b"new");
        assert!(result.is_err(), "잘못된 현재 PW면 변경 실패해야 함");
    }

    #[test]
    fn test_delete_account() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "frank", "Frank", b"pw", "/dl", "/log").unwrap();
        delete_account(&paths, "frank", b"pw").unwrap();

        // 삭제 후 로그인 불가
        let result = login(&paths, "frank", b"pw");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_wrong_password() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "grace", "Grace", b"pw", "/dl", "/log").unwrap();
        let result = delete_account(&paths, "grace", b"wrong");
        assert!(result.is_err());

        // 계정이 그대로 있어야 함
        assert!(login(&paths, "grace", b"pw").is_ok());
    }

    #[test]
    fn test_recover_stale_tmp() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "henry", "Henry", b"pw", "/dl", "/log").unwrap();

        // 수동으로 .tmp 파일 생성 (크래시 시뮬레이션)
        let tmp_path = paths.user_dir("henry").join("identity.enc.tmp");
        std::fs::write(&tmp_path, b"stale").unwrap();
        assert!(tmp_path.exists());

        recover_stale_tmp(&paths, "henry");
        assert!(!tmp_path.exists(), ".tmp 파일이 정리되어야 함");
    }

    #[test]
    fn test_pid_lock_acquire_and_release() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        {
            let _lock = PidLock::acquire(&pid_path).unwrap();
            assert!(pid_path.exists(), "PID 파일이 생성되어야 함");
        }
        // Drop 후 파일 삭제
        assert!(!pid_path.exists(), "PID 파일이 삭제되어야 함");
    }

    #[test]
    fn test_pid_lock_stale_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("test.pid");

        // 죽은 프로세스의 PID (0 또는 매우 큰 값 — Windows에서는 stale 처리)
        std::fs::write(&pid_path, "99999999").unwrap();

        // stale lock이므로 획득 가능해야 함
        let result = PidLock::acquire(&pid_path);
        // Windows에서는 is_process_alive가 false를 반환하므로 항상 성공
        // Unix에서는 PID 99999999가 없으면 성공
        assert!(result.is_ok(), "stale lock은 교체되어야 함: {result:?}");
    }

    #[test]
    fn test_invalid_id_rejected() {
        let (_dir, paths) = tmp_paths();

        // 너무 짧은 ID
        assert!(register(&paths, "ab", "Test", b"pw", "/dl", "/log").is_err());
        // 특수문자 포함
        assert!(register(&paths, "bad-id!", "Test", b"pw", "/dl", "/log").is_err());
        // 너무 긴 ID
        let long_id = "a".repeat(33);
        assert!(register(&paths, &long_id, "Test", b"pw", "/dl", "/log").is_err());
    }

    #[test]
    fn test_multiple_accounts() {
        let (_dir, paths) = tmp_paths();

        register(&paths, "user1", "User One", b"pw1", "/dl", "/log").unwrap();
        register(&paths, "user2", "User Two", b"pw2", "/dl", "/log").unwrap();

        let (_, config1) = login(&paths, "user1", b"pw1").unwrap();
        let (_, config2) = login(&paths, "user2", b"pw2").unwrap();

        assert_eq!(config1.nickname, "User One");
        assert_eq!(config2.nickname, "User Two");

        // 교차 패스워드는 실패해야 함
        assert!(login(&paths, "user1", b"pw2").is_err());
    }
}
