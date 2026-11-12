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
    let list = parse_model_list(model_ids);
    if list.iter().any(|s| s == want) {
        want.to_string()
    } else {
        list.first().cloned().unwrap_or_default()
    }
}

/// 解析候选模型 ID 列表：中英文逗号分隔，去空白去空项，保序去重
fn parse_model_list(value: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    value
        .split([',', '，'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.to_string()))
        .map(String::from)
        .collect()
}

/// 启用列表 = 候选全量 − 禁用子集（保序），供引擎解析与 .env 物化使用
fn enabled_model_ids(model_ids: &str, disabled_model_ids: &str) -> String {
    let disabled = parse_model_list(disabled_model_ids);
    parse_model_list(model_ids)
        .into_iter()
        .filter(|id| !disabled.contains(id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 禁用列表归一化：只保留仍存在于候选全量中的项（prune），保序去重。
/// 随 model_ids 的增删保持「禁用 ⊆ 全量」不变量。
fn prune_disabled_model_ids(model_ids: &str, disabled_model_ids: &str) -> String {
    let all = parse_model_list(model_ids);
    parse_model_list(disabled_model_ids)
        .into_iter()
        .filter(|id| all.contains(id))
        .collect::<Vec<_>>()
        .join(", ")
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

    // 读旧值后合并再写回：候选模型与禁用列表需在本侧 prune，
    // 维持「禁用 ⊆ 全量」不变量（COALESCE 写法做不到跨列约束）
    let current: AiProvider =
        sqlx::query_as("SELECT * FROM ai_providers WHERE id = ? AND is_del = 0")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => AppError::NotFound("AI provider not found".into()),
                other => AppError::Db(other),
            })?;

    let name = input.name.as_deref().map(str::trim).unwrap_or(&current.name);
    let api_type = input.api_type.as_deref().unwrap_or(&current.api_type);
    let base_url = input.base_url.as_deref().unwrap_or(&current.base_url);
    // current.api_key 仍是密文（未走 get_by_id 解密），无新值时原样写回
    let api_key_enc = match input.api_key.as_deref() {
        Some(v) => enc_str(key, v)?,
        None => current.api_key.clone(),
    };
    let model_ids = match input.model_ids.as_deref() {
        Some(v) => parse_model_list(v).join(", "),
        None => current.model_ids.clone(),
    };
    let disabled_model_ids = match input.disabled_model_ids.as_deref() {
        Some(v) => v.to_string(),
        None => current.disabled_model_ids.clone(),
    };
    let disabled_model_ids = prune_disabled_model_ids(&model_ids, &disabled_model_ids);

    sqlx::query(
        r#"UPDATE ai_providers SET
             name = ?,
             api_type = ?,
             base_url = ?,
             api_key = ?,
             model_ids = ?,
             disabled_model_ids = ?,
             updated_at = datetime('now')
           WHERE id = ? AND is_del = 0"#,
    )
    .bind(name)
    .bind(api_type)
    .bind(base_url)
    .bind(&api_key_enc)
    .bind(&model_ids)
    .bind(&disabled_model_ids)
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

pub async fn delete(pool: &DbPool, key: &[u8; 32], id: &str) -> AppResult<()> {
    // 引擎对供应商的引用自动解除（激活模型一并清空）：未绑定供应商是引导
    // 卡片可就地修复的一等状态，无需要求用户先手动解绑再删除。非激活引擎
    // 的残留引用在 UI 上不可见，曾导致"最后一个供应商永远删不掉"。
    let mut tx = pool.begin().await?;
    let unbound = sqlx::query(
        "UPDATE ai_engines SET provider_id = NULL, active_model_id = '' WHERE provider_id = ?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let result =
        sqlx::query("UPDATE ai_providers SET is_del = 1, updated_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    if result.rows_affected() == 0 {
        // 供应商不存在：不提交，解绑一并回滚
        return Err(AppError::NotFound("AI provider not found".into()));
    }
    tx.commit().await?;

    if unbound > 0 {
        // 受影响的引擎可能包含当前激活引擎，重新物化 .env
        materialize_env(pool, key).await?;
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

/// 归一化 token 数字段：显式值去空白且需为合法正整数，否则回退现值/空串
fn normalize_token_field(explicit: Option<&str>, current: &str) -> String {
    match explicit {
        Some(v) => {
            let t = v.trim();
            if t.is_empty() {
                String::new()
            } else if t.parse::<u64>().is_ok() {
                t.to_string()
            } else {
                current.to_string()
            }
        }
        None => current.to_string(),
    }
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

    // 激活模型：显式指定优先；否则在（可能变更后的）供应商启用列表中解析
    // （禁用的候选模型不可选，被禁用的激活模型自动回退到第一个启用模型）
    let model_ids = match provider_id.as_deref() {
        Some(pid) => {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT model_ids, disabled_model_ids FROM ai_providers WHERE id = ? AND is_del = 0",
            )
            .bind(pid)
            .fetch_optional(pool)
            .await?;
            row.map(|(m, d)| enabled_model_ids(&m, &d)).unwrap_or_default()
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

    // 上下文控制字段：显式指定优先，归一化数字（去空白；非法值保持空串语义）
    let context_window = normalize_token_field(
        input.context_window.as_deref(),
        &current.context_window,
    );
    let max_tokens = normalize_token_field(input.max_tokens.as_deref(), &current.max_tokens);
    let max_rounds = normalize_token_field(input.max_rounds.as_deref(), &current.max_rounds);
    // streaming 开关：'1'/'0' 二值，其余值忽略保持现状
    let streaming = match input.streaming.as_deref().map(str::trim) {
        Some("1") => "1".to_string(),
        Some("0") => "0".to_string(),
        _ => current.streaming.clone(),
    };

    sqlx::query(
        r#"UPDATE ai_engines SET
             provider_id = ?,
             active_model_id = ?,
             thinking_level = ?,
             context_window = ?,
             max_tokens = ?,
             max_rounds = ?,
             streaming = ?,
             updated_at = datetime('now')
           WHERE engine = ?"#,
    )
    .bind(provider_id.as_deref())
    .bind(&active_model_id)
    .bind(&thinking_level)
    .bind(&context_window)
    .bind(&max_tokens)
    .bind(&max_rounds)
    .bind(&streaming)
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
        // 上下文控制字段两引擎通用，激活引擎即物化
        cfg.ai_context_window = row.context_window.clone();
        cfg.ai_max_tokens = row.max_tokens.clone();
        cfg.ai_max_rounds = row.max_rounds.clone();
        cfg.ai_streaming = row.streaming.clone();
        if let Some(pid) = row.provider_id.as_deref() {
            if let Ok(provider) = get_by_id(pool, key, pid).await {
                if engine == ENGINE_PI {
                    cfg.pi_provider = derive_pi_provider(&provider.name);
                    cfg.pi_base_url = provider.base_url;
                    cfg.pi_api_key = provider.api_key;
                    cfg.pi_api = provider.api_type;
                    cfg.pi_model = row.active_model_id.clone();
                    cfg.pi_model_ids = enabled_model_ids(&provider.model_ids, &provider.disabled_model_ids);
                    cfg.pi_thinking_level = row.thinking_level.clone();
                } else {
                    cfg.url = provider.base_url;
                    cfg.key = provider.api_key;
                    cfg.model_ids = enabled_model_ids(&provider.model_ids, &provider.disabled_model_ids);
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
        // 默认引擎 pi；旧 .env 显式写了 claude 时尊重既有选择
        let engine = if env.sidecar_type == ENGINE_CLAUDE {
            ENGINE_CLAUDE
        } else {
            ENGINE_PI
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_model_list_splits_and_dedupes() {
        assert_eq!(
            parse_model_list("m-a, m-b，m-c ,m-a,,  "),
            vec!["m-a", "m-b", "m-c"]
        );
        assert_eq!(parse_model_list(""), Vec::<String>::new());
    }

    #[test]
    fn enabled_model_ids_excludes_disabled_and_keeps_order() {
        assert_eq!(
            enabled_model_ids("m-a, m-b, m-c", "m-c, m-a"),
            "m-b"
        );
        // 禁用列表里的未知项不影响全量
        assert_eq!(enabled_model_ids("m-a, m-b", "m-x"), "m-a, m-b");
        // 全部禁用 → 空
        assert_eq!(enabled_model_ids("m-a, m-b", "m-a,m-b"), "");
        // 未禁用 → 原样（归一化）
        assert_eq!(enabled_model_ids("m-a,m-b", ""), "m-a, m-b");
    }

    #[test]
    fn prune_disabled_keeps_only_existing_entries() {
        assert_eq!(prune_disabled_model_ids("m-a, m-b", "m-b, m-x"), "m-b");
        assert_eq!(prune_disabled_model_ids("m-a", "m-a, m-b"), "m-a");
        assert_eq!(prune_disabled_model_ids("", "m-a"), "");
    }

    #[test]
    fn resolve_model_id_falls_back_to_first_enabled() {
        assert_eq!(resolve_model_id("m-a, m-b", "m-b"), "m-b");
        assert_eq!(resolve_model_id("m-a, m-b", "m-x"), "m-a");
        assert_eq!(resolve_model_id("", "m-a"), "");
    }
}
