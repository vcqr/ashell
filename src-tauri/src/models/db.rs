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

    // v6: AI 供应商与 sidecar 引擎解耦
    // 供应商表重建为纯端点定义（丢弃旧的 sidecar 混合字段），
    // 引擎配置（关联供应商/激活模型/thinking level）独立成表。
    // 旧数据由 service 层在表为空时从 .env 重新迁移。
    if current < 6 {
        sqlx::query("DROP TABLE IF EXISTS ai_providers")
            .execute(pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE ai_providers (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                api_type    TEXT NOT NULL DEFAULT 'openai-completions',
                base_url    TEXT NOT NULL DEFAULT '',
                api_key     TEXT NOT NULL DEFAULT '',
                model_ids   TEXT NOT NULL DEFAULT '',
                sort_order  INTEGER NOT NULL DEFAULT 0,
                is_del      INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT DEFAULT (datetime('now')),
                updated_at  TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ai_engines (
                engine          TEXT PRIMARY KEY,
                provider_id     TEXT,
                active_model_id TEXT NOT NULL DEFAULT '',
                thinking_level  TEXT NOT NULL DEFAULT 'off',
                updated_at      TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        // KV 设置表（当前激活引擎等）
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS app_settings (
                key   TEXT PRIMARY KEY,
                value TEXT
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (6)")
            .execute(pool)
            .await?;
    }

    // v7: AI 常用语（快捷收藏的用户消息）
    if current < 7 {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS quick_phrases (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                content     TEXT NOT NULL,
                sort_order  INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (7)")
            .execute(pool)
            .await?;
    }

    // v8: 模板命令（预置命令片段）
    if current < 8 {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS command_templates (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                title       TEXT NOT NULL,
                command     TEXT NOT NULL,
                description TEXT,
                sort_order  INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (8)")
            .execute(pool)
            .await?;
    }

    // v9: hosts 增加跳板机引用（指向另一台 SSH 主机的 id，仅支持一级）
    if current < 9 {
        sqlx::query("ALTER TABLE hosts ADD COLUMN jump_host_id INTEGER")
            .execute(pool)
            .await
            .ok();

        sqlx::query("INSERT INTO schema_version (version) VALUES (9)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    const LATEST_VERSION: i64 = 9;

    async fn current_version(pool: &DbPool) -> i64 {
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn table_names(pool: &DbPool) -> Vec<String> {
        let rows = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        rows.iter().map(|r| r.get::<String, _>("name")).collect()
    }

    #[tokio::test]
    async fn fresh_db_migrates_to_latest() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("fresh.db");
        let pool = init_pool(&db_path).await.unwrap();

        assert_eq!(current_version(&pool).await, LATEST_VERSION);

        let tables = table_names(&pool).await;
        for expected in [
            "groups",
            "hosts",
            "ai_providers",
            "ai_engines",
            "app_settings",
            "quick_phrases",
            "command_templates",
            "schema_version",
        ] {
            assert!(tables.contains(&expected.to_string()), "missing table {expected}");
        }

        pool.close().await;
    }

    #[tokio::test]
    async fn migration_is_idempotent_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("idempotent.db");
        let pool = init_pool(&db_path).await.unwrap();

        sqlx::query(
            "INSERT INTO groups (parent_id, name, path, level) VALUES (0, 'prod', '/', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // 释放连接后对同一文件再次 init：迁移不应重复执行，数据必须保留
        pool.close().await;
        let pool2 = init_pool(&db_path).await.unwrap();

        assert_eq!(current_version(&pool2).await, LATEST_VERSION);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM groups WHERE name = 'prod'")
            .fetch_one(&pool2)
            .await
            .unwrap();
        assert_eq!(count, 1);

        pool2.close().await;
    }

    /// 模拟 v1 旧库（0.1.x 早期版本只建了 groups/hosts）升级到最新：
    /// 既有主机数据不能丢，v2+ 新增列必须可用。
    #[tokio::test]
    async fn upgrade_from_v1_preserves_data_and_adds_columns() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("legacy_v1.db");

        {
            let opts = SqliteConnectOptions::from_str(&format!(
                "sqlite://{}",
                db_path.to_string_lossy().replace('\\', "/")
            ))
            .unwrap()
            .create_if_missing(true);
            let legacy = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();

            // v1 的表结构（与 migrate 中 current < 1 分支一致）
            sqlx::query(
                r#"
                CREATE TABLE groups (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    parent_id   INTEGER NOT NULL DEFAULT 0,
                    name        TEXT NOT NULL,
                    path        TEXT NOT NULL DEFAULT '/',
                    level       INTEGER NOT NULL DEFAULT 0,
                    is_del      INTEGER NOT NULL DEFAULT 0,
                    created_at  TEXT DEFAULT (datetime('now')),
                    updated_at  TEXT DEFAULT (datetime('now'))
                );
                CREATE TABLE hosts (
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
                CREATE TABLE schema_version (
                    version     INTEGER PRIMARY KEY,
                    applied_at  TEXT DEFAULT (datetime('now'))
                );
                INSERT INTO schema_version (version) VALUES (1);
                "#,
            )
            .execute(&legacy)
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO hosts (gid, name, addr, port, username, password) \
                 VALUES (0, 'legacy-host', '192.168.1.10', '22', 'admin', 'old-cipher')",
            )
            .execute(&legacy)
            .await
            .unwrap();

            legacy.close().await;
        }

        // 跑完整迁移
        let pool = init_pool(&db_path).await.unwrap();

        assert_eq!(current_version(&pool).await, LATEST_VERSION);

        // 旧数据完整保留
        let row = sqlx::query("SELECT name, addr, username, password FROM hosts WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("name"), "legacy-host");
        assert_eq!(row.get::<String, _>("addr"), "192.168.1.10");
        assert_eq!(row.get::<String, _>("username"), "admin");
        assert_eq!(row.get::<String, _>("password"), "old-cipher");

        // v2+ 新增列可写（v2 private_key_path、v3 protocol、v9 jump_host_id）
        sqlx::query(
            "UPDATE hosts SET private_key_path = '/tmp/id_ed25519', protocol = 'ssh', \
             jump_host_id = 3 WHERE id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();

        let proto: String =
            sqlx::query_scalar("SELECT protocol FROM hosts WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(proto, "ssh");

        pool.close().await;
    }
}
