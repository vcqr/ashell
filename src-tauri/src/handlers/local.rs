//! 本地终端与本地文件系统接口。
//!
//! - `GET  /api/local/terminal?sid=&shell=&token=` 本地 PTY（WebSocket）
//!   与 `handlers::terminal` 共用前端协议，但走本地 PTY，不需要主机配置。
//! - `GET  /api/local/fs/list?path=`               列本地目录（空 path = 家目录）
//! - `GET  /api/local/fs/roots`                    列本地根（Windows 盘符 / Unix "/"）
//! - `POST /api/local/fs/download_to_local`        远端文件直接落盘本地目录
//!   （SFTP 双栏"下载到对侧"，跳过前端 blob 与另存为对话框）
//! - `POST /api/local/fs/upload_to_remote`         本地文件直传远端（"上传 ->"）

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, Json};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::service::local_fs;
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

#[derive(Debug, Deserialize)]
pub struct LocalFsListQuery {
    /// 绝对路径；空 / 缺省 = 用户家目录
    pub path: Option<String>,
}

/// 列本地目录（SFTP 双栏的本地侧）
pub async fn fs_list(
    Query(q): Query<LocalFsListQuery>,
) -> AppResult<Json<ApiResponse<crate::service::sftp::SftpListResp>>> {
    let resp = local_fs::list(q.path).await?;
    Ok(ApiResponse::ok(resp))
}

/// 列本地"根"（Windows 盘符 / Unix 根目录）
pub async fn fs_roots() -> AppResult<Json<ApiResponse<crate::service::sftp::SftpListResp>>> {
    let resp = local_fs::roots().await?;
    Ok(ApiResponse::ok(resp))
}

#[derive(Debug, Deserialize)]
pub struct DownloadToLocalReq {
    /// SFTP 会话 id
    pub sid: String,
    /// 远端文件绝对路径
    pub remote_path: String,
    /// 本地目标目录（绝对路径，不存在会自动创建，同名文件覆盖）
    pub local_dir: String,
}

/// 远端文件流式落盘到本地目录
pub async fn fs_download_to_local(
    Json(req): Json<DownloadToLocalReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let written = local_fs::download_to_local(&req.sid, &req.remote_path, &req.local_dir).await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "bytes": written })),
    }))
}

#[derive(Debug, Deserialize)]
pub struct UploadToRemoteReq {
    /// SFTP 会话 id
    pub sid: String,
    /// 本地文件绝对路径
    pub local_path: String,
    /// 远端目标绝对路径
    pub remote_path: String,
}

/// 本地文件流式直传到远端（"上传 ->"按钮）
pub async fn fs_upload_to_remote(
    Json(req): Json<UploadToRemoteReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let written = local_fs::upload_to_remote(&req.sid, &req.local_path, &req.remote_path).await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "bytes": written })),
    }))
}
