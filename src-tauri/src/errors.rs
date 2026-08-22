use thiserror::Error;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub type AppResult<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ssh error: {0}")]
    Ssh(String),

    /// SSH 认证失败（携带主机 id，供终端 WS 发起交互式重新输入密码）
    #[error("ssh authentication failed (host {host_id})")]
    AuthFailed { host_id: i64 },

    #[error("{0}")]
    Sftp(String),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::AuthFailed { .. } => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Sftp(msg) => {
                let lower = msg.to_ascii_lowercase();
                if lower.contains("permission denied") || lower.contains("permissiondenied") {
                    StatusCode::FORBIDDEN
                } else if lower.contains("no such file") || lower.contains("not found") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({
            "code": status.as_u16(),
            "message": self.to_string(),
        }));
        (status, body).into_response()
    }
}
