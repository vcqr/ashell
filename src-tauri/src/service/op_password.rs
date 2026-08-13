use subtle::ConstantTimeEq;

use crate::config::crypto;
use crate::errors::{AppError, AppResult};
use crate::models::DbPool;

const SETTING_OP_PASSWORD: &str = "op_password";

/// 操作密码是否已设置
pub async fn is_set(pool: &DbPool) -> AppResult<bool> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(SETTING_OP_PASSWORD)
        .fetch_optional(pool)
        .await?;
    Ok(v.is_some())
}

/// 首次设置操作密码（用 crypto_key 加密后存入 app_settings）
pub async fn set(pool: &DbPool, key: &[u8; 32], password: &str) -> AppResult<()> {
    if password.is_empty() {
        return Err(AppError::BadRequest("operation password cannot be empty".into()));
    }
    let encrypted = crypto::encrypt(key, password)?;
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(SETTING_OP_PASSWORD)
    .bind(encrypted)
    .execute(pool)
    .await?;
    Ok(())
}

/// 校验操作密码（常量时间比较，防时序攻击）
pub async fn verify(pool: &DbPool, key: &[u8; 32], password: &str) -> AppResult<bool> {
    let v: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(SETTING_OP_PASSWORD)
        .fetch_optional(pool)
        .await?;
    match v {
        Some(stored) => {
            let decrypted = crypto::decrypt(key, &stored)?;
            Ok(decrypted.as_bytes().ct_eq(password.as_bytes()).into())
        }
        None => Ok(false),
    }
}

/// 修改操作密码（需验证旧密码）
pub async fn change(pool: &DbPool, key: &[u8; 32], old: &str, new: &str) -> AppResult<()> {
    if !verify(pool, key, old).await? {
        return Err(AppError::BadRequest("incorrect operation password".into()));
    }
    set(pool, key, new).await
}

/// 清除操作密码（需验证当前密码）
pub async fn clear(pool: &DbPool, key: &[u8; 32], password: &str) -> AppResult<()> {
    if !verify(pool, key, password).await? {
        return Err(AppError::BadRequest("incorrect operation password".into()));
    }
    sqlx::query("DELETE FROM app_settings WHERE key = ?")
        .bind(SETTING_OP_PASSWORD)
        .execute(pool)
        .await?;
    Ok(())
}
