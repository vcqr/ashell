//! 已信任 SSH 主机指纹（TOFU）管理接口

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::errors::{AppError, AppResult};
use crate::handlers::{ok_msg, ApiResponse};
use crate::models::KnownHost;
use crate::service::known_host as svc;
use crate::service::AppState;

/// `GET /api/known-hosts`：全部已信任指纹（设置页管理列表）
pub async fn list(State(state): State<AppState>) -> AppResult<Json<ApiResponse<Vec<KnownHost>>>> {
    Ok(ApiResponse::ok(svc::list(&state.db).await?))
}

#[derive(Debug, Deserialize)]
pub struct TrustReq {
    pub addr: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
}

/// `POST /api/known-hosts`：信任（或变更后更新）一条指纹。
/// 供 REST 建联路径（如 SFTP 独立会话）收到 409 hostkey_verify 后两段式确认。
pub async fn trust(
    State(state): State<AppState>,
    Json(req): Json<TrustReq>,
) -> AppResult<Json<ApiResponse<Value>>> {
    let addr = req.addr.trim();
    let key_type = req.key_type.trim();
    let fingerprint = req.fingerprint.trim();
    if addr.is_empty() || key_type.is_empty() || fingerprint.is_empty() {
        return Err(AppError::BadRequest(
            "addr / key_type / fingerprint required".into(),
        ));
    }
    // 只接受 OpenSSH 风格 SHA256 指纹，防止调用方误存任意字符串导致校验失效
    if !fingerprint.starts_with("SHA256:") {
        return Err(AppError::BadRequest(
            "fingerprint must be in SHA256:... form".into(),
        ));
    }
    svc::trust(&state.db, addr, req.port, key_type, fingerprint).await?;
    Ok(ok_msg("ok"))
}

/// `DELETE /api/known-hosts/{id}`：删除信任记录（下次连接重新确认）
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<Value>>> {
    svc::remove(&state.db, id).await?;
    Ok(ok_msg("ok"))
}
