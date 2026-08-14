use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use rand::RngCore;

use crate::errors::{AppError, AppResult};

/// 使用 AES-256-GCM 加密。输出格式：base64(nonce(12B) || ciphertext)
pub fn encrypt(key: &[u8; 32], plaintext: &str) -> AppResult<String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Crypto(e.to_string()))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(B64.encode(out))
}

/// 解密 encrypt 输出的字符串
pub fn decrypt(key: &[u8; 32], encoded: &str) -> AppResult<String> {
    let raw = B64
        .decode(encoded.as_bytes())
        .map_err(|e| AppError::Crypto(format!("base64 decode: {e}")))?;
    if raw.len() < 13 {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ct)
        .map_err(|e| AppError::Crypto(e.to_string()))?;
    String::from_utf8(plain).map_err(|e| AppError::Crypto(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_a() -> [u8; 32] {
        [1u8; 32]
    }

    fn key_b() -> [u8; 32] {
        [2u8; 32]
    }

    #[test]
    fn roundtrip_plain_ascii() {
        let ct = encrypt(&key_a(), "s3cret-password").unwrap();
        assert_ne!(ct, "s3cret-password");
        assert_eq!(decrypt(&key_a(), &ct).unwrap(), "s3cret-password");
    }

    #[test]
    fn roundtrip_cjk_and_multiline() {
        // 凭证可能含中文备注 / 多行私钥
        let plain = "密码 p@ss\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc\ndef\n";
        let ct = encrypt(&key_a(), plain).unwrap();
        assert_eq!(decrypt(&key_a(), &ct).unwrap(), plain);
    }

    #[test]
    fn roundtrip_empty_string() {
        let ct = encrypt(&key_a(), "").unwrap();
        assert_eq!(decrypt(&key_a(), &ct).unwrap(), "");
    }

    #[test]
    fn nonce_is_random_per_encryption() {
        // 同一明文两次加密应产生不同密文（随机 nonce），且都能解回
        let a = encrypt(&key_a(), "same").unwrap();
        let b = encrypt(&key_a(), "same").unwrap();
        assert_ne!(a, b);
        assert_eq!(decrypt(&key_a(), &a).unwrap(), "same");
        assert_eq!(decrypt(&key_a(), &b).unwrap(), "same");
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt(&key_a(), "secret").unwrap();
        assert!(decrypt(&key_b(), &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let ct = encrypt(&key_a(), "secret").unwrap();
        let mut raw = B64.decode(ct.as_bytes()).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        let tampered = B64.encode(raw);
        assert!(decrypt(&key_a(), &tampered).is_err());
    }

    #[test]
    fn too_short_input_fails() {
        let short = B64.encode([0u8; 8]);
        assert!(decrypt(&key_a(), &short).is_err());
        assert!(decrypt(&key_a(), "not-base64-!!!").is_err());
    }
}
