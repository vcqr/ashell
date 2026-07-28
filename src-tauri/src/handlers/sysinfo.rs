//! 主机系统信息 REST 端点
//!
//! GET /api/ssh/sysinfo?sid=<sid>
//!
//! 复用已建立的 SSH 会话（由终端 WS 或 /api/ssh/sftp/open 创建）；
//! sid 不存在时返回 404。

use axum::extract::Query;
use axum::Json;
use serde::Deserialize;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::service::sysinfo as sysinfo_svc;

#[derive(Debug, Deserialize)]
pub struct SysInfoQuery {
    pub sid: String,
}

pub async fn get(
    Query(q): Query<SysInfoQuery>,
) -> AppResult<Json<ApiResponse<sysinfo_svc::SysInfo>>> {
    let info = sysinfo_svc::collect(&q.sid).await?;
    Ok(ApiResponse::ok(info))
}
