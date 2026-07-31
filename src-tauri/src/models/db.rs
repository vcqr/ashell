use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::errors::AppResult;

pub type DbPool = SqlitePool;

/// 初始化连接池并执行 migration
pub async fn init_pool(db_path: &std::path::Path) -> AppResult<DbPool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let url = format!("sqlite://{}", db_path.to_string_lossy().replace('\\', "/"));
    let opts = SqliteConnectOptions::from_str(&url)
        .map_err(sqlx::Error::from)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &DbPool) -> AppResult<()> {
    // 版本追踪表：记录已执行的迁移版本号
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS schema_version (
            version     INTEGER PRIMARY KEY,
            applied_at  TEXT DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(pool)
    .await?;

    let current: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version")
        .fetch_one(pool)
        .await?;

    // v1: 基础表结构
    if current < 1 {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS groups (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id   INTEGER NOT NULL DEFAULT 0,
                name        TEXT NOT NULL,
                path        TEXT NOT NULL DEFAULT '/',
                level       INTEGER NOT NULL DEFAULT 0,
                is_del      INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_groups_parent ON groups(parent_id);")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_groups_path ON groups(path);")
            .execute(pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS hosts (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                gid         INTEGER NOT NULL DEFAULT 0,
                name        TEXT NOT NULL,
                icon        TEXT,
                color       TEXT,
                addr        TEXT NOT NULL,
                port        TEXT NOT NULL DEFAULT '22',
                username    TEXT NOT NULL,
                password    TEXT,
                desc        TEXT,
                is_del      INTEGER NOT NULL DEFAULT 0,
                private_key TEXT,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_hosts_gid ON hosts(gid);")
            .execute(pool)
            .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (1)")
            .execute(pool)
            .await?;
    }

    // v2: hosts 增加 private_key_path 字段
    if current < 2 {
        sqlx::query("ALTER TABLE hosts ADD COLUMN private_key_path TEXT")
            .execute(pool)
            .await
            .ok();

        sqlx::query("INSERT INTO schema_version (version) VALUES (2)")
            .execute(pool)
            .await?;
    }

    // v3: hosts 增加 protocol + 串口配置字段
    if current < 3 {
        sqlx::query("ALTER TABLE hosts ADD COLUMN protocol TEXT NOT NULL DEFAULT 'ssh'")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN baud_rate INTEGER")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN data_bits INTEGER")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN stop_bits INTEGER")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN parity TEXT")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN flow_control TEXT")
            .execute(pool)
            .await
            .ok();

        sqlx::query("INSERT INTO schema_version (version) VALUES (3)")
            .execute(pool)
            .await?;
    }

    // v4: hosts 增加 SSH 保活/超时/idle 配置字段
    if current < 4 {
        sqlx::query("ALTER TABLE hosts ADD COLUMN keepalive_interval INTEGER")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN inactivity_timeout INTEGER")
            .execute(pool)
            .await
            .ok();
        sqlx::query("ALTER TABLE hosts ADD COLUMN idle_send_interval INTEGER")
            .execute(pool)
            .await
            .ok();

        sqlx::query("INSERT INTO schema_version (version) VALUES (4)")
            .execute(pool)
            .await?;
    }

    // v5: AI 供应商表
    if current < 5 {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ai_providers (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                sidecar_type TEXT NOT NULL DEFAULT 'claude',
                sort_order  INTEGER NOT NULL DEFAULT 0,
                is_active   INTEGER NOT NULL DEFAULT 0,
                url         TEXT NOT NULL DEFAULT '',
                api_key     TEXT NOT NULL DEFAULT '',
                model_ids   TEXT NOT NULL DEFAULT '',
                active_model_id TEXT NOT NULL DEFAULT '',
                pi_provider    TEXT NOT NULL DEFAULT '',
                pi_model       TEXT NOT NULL DEFAULT '',
                pi_model_ids   TEXT NOT NULL DEFAULT '',
                pi_base_url    TEXT NOT NULL DEFAULT '',
                pi_api_key     TEXT NOT NULL DEFAULT '',
                pi_api         TEXT NOT NULL DEFAULT '',
                pi_thinking_level TEXT NOT NULL DEFAULT '',
                is_del      INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (5)")
            .execute(pool)
            .await?;
    }

    Ok(())
}
