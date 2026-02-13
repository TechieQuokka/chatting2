#[cfg(test)]
mod tests {
    use crate::crypto::{decrypt, derive_key, encrypt, load_enc, save_enc};
    use tempfile;

    #[test]
    fn test_derive_key_deterministic() {
        let password = b"my_password";
        let salt = b"unique_salt_16by"; // 16 bytes

        let key1 = derive_key(password, salt).unwrap();
        let key2 = derive_key(password, salt).unwrap();
        assert_eq!(*key1, *key2, "동일 입력이면 동일 키가 나와야 함");
    }

    #[test]
    fn test_derive_key_different_salt() {
        let password = b"my_password";
        let salt1 = b"salt_aaaaaaaaaa1";
        let salt2 = b"salt_aaaaaaaaaa2";

        let key1 = derive_key(password, salt1).unwrap();
        let key2 = derive_key(password, salt2).unwrap();
        assert_ne!(*key1, *key2, "salt가 다르면 키도 달라야 함");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let password = b"test_password";
        let salt = b"test_salt_16byte";
        let key = derive_key(password, salt).unwrap();

        let plaintext = b"hello, secure world!";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_nonce_prepended() {
        let key = [0u8; 32];
        let plaintext = b"data";
        let encrypted = encrypt(&key, plaintext).unwrap();

        // nonce 12 bytes + GCM tag 16 bytes + data 4 bytes = 32 bytes minimum
        assert!(encrypted.as_bytes().len() >= 12 + 16 + plaintext.len());
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = derive_key(b"password1", b"salt_aaaaaaaaaa1").unwrap();
        let key2 = derive_key(b"password2", b"salt_aaaaaaaaaa1").unwrap();

        let encrypted = encrypt(&key1, b"secret").unwrap();
        let result = decrypt(&key2, &encrypted);
        assert!(result.is_err(), "잘못된 키로 복호화는 실패해야 함");
    }

    #[test]
    fn test_encrypt_nondeterministic() {
        let key = [0u8; 32];
        let plaintext = b"same data";

        let enc1 = encrypt(&key, plaintext).unwrap();
        let enc2 = encrypt(&key, plaintext).unwrap();

        // nonce가 매번 랜덤이므로 암호문이 달라야 함
        assert_ne!(enc1.as_bytes(), enc2.as_bytes(), "랜덤 nonce로 암호문이 달라야 함");
    }

    #[test]
    fn test_enc_file_roundtrip() {
        let key = derive_key(b"file_test_pw", b"file_test_salt_!").unwrap();
        let plaintext = b"file content to protect";

        let tmp = tempfile::Builder::new()
            .suffix(".enc")
            .tempfile()
            .unwrap();
        let path = tmp.path().to_path_buf();
        drop(tmp); // 파일 닫기 (rename을 위해)

        save_enc(&path, &key, plaintext).unwrap();
        let loaded = load_enc(&path, &key).unwrap();

        assert_eq!(plaintext.as_slice(), loaded.as_slice());
        std::fs::remove_file(&path).ok();
    }
}
