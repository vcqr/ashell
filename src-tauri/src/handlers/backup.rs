use axum::extract::State;
use axum::Json;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::service::backup::{self, BackupConfig, BackupItem};
use crate::service::AppState;

use serde::Deserialize;

pub async fn get_config(State(_s): State<AppState>) -> AppResult<Json<ApiResponse<BackupConfig>>> {
    let cfg = backup::load_config()?;
    Ok(ApiResponse::ok(cfg))
}

#[derive(Deserialize)]
pub struct SaveConfigInput {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_prefix: String,
}

pub async fn save_config(
    _state: State<AppState>,
    Json(input): Json<SaveConfigInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = BackupConfig {
        endpoint: input.endpoint,
        bucket: input.bucket,
        region: input.region,
        access_key: input.access_key,
        secret_key: input.secret_key,
        path_prefix: input.path_prefix,
    };
    backup::save_config(&cfg)?;
    Ok(crate::handlers::ok_msg("saved"))
}

#[derive(Deserialize)]
pub struct TestInput {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub path_prefix: String,
}

pub async fn test_connection(
    _state: State<AppState>,
    Json(input): Json<TestInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = BackupConfig {
        endpoint: input.endpoint,
        bucket: input.bucket,
        region: input.region,
        access_key: input.access_key,
        secret_key: input.secret_key,
        path_prefix: input.path_prefix,
    };
    backup::test_connection(&cfg).await?;
    Ok(crate::handlers::ok_msg("ok"))
}

#[derive(Deserialize)]
pub struct CreateBackupInput {
    pub command_history: Vec<String>,
    pub password: String,
}

pub async fn create_backup(
    State(s): State<AppState>,
    Json(input): Json<CreateBackupInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = backup::load_config()?;
    let key = backup::create_backup(
        &s.db,
        &s.config.crypto_key,
        &cfg,
        input.command_history,
        input.password,
    )
    .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "key": key })),
    }))
}

pub async fn export_backup(
    State(s): State<AppState>,
    Json(input): Json<CreateBackupInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let content =
        backup::export_backup(&s.db, &s.config.crypto_key, input.command_history, input.password)
            .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "content": content })),
    }))
}

pub async fn list_backups(
    _state: State<AppState>,
) -> AppResult<Json<ApiResponse<Vec<BackupItem>>>> {
    let cfg = backup::load_config()?;
    let items = backup::list_backups(&cfg).await?;
    Ok(ApiResponse::ok(items))
}

#[derive(Deserialize)]
pub struct RestoreInput {
    pub key: String,
    pub password: String,
}

pub async fn restore_backup(
    State(s): State<AppState>,
    Json(input): Json<RestoreInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = backup::load_config()?;
    let command_history =
        backup::restore_backup(&s.db, &s.config.crypto_key, &cfg, &input.key, &input.password)
            .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "command_history": command_history })),
    }))
}

#[derive(Deserialize)]
pub struct ImportInput {
    pub content: String,
    pub password: String,
}

pub async fn import_backup(
    State(s): State<AppState>,
    Json(input): Json<ImportInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let command_history =
        backup::import_backup(&s.db, &s.config.crypto_key, &input.content, &input.password)
            .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "command_history": command_history })),
    }))
}

#[derive(Deserialize)]
pub struct DeleteInput {
    pub key: String,
}

pub async fn delete_backup(
    _state: State<AppState>,
    Json(input): Json<DeleteInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = backup::load_config()?;
    backup::delete_backup(&cfg, &input.key).await?;
    Ok(crate::handlers::ok_msg("deleted"))
}
