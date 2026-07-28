use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;

use crate::service::AppState;

/// 常量时间比较，避免时序侧信道
fn ct_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Token 鉴权中间件
/// 接受方式：
///   1) HTTP header  `Authorization: Bearer <token>`
///   2) HTTP header  `X-Auth-Token: <token>`
///   3) Query param  `?token=<token>` （主要给 WebSocket 用）
pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = state.config.token.as_str();

    // 1) Authorization header
    if let Some(v) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(s) = v.to_str() {
            let s = s.trim();
            let token = s.strip_prefix("Bearer ").unwrap_or(s);
            if ct_eq(token, expected) {
                return Ok(next.run(req).await);
            }
        }
    }
    // 2) X-Auth-Token header
    if let Some(v) = req.headers().get("x-auth-token") {
        if let Ok(s) = v.to_str() {
            if ct_eq(s.trim(), expected) {
                return Ok(next.run(req).await);
            }
        }
    }
    // 3) query string
    if let Some(q) = req.uri().query() {
        for pair in q.split('&') {
            let mut it = pair.splitn(2, '=');
            if it.next() == Some("token") {
                if let Some(v) = it.next() {
                    if ct_eq(v, expected) {
                        return Ok(next.run(req).await);
                    }
                }
            }
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}
