pub mod forward;
pub mod group;
pub mod host;
pub mod icons;
pub mod local;
pub mod serial;
pub mod sftp;
pub mod sysinfo;
pub mod telnet;
pub mod terminal;
pub mod ai_provider;
pub mod phrase;

use axum::Json;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: u16,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Json<ApiResponse<T>> {
        Json(ApiResponse {
            code: 0,
            message: "ok".into(),
            data: Some(data),
        })
    }
}

pub fn ok_msg(msg: &str) -> Json<ApiResponse<Value>> {
    Json(ApiResponse {
        code: 0,
        message: msg.into(),
        data: None,
    })
}
