//! SSH 主机密钥指纹（TOFU）存取。
//!
//! 按 `addr + port + key_type` 唯一记录一台服务器出示过的主机密钥指纹：
//! 首次连接由用户确认后写入；之后每次握手比对，一致静默放行，不一致则
//! 要求用户再次确认（可能是重装/换 key，也可能是中间人攻击）。

use crate::errors::AppResult;
use crate::models::{DbPool, KnownHost};

/// 查询某地址某密钥类型已信任的指纹；未记录返回 None
pub async fn lookup(
    pool: &DbPool,
    addr: &str,
    port: u16,
    key_type: &str,
) -> AppResult<Option<String>> {
    let fp: Option<String> = sqlx::query_scalar(
        "SELECT fingerprint FROM known_hosts WHERE addr = ? AND port = ? AND key_type = ?",
    )
    .bind(addr)
    .bind(port as i64)
    .bind(key_type)
    .fetch_optional(pool)
    .await?;
    Ok(fp)
}

/// 信任（或变更后更新）某地址某密钥类型的指纹
pub async fn trust(
    pool: &DbPool,
    addr: &str,
    port: u16,
    key_type: &str,
    fingerprint: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO known_hosts (addr, port, key_type, fingerprint)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(addr, port, key_type)
        DO UPDATE SET fingerprint = excluded.fingerprint,
                      updated_at  = datetime('now')
        "#,
    )
    .bind(addr)
    .bind(port as i64)
    .bind(key_type)
    .bind(fingerprint)
    .execute(pool)
    .await?;
    Ok(())
}

/// 全部已信任指纹（设置页管理列表）
pub async fn list(pool: &DbPool) -> AppResult<Vec<KnownHost>> {
    let rows = sqlx::query_as::<_, KnownHost>(
        "SELECT id, addr, port, key_type, fingerprint, created_at, updated_at \
         FROM known_hosts ORDER BY addr, port",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 删除一条信任记录（下次连接将重新确认）
pub async fn remove(pool: &DbPool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM known_hosts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
