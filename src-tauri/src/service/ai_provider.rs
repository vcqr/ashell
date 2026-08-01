use uuid::Uuid;

use crate::ai_env::{read_env_config, write_env_config, AiModelConfig};
use crate::config::crypto;
use crate::errors::{AppError, AppResult};
use crate::models::{
    AiEngine, AiEngineUpdate, AiEnginesState, AiProvider, AiProviderCreate, AiProviderUpdate, DbPool,
};

pub const ENGINE_CLAUDE: &str = "claude";
pub const ENGINE_PI: &str = "pi";

const SETTING_ACTIVE_ENGINE: &str = "ai_active_engine";

/// 加密非空字符串；空串保持空串
fn enc_str(key: &[u8; 32], v: &str) -> AppResult<String> {
    if v.is_empty() {
        return Ok(String::new());
    }
    Ok(crypto::encrypt(key, v)?)
}

/// 解密非空字符串
fn dec_str(key: &[u8; 32], v: &str) -> AppResult<String> {
    if v.is_empty() {
        return Ok(String::new());
    }
    crypto::decrypt(key, v)
}

/// 解密 provider 的 api_key（读取后调用）
fn decrypt_provider_key(key: &[u8; 32], p: &mut AiProvider) -> AppResult<()> {
    p.api_key = dec_str(key, &p.api_key)?;
    Ok(())
}

/// 候选模型列表中解析有效模型 ID：在列表中则保留，否则取第一个
fn resolve_model_id(model_ids: &str, want: &str) -> String {
    let list: Vec<&str> = model_ids.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if list.contains(&want) {
        want.to_string()
    } else {
        list.first().map(|s| s.to_string()).unwrap_or_default()
    }
}

/// 从供应商名称派生 Pi sidecar 的 PI_PROVIDER（注册表 key）：
/// 小写 + 仅保留字母数字，空则 "custom"。
fn derive_pi_provider(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if s.is_empty() {
        "custom".to_string()
    } else {
        s
    }
}

// ────────────────────────── 供应商 CRUD ──────────────────────────

/// 列出所有未删除的供应商。若表为空且 .env 有配置，自动迁移。
pub async fn list(pool: &DbPool, key: &[u8; 32]) -> AppResult<Vec<AiProvider>> {
    migrate_from_env_if_needed(pool, key).await?;

    let mut providers: Vec<AiProvider> =
        sqlx::query_as("SELECT * FROM ai_providers WHERE is_del = 0 ORDER BY sort_order, created_at")
            .fetch_all(pool)
            .await?;
    for p in &mut providers {
        decrypt_provider_key(key, p)?;
    }
    Ok(providers)
}

pub async fn get_by_id(pool: &DbPool, key: &[u8; 32], id: &str) -> AppResult<AiProvider> {
    let mut provider: AiProvider =
        sqlx::query_as("SELECT * FROM ai_providers WHERE id = ? AND is_del = 0")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => AppError::NotFound("AI provider not found".into()),
                other => AppError::Db(other),
            })?;
    decrypt_provider_key(key, &mut provider)?;
    Ok(provider)
}

pub async fn create(pool: &DbPool, key: &[u8; 32], input: AiProviderCreate) -> AppResult<AiProvider> {
    if input.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".into()));
    }

    let id = insert_provider(
        pool,
        key,
        input.name.trim(),
        input.api_type.as_deref().unwrap_or("openai-completions"),
        input.base_url.as_deref().unwrap_or(""),
        input.api_key.as_deref().unwrap_or(""),
        input.model_ids.as_deref().unwrap_or(""),
    )
    .await?;

    get_by_id(pool, key, &id).await
}

pub async fn update(
    pool: &DbPool,
    key: &[u8; 32],
    id: &str,
    input: AiProviderUpdate,
) -> AppResult<AiProvider> {
    if let Some(ref name) = input.name {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest("name 不能为空".into()));
        }
    }

    let api_key_enc = match input.api_key.as_deref() {
        Some(v) => Some(enc_str(key, v)?),
        None => None,
    };

    sqlx::query(
        r#"UPDATE ai_providers SET
             name = COALESCE(?, name),
             api_type = COALESCE(?, api_type),
             base_url = COALESCE(?, base_url),
             api_key = COALESCE(?, api_key),
             model_ids = COALESCE(?, model_ids),
             updated_at = datetime('now')
           WHERE id = ? AND is_del = 0"#,
    )
    .bind(input.name.as_deref().map(str::trim))
    .bind(input.api_type.as_deref())
    .bind(input.base_url.as_deref())
    .bind(api_key_enc.as_deref())
    .bind(input.model_ids.as_deref())
    .bind(id)
    .execute(pool)
    .await?;

    let provider = get_by_id(pool, key, id).await?;

    // 端点或模型列表变更影响当前激活引擎时，重新物化 .env
    if is_active_engine_provider(pool, id).await? {
        materialize_env(pool, key).await?;
    }

    Ok(provider)
}

pub async fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
    let refs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_engines WHERE provider_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if refs > 0 {
        return Err(AppError::BadRequest(
            "供应商仍被引擎引用，请先在引擎设置中解除关联".into(),
        ));
    }

    let result =
        sqlx::query("UPDATE ai_providers SET is_del = 1, updated_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("AI provider not found".into()));
    }
    Ok(())
}

async fn insert_provider<'e, E>(
    executor: E,
    key: &[u8; 32],
    name: &str,
    api_type: &str,
    base_url: &str,
    api_key_plain: &str,
    model_ids: &str,
) -> AppResult<String>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let id = Uuid::new_v4().to_string();
    let api_key_enc = enc_str(key, api_key_plain)?;
    sqlx::query(
        r#"INSERT INTO ai_providers (id, name, api_type, base_url, api_key, model_ids)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(name)
    .bind(api_type)
    .bind(base_url)
    .bind(&api_key_enc)
    .bind(model_ids)
    .execute(executor)
    .await?;
    Ok(id)
}

// ────────────────────────── 引擎配置 ──────────────────────────

fn validate_engine(engine: &str) -> AppResult<()> {
    if engine == ENGINE_CLAUDE || engine == ENGINE_PI {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!("未知引擎: {engine}")))
    }
}

async fn ensure_engine_rows(pool: &DbPool) -> AppResult<()> {
    for engine in [ENGINE_CLAUDE, ENGINE_PI] {
        sqlx::query("INSERT OR IGNORE INTO ai_engines (engine) VALUES (?)")
            .bind(engine)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn active_engine(pool: &DbPool) -> AppResult<String> {
    let v: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
            .bind(SETTING_ACTIVE_ENGINE)
            .fetch_optional(pool)
            .await?;
    Ok(v.unwrap_or_else(|| ENGINE_CLAUDE.to_string()))
}

/// 引擎配置总览：当前激活引擎 + 全部引擎行
pub async fn list_engines(pool: &DbPool, key: &[u8; 32]) -> AppResult<AiEnginesState> {
    migrate_from_env_if_needed(pool, key).await?;

    let engines: Vec<AiEngine> = sqlx::query_as("SELECT * FROM ai_engines ORDER BY engine")
        .fetch_all(pool)
        .await?;
    let active = active_engine(pool).await?;
    Ok(AiEnginesState {
        active_engine: active,
        engines,
    })
}

/// 更新单个引擎的关联供应商 / 激活模型 / thinking level
pub async fn update_engine(
    pool: &DbPool,
    key: &[u8; 32],
    engine: &str,
    input: AiEngineUpdate,
) -> AppResult<AiEngine> {
    validate_engine(engine)?;
    ensure_engine_rows(pool).await?;

    let current: AiEngine = sqlx::query_as("SELECT * FROM ai_engines WHERE engine = ?")
        .bind(engine)
        .fetch_one(pool)
        .await?;

    // provider_id：Some("") 视为解除关联
    let provider_id: Option<String> = match input.provider_id.as_deref() {
        Some(pid) if !pid.trim().is_empty() => {
            let exists: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ai_providers WHERE id = ? AND is_del = 0")
                    .bind(pid)
                    .fetch_one(pool)
                    .await?;
            if exists == 0 {
                return Err(AppError::BadRequest("供应商不存在".into()));
            }
            Some(pid.to_string())
        }
        Some(_) => None,
        None => current.provider_id.clone(),
    };

    // 激活模型：显式指定优先；否则在（可能变更后的）供应商模型列表中解析
    let model_ids = match provider_id.as_deref() {
        Some(pid) => {
            let v: Option<String> =
                sqlx::query_scalar("SELECT model_ids FROM ai_providers WHERE id = ? AND is_del = 0")
                    .bind(pid)
                    .fetch_optional(pool)
                    .await?;
            v.unwrap_or_default()
        }
        None => String::new(),
    };
    let provider_changed = provider_id != current.provider_id;
    let active_model_id = match input.active_model_id.as_deref() {
        Some(m) => m.trim().to_string(),
        None if provider_changed => resolve_model_id(&model_ids, ""),
        None => resolve_model_id(&model_ids, &current.active_model_id),
    };

    let thinking_level = input
        .thinking_level
        .as_deref()
        .unwrap_or(&current.thinking_level)
        .to_string();

    sqlx::query(
        r#"UPDATE ai_engines SET
             provider_id = ?,
             active_model_id = ?,
             thinking_level = ?,
             updated_at = datetime('now')
           WHERE engine = ?"#,
    )
    .bind(provider_id.as_deref())
    .bind(&active_model_id)
    .bind(&thinking_level)
    .bind(engine)
    .execute(pool)
    .await?;

    // 更新的是当前激活引擎 → 同步物化 .env
    if active_engine(pool).await? == engine {
        materialize_env(pool, key).await?;
    }

    let updated: AiEngine = sqlx::query_as("SELECT * FROM ai_engines WHERE engine = ?")
        .bind(engine)
        .fetch_one(pool)
        .await?;
    Ok(updated)
}

/// 切换当前激活引擎：写 app_settings 并把该引擎配置物化进 .env
pub async fn activate_engine(
    pool: &DbPool,
    key: &[u8; 32],
    engine: &str,
) -> AppResult<AiEnginesState> {
    validate_engine(engine)?;
    ensure_engine_rows(pool).await?;

    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(SETTING_ACTIVE_ENGINE)
    .bind(engine)
    .execute(pool)
    .await?;

    materialize_env(pool, key).await?;
    list_engines(pool, key).await
}

/// 该供应商是否被当前激活引擎引用
async fn is_active_engine_provider(pool: &DbPool, provider_id: &str) -> AppResult<bool> {
    let engine = active_engine(pool).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_engines WHERE engine = ? AND provider_id = ?")
            .bind(&engine)
            .bind(provider_id)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// 把当前激活引擎的配置物化进 ~/.ashell/ai/.env（sidecar spawn 时读取）。
/// 非激活引擎对应的字段写空串，由 write_env_config 移除对应行。
async fn materialize_env(pool: &DbPool, key: &[u8; 32]) -> AppResult<()> {
    let engine = active_engine(pool).await?;
    let row: Option<AiEngine> = sqlx::query_as("SELECT * FROM ai_engines WHERE engine = ?")
        .bind(&engine)
        .fetch_optional(pool)
        .await?;

    let mut cfg = AiModelConfig {
        sidecar_type: engine.clone(),
        ..Default::default()
    };

    if let Some(row) = row {
        if let Some(pid) = row.provider_id.as_deref() {
            if let Ok(provider) = get_by_id(pool, key, pid).await {
                if engine == ENGINE_PI {
                    cfg.pi_provider = derive_pi_provider(&provider.name);
                    cfg.pi_base_url = provider.base_url;
                    cfg.pi_api_key = provider.api_key;
                    cfg.pi_api = provider.api_type;
                    cfg.pi_model = row.active_model_id.clone();
                    cfg.pi_model_ids = provider.model_ids;
                    cfg.pi_thinking_level = row.thinking_level.clone();
                } else {
                    cfg.url = provider.base_url;
                    cfg.key = provider.api_key;
                    cfg.model_ids = provider.model_ids;
                    cfg.active_model_id = row.active_model_id.clone();
                }
            }
        }
    }

    write_env_config(cfg).map_err(AppError::Internal)?;
    Ok(())
}

// ────────────────────────── .env 迁移 ──────────────────────────

/// 幂等迁移：保证引擎行与激活引擎设置存在；
/// 供应商表为空且 .env 有配置时，按 claude / pi 两套字段各建一个供应商并接入对应引擎。
///
/// 建供应商段放在事务里并二次检查行数，避免 list / list_engines 并发触发时重复写入。
async fn migrate_from_env_if_needed(pool: &DbPool, key: &[u8; 32]) -> AppResult<()> {
    ensure_engine_rows(pool).await?;

    let env = read_env_config().unwrap_or_default();

    // 激活引擎：仅在未设置时从 .env SIDECAR_TYPE 推断一次
    let has_setting: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
            .bind(SETTING_ACTIVE_ENGINE)
            .fetch_optional(pool)
            .await?;
    if has_setting.is_none() {
        let engine = if env.sidecar_type == ENGINE_PI {
            ENGINE_PI
        } else {
            ENGINE_CLAUDE
        };
        sqlx::query("INSERT INTO app_settings (key, value) VALUES (?, ?)")
            .bind(SETTING_ACTIVE_ENGINE)
            .bind(engine)
            .execute(pool)
            .await?;
    }

    let claude_has = !env.url.is_empty() || !env.key.is_empty() || !env.model_ids.is_empty();
    let pi_has =
        !env.pi_base_url.is_empty() || !env.pi_api_key.is_empty() || !env.pi_model_ids.is_empty();
    if !claude_has && !pi_has {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_providers WHERE is_del = 0")
        .fetch_one(&mut *tx)
        .await?;
    if count > 0 {
        // 已有供应商，无需迁移；事务自动回滚
        return Ok(());
    }

    if claude_has {
        let id = insert_provider(
            &mut *tx,
            key,
            "Default",
            "anthropic-messages",
            &env.url,
            &env.key,
            &env.model_ids,
        )
        .await?;
        sqlx::query("UPDATE ai_engines SET provider_id = ?, active_model_id = ?, updated_at = datetime('now') WHERE engine = ?")
            .bind(&id)
            .bind(resolve_model_id(&env.model_ids, &env.active_model_id))
            .bind(ENGINE_CLAUDE)
            .execute(&mut *tx)
            .await?;
    }

    if pi_has {
        // 用 .env 里的 PI_PROVIDER 值做供应商名，使派生出的 PI_PROVIDER 注册表 key 原样回环
        let name = if env.pi_provider.is_empty() {
            "Default (Pi)"
        } else {
            env.pi_provider.as_str()
        };
        let api_type = if env.pi_api.is_empty() {
            "openai-completions"
        } else {
            env.pi_api.as_str()
        };
        let thinking = if env.pi_thinking_level.is_empty() {
            "off"
        } else {
            env.pi_thinking_level.as_str()
        };
        let id = insert_provider(
            &mut *tx,
            key,
            name,
            api_type,
            &env.pi_base_url,
            &env.pi_api_key,
            &env.pi_model_ids,
        )
        .await?;
        sqlx::query("UPDATE ai_engines SET provider_id = ?, active_model_id = ?, thinking_level = ?, updated_at = datetime('now') WHERE engine = ?")
            .bind(&id)
            .bind(resolve_model_id(&env.pi_model_ids, &env.pi_model))
            .bind(thinking)
            .bind(ENGINE_PI)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}
