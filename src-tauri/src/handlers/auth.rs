//! Web 版登录端点
//!
//! 单用户鉴权模型：访问令牌即登录密码。服务端以常量时间比较校验密码，
//! 成功后返回令牌本身，前端持久化到 localStorage 并以
//! `Authorization: Bearer <token>` 访问后续接口（与桌面端 get_api_info
//! 拿到的 token 同一体系，auth 中间件无需感知登录态）。

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::handlers::ApiResponse;
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
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, StatusCode> {
    let expected = state.config.token.as_str();
    let ok: bool = req.password.as_bytes().ct_eq(expected.as_bytes()).into();
    if !ok {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(ApiResponse::ok(LoginResponse {
        token: expected.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}
