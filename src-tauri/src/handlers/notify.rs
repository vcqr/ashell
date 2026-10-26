use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::errors::{AppError, AppResult};
use crate::handlers::{ok_msg, ApiResponse};
use crate::service::notify::{self, NotifyConfig, NotifyEvent, NotifyLine};
use crate::service::AppState;

pub async fn get_config(
    State(_s): State<AppState>,
) -> AppResult<Json<ApiResponse<NotifyConfig>>> {
    Ok(ApiResponse::ok(notify::load_config()?))
}

#[derive(Deserialize)]
pub struct SaveConfigInput {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub webhook: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub custom_template: String,
}

pub async fn save_config(
    _state: State<AppState>,
    Json(input): Json<SaveConfigInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = NotifyConfig {
        enabled: input.enabled,
        platform: input.platform,
        webhook: input.webhook.trim().to_string(),
        secret: input.secret,
        custom_template: input.custom_template,
    };
    notify::save_config(&cfg)?;
    Ok(ok_msg("saved"))
}

#[derive(Deserialize)]
pub struct TestInput {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub webhook: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub custom_template: String,
}

pub async fn test_connection(
    State(_s): State<AppState>,
    Json(input): Json<TestInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let cfg = NotifyConfig {
        enabled: true,
        platform: input.platform,
        webhook: input.webhook.trim().to_string(),
        secret: input.secret,
        custom_template: input.custom_template,
    };
    let event = NotifyEvent {
        title: "通知测试".into(),
        sid: "-".into(),
        template: "green",
        lines: vec![NotifyLine::plain(
            "ashell 通知配置成功：AI 审批与提问将推送到这里",
        )],
    };
    notify::send_message(&cfg, &event)
        .await
        .map_err(AppError::Internal)?;
    Ok(ok_msg("sent"))
}
