//! `GET /api/serial/terminal/{host_id}?sid=&token=`
//!
//! 与 `handlers::terminal` 共用前端协议，但走本地串口设备。

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::AppError;
use crate::service::{self, serial::SerialConfig, AppState};

#[derive(Debug, Deserialize)]
pub struct SerialTerminalQuery {
    pub sid: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(host_id): Path<i64>,
    Query(q): Query<SerialTerminalQuery>,
) -> Result<impl IntoResponse, AppError> {
    let host = service::host::get_by_id(&state.db, host_id).await?;

    if host.protocol != "serial" {
        return Err(AppError::BadRequest(format!(
            "host {host_id} protocol is '{}', expected 'serial'",
            host.protocol
        )));
    }

    let sid = q.sid.unwrap_or_else(|| Uuid::new_v4().to_string());

    let config = SerialConfig {
        path: host.addr.clone(),
        baud_rate: host.baud_rate.unwrap_or(9600) as u32,
        data_bits: host.data_bits.unwrap_or(8) as u8,
        stop_bits: host.stop_bits.unwrap_or(1) as u8,
        parity: host.parity.clone().unwrap_or_else(|| "none".into()),
        flow_control: host.flow_control.clone().unwrap_or_else(|| "none".into()),
    };

    Ok(ws.on_upgrade(move |socket| service::serial::handle(socket, sid, config)))
}
