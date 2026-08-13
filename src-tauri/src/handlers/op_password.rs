use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::errors::AppResult;
use crate::handlers::{ok_msg, ApiResponse};
use crate::service::{self, AppState};

#[derive(Serialize)]
pub struct OpPasswordStatus {
    pub set: bool,
}

pub async fn status(
    State(s): State<AppState>,
) -> AppResult<Json<ApiResponse<OpPasswordStatus>>> {
    let set = service::op_password::is_set(&s.db).await?;
    Ok(ApiResponse::ok(OpPasswordStatus { set }))
}

#[derive(Deserialize)]
pub struct SetRequest {
    pub password: String,
}

pub async fn set(
    State(s): State<AppState>,
    Json(req): Json<SetRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    service::op_password::set(&s.db, &s.config.crypto_key, &req.password).await?;
    Ok(ok_msg("ok"))
}

#[derive(Deserialize)]
pub struct ChangeRequest {
    pub old_password: String,
    pub new_password: String,
}

pub async fn change(
    State(s): State<AppState>,
    Json(req): Json<ChangeRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    service::op_password::change(
        &s.db,
        &s.config.crypto_key,
        &req.old_password,
        &req.new_password,
    )
    .await?;
    Ok(ok_msg("ok"))
}

#[derive(Deserialize)]
pub struct ClearRequest {
    pub password: String,
}

pub async fn clear(
    State(s): State<AppState>,
    Json(req): Json<ClearRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    service::op_password::clear(&s.db, &s.config.crypto_key, &req.password).await?;
    Ok(ok_msg("ok"))
}
