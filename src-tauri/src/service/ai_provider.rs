use uuid::Uuid;

use crate::ai_env::{read_env_config, write_env_config, AiModelConfig};
use crate::config::crypto;
use crate::errors::{AppError, AppResult};
use crate::models::{AiProvider, AiProviderCreate, AiProviderUpdate, DbPool};

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

/// 解密 provider 中的密钥字段（读取后调用）
fn decrypt_provider_keys(key: &[u8; 32], p: &mut AiProvider) -> AppResult<()> {
    p.api_key = dec_str(key, &p.api_key)?;
    p.pi_api_key = dec_str(key, &p.pi_api_key)?;
    Ok(())
}

/// 列出所有未删除的供应商。若表为空且 .env 有配置，自动迁移。
pub async fn list(pool: &DbPool, key: &[u8; 32]) -> AppResult<Vec<AiProvider>> {
    let mut providers: Vec<AiProvider> =
        sqlx::query_as("SELECT * FROM ai_providers WHERE is_del = 0 ORDER BY sort_order, created_at")
            .fetch_all(pool)
            .await?;

    if providers.is_empty() {
        if let Some(provider) = try_migrate_from_env(pool, key).await? {
            providers.push(provider);
        }
    }

    for p in &mut providers {
        decrypt_provider_keys(key, p)?;
    }
    Ok(providers)
}

/// 从 .env 配置迁移为默认供应商（表为空时调用）
async fn try_migrate_from_env(pool: &DbPool, key: &[u8; 32]) -> AppResult<Option<AiProvider>> {
    let env = read_env_config().unwrap_or_default();
    let has_config = !env.url.is_empty()
        || !env.key.is_empty()
        || !env.active_model_id.is_empty()
        || !env.pi_base_url.is_empty()
        || !env.pi_api_key.is_empty();
    if !has_config {
        return Ok(None);
    }

    let id = Uuid::new_v4().to_string();
    let sidecar_type = if env.sidecar_type.is_empty() {
        "claude"
    } else {
        env.sidecar_type.as_str()
    };
    let api_key_enc = enc_str(key, &env.key)?;
    let pi_api_key_enc = enc_str(key, &env.pi_api_key)?;

    sqlx::query(
        r#"INSERT INTO ai_providers
           (id, name, sidecar_type, sort_order, is_active, url, api_key, model_ids, active_model_id,
            pi_provider, pi_model, pi_model_ids, pi_base_url, pi_api_key, pi_api, pi_thinking_level)
           VALUES (?, 'Default', ?, 0, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(sidecar_type)
    .bind(&env.url)
    .bind(&api_key_enc)
    .bind(&env.model_ids)
    .bind(&env.active_model_id)
    .bind(&env.pi_provider)
    .bind(&env.pi_model)
    .bind(&env.pi_model_ids)
    .bind(&env.pi_base_url)
    .bind(&pi_api_key_enc)
    .bind(&env.pi_api)
    .bind(&env.pi_thinking_level)
    .execute(pool)
    .await?;

    let mut provider: AiProvider =
        sqlx::query_as("SELECT * FROM ai_providers WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await?;
    decrypt_provider_keys(key, &mut provider)?;
    Ok(Some(provider))
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
    decrypt_provider_keys(key, &mut provider)?;
    Ok(provider)
}

pub async fn create(pool: &DbPool, key: &[u8; 32], input: AiProviderCreate) -> AppResult<AiProvider> {
    if input.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".into()));
    }

    let id = Uuid::new_v4().to_string();
    let sidecar_type = input.sidecar_type.as_deref().unwrap_or("claude");
    let api_key_enc = enc_str(key, input.api_key.as_deref().unwrap_or(""))?;
    let pi_api_key_enc = enc_str(key, input.pi_api_key.as_deref().unwrap_or(""))?;

    sqlx::query(
        r#"INSERT INTO ai_providers
           (id, name, sidecar_type, sort_order, is_active, url, api_key, model_ids, active_model_id,
            pi_provider, pi_model, pi_model_ids, pi_base_url, pi_api_key, pi_api, pi_thinking_level)
           VALUES (?, ?, ?, 0, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(input.name.trim())
    .bind(sidecar_type)
    .bind(input.url.as_deref().unwrap_or(""))
    .bind(&api_key_enc)
    .bind(input.model_ids.as_deref().unwrap_or(""))
    .bind(input.active_model_id.as_deref().unwrap_or(""))
    .bind(input.pi_provider.as_deref().unwrap_or(""))
    .bind(input.pi_model.as_deref().unwrap_or(""))
    .bind(input.pi_model_ids.as_deref().unwrap_or(""))
    .bind(input.pi_base_url.as_deref().unwrap_or(""))
    .bind(&pi_api_key_enc)
    .bind(input.pi_api.as_deref().unwrap_or(""))
    .bind(input.pi_thinking_level.as_deref().unwrap_or(""))
    .execute(pool)
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
    let pi_api_key_enc = match input.pi_api_key.as_deref() {
        Some(v) => Some(enc_str(key, v)?),
        None => None,
    };

    sqlx::query(
        r#"UPDATE ai_providers SET
             name = COALESCE(?, name),
             sidecar_type = COALESCE(?, sidecar_type),
             url = COALESCE(?, url),
             api_key = COALESCE(?, api_key),
             model_ids = COALESCE(?, model_ids),
             active_model_id = COALESCE(?, active_model_id),
             pi_provider = COALESCE(?, pi_provider),
             pi_model = COALESCE(?, pi_model),
             pi_model_ids = COALESCE(?, pi_model_ids),
             pi_base_url = COALESCE(?, pi_base_url),
             pi_api_key = COALESCE(?, pi_api_key),
             pi_api = COALESCE(?, pi_api),
             pi_thinking_level = COALESCE(?, pi_thinking_level),
             updated_at = datetime('now')
           WHERE id = ? AND is_del = 0"#,
    )
    .bind(input.name.as_deref().map(str::trim))
    .bind(input.sidecar_type.as_deref())
    .bind(input.url.as_deref())
    .bind(api_key_enc.as_deref())
    .bind(input.model_ids.as_deref())
    .bind(input.active_model_id.as_deref())
    .bind(input.pi_provider.as_deref())
    .bind(input.pi_model.as_deref())
    .bind(input.pi_model_ids.as_deref())
    .bind(input.pi_base_url.as_deref())
    .bind(pi_api_key_enc.as_deref())
    .bind(input.pi_api.as_deref())
    .bind(input.pi_thinking_level.as_deref())
    .bind(id)
    .execute(pool)
    .await?;

    let provider = get_by_id(pool, key, id).await?;

    if provider.is_active {
        sync_env_from_provider(&provider)?;
    }

    Ok(provider)
}

pub async fn delete(pool: &DbPool, id: &str) -> AppResult<()> {
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

/// 激活指定供应商：取消其它供应商的 active 标记，将该供应商配置写入 .env
pub async fn activate(pool: &DbPool, key: &[u8; 32], id: &str) -> AppResult<AiProvider> {
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE ai_providers SET is_active = 0")
        .execute(&mut *tx)
        .await?;

    let result =
        sqlx::query("UPDATE ai_providers SET is_active = 1, updated_at = datetime('now') WHERE id = ? AND is_del = 0")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("AI provider not found".into()));
    }

    tx.commit().await?;

    let provider = get_by_id(pool, key, id).await?;
    sync_env_from_provider(&provider)?;
    Ok(provider)
}

/// 将供应商配置写入 .env（sidecar 启动时读取）
fn sync_env_from_provider(p: &AiProvider) -> AppResult<()> {
    let config = AiModelConfig {
        url: p.url.clone(),
        key: p.api_key.clone(),
        model_ids: p.model_ids.clone(),
        active_model_id: p.active_model_id.clone(),
        sidecar_type: p.sidecar_type.clone(),
        pi_provider: p.pi_provider.clone(),
        pi_model: p.pi_model.clone(),
        pi_model_ids: p.pi_model_ids.clone(),
        pi_base_url: p.pi_base_url.clone(),
        pi_api_key: p.pi_api_key.clone(),
        pi_api: p.pi_api.clone(),
        pi_thinking_level: p.pi_thinking_level.clone(),
    };
    write_env_config(config).map_err(|e| AppError::Internal(e))?;
    Ok(())
}
