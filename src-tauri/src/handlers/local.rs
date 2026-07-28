//! `GET /api/local/terminal?sid=&shell=&token=`
//!
//! 与 `handlers::terminal` 共用前端协议，但走本地 PTY，不需要主机配置。

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::Query;
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::service::local_pty;

#[derive(Debug, Deserialize)]
pub struct LocalTerminalQuery {
    /// 客户端可指定 sid（与远端协议保持一致），省略则自动生成
    pub sid: Option<String>,
    /// 期望的 shell：powershell/pwsh/cmd/bash/zsh/sh/fish/git-bash 或绝对路径；
    /// 省略走平台 auto 默认
    pub shell: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<LocalTerminalQuery>,
) -> impl IntoResponse {
    let sid = q.sid.unwrap_or_else(|| Uuid::new_v4().to_string());
    let shell = q.shell;
    ws.on_upgrade(move |socket| local_pty::handle(socket, sid, shell))
}
