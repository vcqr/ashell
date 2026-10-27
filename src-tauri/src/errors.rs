use thiserror::Error;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub type AppResult<T> = std::result::Result<T, AppError>;

/// keyboard-interactive 单条提示（RFC 4256，2FA/TOTP 等交互式认证）
#[derive(Debug, Clone, serde::Serialize)]
pub struct KbdPrompt {
    /// 提示文本（如 "Please enter the verification code(TOTP):"）
    pub prompt: String,
    /// 是否回显用户输入（验证码通常回显，密码不回显）
    pub echo: bool,
    /// 是否已被自动应答（密码式提示 + 已存密码）；前端弹窗只收集 false 项
    pub answered: bool,
}

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

    /// SSH 私钥需要 passphrase（未设置口令或口令错误导致密钥解码失败）。
    /// 终端 WS 据此发起交互式 passphrase 输入；REST 调用方收到 401
    #[error("ssh private key passphrase required (host {host_id})")]
    KeyPassphraseRequired { host_id: i64 },

    /// SSH keyboard-interactive 认证需要人工应答（2FA/TOTP，或仅开放该方法的
    /// 密码式服务器）。终端 WS 据此弹窗收集逐条应答后带答案重连重放；
    /// REST 调用方收到 401 + 结构化字段
    #[error("ssh keyboard-interactive auth required (host {host_id})")]
    KeyboardInteractiveRequired {
        host_id: i64,
        /// 设备/方法名（如 pam_google_authenticator）
        name: String,
        /// 服务器附带的说明文字
        instructions: String,
        /// 本次连接遇到过的全部提示（按出现顺序；answered=true 的项已自动应答）
        prompts: Vec<KbdPrompt>,
        /// 自动应答的答案（与 prompts 中 answered=true 的项按序对应）。
        /// 重连时调用方把它拼在收集到的应答之前，还原完整的位置应答序列
        given: Vec<String>,
    },

    /// SSH 主机密钥待确认（首次连接或指纹变更）。终端 WS 走交互式确认；
    /// REST 调用方收到 409 + 结构化字段，做"确认 → 信任 → 重试"两段式流程
    #[error("host key verification required for {addr}:{port}")]
    HostKeyVerify {
        host_id: i64,
        addr: String,
        port: u16,
        key_type: String,
        fingerprint: String,
        /// 库中已存指纹（指纹变更场景）；首次连接为 None
        previous: Option<String>,
    },

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
            AppError::KeyPassphraseRequired { .. } => StatusCode::UNAUTHORIZED,
            AppError::KeyboardInteractiveRequired { .. } => StatusCode::UNAUTHORIZED,
            AppError::HostKeyVerify { .. } => StatusCode::CONFLICT,
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
        // HostKeyVerify 附带完整指纹信息，REST 调用方（如 SFTP 独立建联）
        // 据此弹确认框，信任后重试原请求
        let body = match &self {
            AppError::HostKeyVerify {
                host_id,
                addr,
                port,
                key_type,
                fingerprint,
                previous,
            } => Json(json!({
                "code": status.as_u16(),
                "kind": "hostkey_verify",
                "message": self.to_string(),
                "hostId": host_id,
                "addr": addr,
                "port": port,
                "keyType": key_type,
                "fingerprint": fingerprint,
                "previous": previous,
            })),
            // KeyboardInteractiveRequired 附带提示列表，REST 调用方可据此做
            // 交互式收集；given 含已存密码明文，不对外下发
            AppError::KeyboardInteractiveRequired {
                host_id,
                name,
                instructions,
                prompts,
                ..
            } => Json(json!({
                "code": status.as_u16(),
                "kind": "keyboard_interactive",
                "message": self.to_string(),
                "hostId": host_id,
                "name": name,
                "instructions": instructions,
                "prompts": prompts,
            })),
            _ => Json(json!({
                "code": status.as_u16(),
                "message": self.to_string(),
            })),
        };
        (status, body).into_response()
    }
}
