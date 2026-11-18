//! tmux 会话管理 REST 端点
//!
//! GET  /api/ssh/tmux/sessions?sid=&host_id=   列出远端 tmux 会话
//! POST /api/ssh/tmux/kill { sid, name, host_id }  终止指定会话
//!
//! 复用已建立的 SSH 会话（由终端 WS 创建）；sid 不存在时 404。
//! host_id 用于堡垒机拒绝（exec 落在网关菜单上，见 service 层说明）。

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::service::{tmux as tmux_svc, AppState};

#[derive(Debug, Deserialize)]
pub struct SessionsQuery {
    pub sid: String,
    pub host_id: i64,
}

pub async fn sessions(
    State(state): State<AppState>,
    Query(q): Query<SessionsQuery>,
) -> AppResult<Json<ApiResponse<Vec<tmux_svc::TmuxSession>>>> {
    let list = tmux_svc::list(&q.sid, q.host_id, &state.db).await?;
    Ok(ApiResponse::ok(list))
}

#[derive(Debug, Deserialize)]
pub struct KillReq {
    pub sid: String,
    pub name: String,
    pub host_id: i64,
}

pub async fn kill(
    State(state): State<AppState>,
    Json(req): Json<KillReq>,
) -> AppResult<Json<ApiResponse<()>>> {
    tmux_svc::kill(&req.sid, &req.name, req.host_id, &state.db).await?;
    Ok(ApiResponse::ok(()))
}
