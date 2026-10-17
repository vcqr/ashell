use axum::extract::{Query, State};
use axum::Json;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::service::backup::{self, BackupConfig, BackupItem};
use crate::service::sftp_backup::SftpConfig;
use crate::service::webdav::WebDavConfig;
use crate::service::AppState;

use serde::Deserialize;

pub async fn get_config(State(_s): State<AppState>) -> AppResult<Json<ApiResponse<BackupConfig>>> {
    let cfg = backup::load_config()?;
    Ok(ApiResponse::ok(cfg))
}

#[derive(Deserialize)]
pub struct SaveConfigInput {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default)]
    pub path_prefix: String,
    #[serde(default)]
    pub webdav: Option<WebDavConfig>,
    #[serde(default)]
    pub sftp: Option<SftpConfig>,
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
        webdav: input.webdav,
        sftp: input.sftp,
    };
    backup::save_config(&cfg)?;
    Ok(crate::handlers::ok_msg("saved"))
}

#[derive(Deserialize)]
pub struct TestInput {
    /// 远端存储类型："s3"（默认）| "webdav"
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default)]
    pub path_prefix: String,
    #[serde(default)]
    pub webdav: Option<WebDavConfig>,
    #[serde(default)]
    pub sftp: Option<SftpConfig>,
}

pub async fn test_connection(
    State(s): State<AppState>,
    Json(input): Json<TestInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = BackupConfig {
        endpoint: input.endpoint,
        bucket: input.bucket,
        region: input.region,
        access_key: input.access_key,
        secret_key: input.secret_key,
        path_prefix: input.path_prefix,
        webdav: input.webdav,
        sftp: input.sftp,
    };
    backup::test_connection(
        &s.db,
        &s.config.crypto_key,
        &cfg,
        input.backend.as_deref().unwrap_or("s3"),
    )
    .await?;
    Ok(crate::handlers::ok_msg("ok"))
}

#[derive(Deserialize)]
pub struct CreateBackupInput {
    pub command_history: Vec<String>,
    /// 前端偏好快照（键位/终端设置/主题等 localStorage 白名单键），后端透传存储
    #[serde(default)]
    pub frontend_prefs: Option<serde_json::Value>,
    pub password: String,
    /// 远端存储类型："s3"（默认）| "webdav"
    #[serde(default)]
    pub backend: Option<String>,
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
        input.backend.as_deref().unwrap_or("s3"),
        input.command_history,
        input.frontend_prefs,
        input.password,
    )
    .await?;    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "key": key })),
    }))
}

pub async fn export_backup(
    State(s): State<AppState>,
    Json(input): Json<CreateBackupInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let content = backup::export_backup(
        &s.db,
        &s.config.crypto_key,
        input.command_history,
        input.frontend_prefs,
        input.password,
    )
    .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "content": content })),
    }))
}

#[derive(Deserialize)]
pub struct ListBackupsQuery {
    /// 远端存储类型："s3"（默认）| "webdav"
    #[serde(default)]
    pub backend: Option<String>,
}

pub async fn list_backups(
    State(s): State<AppState>,
    Query(q): Query<ListBackupsQuery>,
) -> AppResult<Json<ApiResponse<Vec<BackupItem>>>> {
    let cfg = backup::load_config()?;
    let items = backup::list_backups(
        &s.db,
        &s.config.crypto_key,
        &cfg,
        q.backend.as_deref().unwrap_or("s3"),
    )
    .await?;
    Ok(ApiResponse::ok(items))
}

#[derive(Deserialize)]
pub struct RestoreInput {
    pub key: String,
    pub password: String,
    /// 远端存储类型："s3"（默认）| "webdav"
    #[serde(default)]
    pub backend: Option<String>,
}

pub async fn restore_backup(
    State(s): State<AppState>,
    Json(input): Json<RestoreInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = backup::load_config()?;
    let restored = backup::restore_backup(
        &s.db,
        &s.config.crypto_key,
        &cfg,
        input.backend.as_deref().unwrap_or("s3"),
        &input.key,
        &input.password,
    )
    .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({
            "command_history": restored.command_history,
            "frontend_prefs": restored.frontend_prefs,
        })),
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
    let restored =
        backup::import_backup(&s.db, &s.config.crypto_key, &input.content, &input.password)
            .await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({
            "command_history": restored.command_history,
            "frontend_prefs": restored.frontend_prefs,
        })),
    }))
}

#[derive(Deserialize)]
pub struct DeleteInput {
    pub key: String,
    /// 远端存储类型："s3"（默认）| "webdav"
    #[serde(default)]
    pub backend: Option<String>,
}

pub async fn delete_backup(
    State(s): State<AppState>,
    Json(input): Json<DeleteInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = backup::load_config()?;
    backup::delete_backup(
        &s.db,
        &s.config.crypto_key,
        &cfg,
        input.backend.as_deref().unwrap_or("s3"),
        &input.key,
    )
    .await?;
    Ok(crate::handlers::ok_msg("deleted"))
}
