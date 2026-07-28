//! SSH 端口转发 REST 接口
//!
//! - GET    /api/ssh/forward?sid=                列出 sid 的全部转发规则
//! - POST   /api/ssh/forward                     创建并启动一条规则
//! - DELETE /api/ssh/forward/{rule_id}?sid=      删除一条规则

use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;

use crate::errors::AppResult;
use crate::handlers::{ok_msg, ApiResponse};
use crate::service::forward::{self as fwd, ForwardCreate, ForwardRule};

#[derive(Debug, Deserialize)]
pub struct SidQuery {
    pub sid: String,
}

pub async fn list(
    Query(q): Query<SidQuery>,
) -> AppResult<Json<ApiResponse<Vec<ForwardRule>>>> {
    Ok(ApiResponse::ok(fwd::list(&q.sid).await))
}

pub async fn create(
    Json(req): Json<ForwardCreate>,
) -> AppResult<Json<ApiResponse<ForwardRule>>> {
    let rule = fwd::create(req).await?;
    Ok(ApiResponse::ok(rule))
}

pub async fn delete(
    Path(rule_id): Path<String>,
    Query(q): Query<SidQuery>,
) -> AppResult<Json<crate::handlers::ApiResponse<serde_json::Value>>> {
    fwd::remove_rule(&q.sid, &rule_id).await?;
    Ok(ok_msg("ok"))
}
