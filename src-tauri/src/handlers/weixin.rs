use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::errors::AppResult;
use crate::handlers::{ok_msg, ApiResponse};
use crate::service::weixin::{self, LoginView, StatusView};
use crate::service::AppState;

/// 登录/轮询/配对状态
pub async fn get_status(State(_s): State<AppState>) -> AppResult<Json<ApiResponse<StatusView>>> {
    Ok(ApiResponse::ok(weixin::status_view()))
}

/// 发起扫码登录，返回二维码 URL（前端渲染）
pub async fn login_start(
    State(_s): State<AppState>,
) -> AppResult<Json<ApiResponse<LoginView>>> {
    let view = weixin::login_start().await?;
    Ok(ApiResponse::ok(view))
}

/// 读取登录状态机当前状态（前端 1~2s 轮询）
pub async fn login_poll(State(_s): State<AppState>) -> AppResult<Json<ApiResponse<LoginView>>> {
    Ok(ApiResponse::ok(weixin::login_view()))
}

#[derive(Deserialize)]
pub struct VerifyCodeInput {
    pub code: String,
}

/// 提交配对数字（need_verifycode 时）
pub async fn login_verify(
    State(_s): State<AppState>,
    Json(input): Json<VerifyCodeInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    weixin::login_submit_code(&input.code);
    Ok(ok_msg("submitted"))
}

pub async fn login_cancel(
    State(_s): State<AppState>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    weixin::login_cancel();
    Ok(ok_msg("cancelled"))
}

pub async fn logout(State(_s): State<AppState>) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    weixin::logout()?;
    Ok(ok_msg("logged_out"))
}

#[derive(Deserialize)]
pub struct SaveRemoteConfigInput {
    #[serde(default)]
    pub notify_enabled: bool,
}

/// 保存「远程」配置（当前仅通知联动开关）
pub async fn save_config(
    State(_s): State<AppState>,
    Json(input): Json<SaveRemoteConfigInput>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let mut cfg = weixin::load_config()?;
    cfg.notify_enabled = input.notify_enabled;
    weixin::save_config(&cfg)?;
    Ok(ok_msg("saved"))
}

/// 测试推送：向默认目标发一条测试消息
pub async fn test_push(State(_s): State<AppState>) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let text = "【ashell】微信远程测试\n\n连接成功：AI 审批与提问将推送到这里";
    weixin::send_text(None, text)
        .await
        .map_err(crate::errors::AppError::Internal)?;
    Ok(ok_msg("sent"))
}
