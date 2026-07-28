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
