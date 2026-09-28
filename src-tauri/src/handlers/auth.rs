//! Web 版登录端点
//!
//! 单用户鉴权模型：访问令牌即登录密码。服务端以常量时间比较校验密码，
//! 成功后返回令牌本身，前端持久化到 localStorage 并以
//! `Authorization: Bearer <token>` 访问后续接口（与桌面端 get_api_info
//! 拿到的 token 同一体系，auth 中间件无需感知登录态）。
//!
//! 防爆破：按来源 IP 限次——窗口内连续失败达到上限触发指数锁定，
//! 每次 失败 / 锁定 / 成功 均写审计日志（含来源 IP）。

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::handlers::ApiResponse;
use crate::middleware::login_guard;
use crate::service::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub version: String,
}

/// `POST /api/auth/login`（免鉴权）：校验访问令牌，成功返回令牌与服务端版本。
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let ip = peer.ip();

    // 锁定中的 IP 直接拒绝，不再消耗比较与失败计数
    if let Some(wait) = login_guard::check_locked(ip) {
        log::warn!("login blocked from {ip}: locked for another {}s", wait.as_secs());
        return Err(err_json(
            StatusCode::TOO_MANY_REQUESTS,
            &format!("尝试次数过多，已临时锁定，请 {} 秒后再试", wait.as_secs().max(1)),
        ));
    }

    let expected = state.config.token.as_str();
    let ok: bool = req.password.as_bytes().ct_eq(expected.as_bytes()).into();
    if !ok {
        match login_guard::record_failure(ip) {
            Some(lockout) => {
                log::warn!(
                    "login failed from {ip}: reached failure limit, locked for {}s",
                    lockout.as_secs()
                );
                return Err(err_json(
                    StatusCode::TOO_MANY_REQUESTS,
                    &format!("失败次数过多，已临时锁定，请 {} 秒后再试", lockout.as_secs().max(1)),
                ));
            }
            None => {
                log::warn!("login failed from {ip}");
                return Err(err_json(StatusCode::UNAUTHORIZED, "密码错误"));
            }
        }
    }

    login_guard::record_success(ip);
    log::info!("login ok from {ip}");
    Ok(ApiResponse::ok(LoginResponse {
        token: expected.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

fn err_json(status: StatusCode, message: &str) -> (StatusCode, Json<ApiResponse<()>>) {
    (
        status,
        Json(ApiResponse {
            code: status.as_u16(),
            message: message.into(),
            data: None,
        }),
    )
}
