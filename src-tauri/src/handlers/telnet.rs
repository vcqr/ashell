//! `GET /api/telnet/terminal/{host_id}?sid=&token=`
//!
//! 与 `handlers::terminal` 共用前端协议，但走 Telnet TCP 连接。

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::AppError;
use crate::service::{self, AppState};

#[derive(Debug, Deserialize)]
pub struct TelnetTerminalQuery {
    pub sid: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(host_id): Path<i64>,
    Query(q): Query<TelnetTerminalQuery>,
) -> Result<impl IntoResponse, AppError> {
    let host = service::host::get_by_id(&state.db, host_id).await?;

    if host.protocol != "telnet" {
        return Err(AppError::BadRequest(format!(
            "host {host_id} protocol is '{}', expected 'telnet'",
            host.protocol
        )));
    }

    let sid = q.sid.unwrap_or_else(|| Uuid::new_v4().to_string());
    let addr = host.addr.clone();
    let port: u16 = host.port.parse().unwrap_or(23);
    let username = host.username.clone();

    // Telnet 密码需要解密（如果有的话）
    let password = if let Some(ref p) = host.password {
        if p.is_empty() {
            None
        } else {
            Some(
                crate::config::crypto::decrypt(&state.config.crypto_key, p)
                    .map_err(|e| AppError::Internal(format!("decrypt password: {e}")))?,
            )
        }
    } else {
        None
    };

    Ok(ws.on_upgrade(move |socket| {
        service::telnet::handle(socket, sid, addr, port, username, password)
    }))
}
