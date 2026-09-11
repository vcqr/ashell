//! AI sidecar 进程管理的 REST + WebSocket 端点（Web 形态的 P1 AI 能力）。
//!
//! 与桌面端 Tauri commands（spawn/write/kill/has/pid + event 回推）一一对应：
//! - POST /api/ai/sidecar/spawn          启动 sidecar {ssid, workspace, token, addr, sidecarType}
//! - GET  /api/ai/sidecar/{ssid}         查询运行状态 {running, pid}
//! - POST /api/ai/sidecar/write          向进程 stdin 写数据 {ssid, data}
//! - POST /api/ai/sidecar/kill           终止进程 {ssid}
//! - GET  /api/ai/sidecar/{ssid}/stream  WebSocket 订阅 stdout/stderr 行广播
//!
//! WS 广播基于 sidecar.rs 的 per-ssid broadcast 通道，多窗口可同时附着
//! 同一 ssid（对应桌面端 Tauri emit 广播语义），互不干扰。

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::errors::{AppError, AppResult};
use crate::handlers::ApiResponse;
use crate::sidecar;
use crate::service::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnRequest {
    pub ssid: String,
    pub workspace: String,
    pub token: String,
    pub addr: String,
    pub sidecar_type: Option<String>,
}

/// `POST /api/ai/sidecar/spawn`
pub async fn spawn(
    State(_state): State<AppState>,
    Json(req): Json<SpawnRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let pid = sidecar::spawn_sidecar_web(
        req.ssid,
        req.workspace,
        req.token,
        req.addr,
        req.sidecar_type,
    )
    .map_err(AppError::BadRequest)?;
    Ok(ApiResponse::ok(serde_json::json!({ "pid": pid })))
}

#[derive(Debug, Deserialize)]
pub struct WriteRequest {
    pub ssid: String,
    pub data: String,
}

/// `POST /api/ai/sidecar/write`
pub async fn write(Json(req): Json<WriteRequest>) -> AppResult<axum::response::Response> {
    sidecar::write_to_sidecar(req.ssid, req.data).map_err(AppError::BadRequest)?;
    Ok((axum::http::StatusCode::OK, "ok").into_response())
}

#[derive(Debug, Deserialize)]
pub struct KillRequest {
    pub ssid: String,
}

/// `POST /api/ai/sidecar/kill`
pub async fn kill(Json(req): Json<KillRequest>) -> AppResult<axum::response::Response> {
    sidecar::kill_sidecar(req.ssid).map_err(AppError::BadRequest)?;
    Ok((axum::http::StatusCode::OK, "ok").into_response())
}

/// `GET /api/ai/sidecar/{ssid}`：运行状态查询
pub async fn status(
    Path(ssid): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    ApiResponse::ok(serde_json::json!({
        "running": sidecar::has_sidecar(ssid.clone()),
        "pid": sidecar::get_sidecar_pid(ssid),
    }))
}

/// `GET /api/ai/sidecar/{ssid}/stream?token=`：订阅 sidecar 输出行。
///
/// 消息格式（JSON 文本帧）：`{"stream":"stdout"|"stderr","line":"..."}`
/// 进程退出（map 中被移除）后通道关闭，连接随之结束；期间断开重连不丢
/// 已订阅语义之外的数据（broadcast 只给活跃订阅者投递，历史不回放，
/// 与桌面端 Tauri event 行为一致）。
pub async fn stream(
    ws: WebSocketUpgrade,
    Path(ssid): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_stream(socket, ssid))
}

async fn handle_stream(socket: WebSocket, ssid: String) {
    let Some(mut rx) = sidecar::subscribe_events(&ssid) else {
        return;
    };

    let (mut sender, mut receiver) = socket.split();

    // 出站：broadcast 行 → WS 文本帧
    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let Ok(text) = serde_json::to_string(&event) else {
                        continue;
                    };
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // 订阅落后：丢弃 n 行（AI 高速输出场景），通知前端可选处理
                    let text = serde_json::json!({ "stream": "lagged", "dropped": n }).to_string();
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // 入站：仅处理客户端关闭/心跳（前端不向该 WS 发业务消息）
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
