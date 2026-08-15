//! 本地终端与本地文件系统接口。
//!
//! - `GET  /api/local/terminal?sid=&shell=&token=` 本地 PTY（WebSocket）
//!   与 `handlers::terminal` 共用前端协议，但走本地 PTY，不需要主机配置。
//! - `GET  /api/local/fs/list?path=`               列本地目录（空 path = 家目录）
//! - `GET  /api/local/fs/roots`                    列本地根（Windows 盘符 / Unix "/"）
//! - `POST /api/local/fs/download_to_local`        远端文件直接落盘本地目录
//!   （SFTP 双栏"下载到对侧"，跳过前端 blob 与另存为对话框）
//! - `POST /api/local/fs/upload_to_remote`         本地文件直传远端（"上传 ->"）
//! - `POST /api/local/fs/save_file`                OS 拖放文件落盘本地目录
//!   （双栏下拖进本地栏；multipart 字节流，同名覆盖）
//! - `POST /api/local/fs/progress`                 轮询直传任务进度
//!   （Rust 进程内直传不经过 webview，进度按 task_id 记账）

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, Json};
use axum::response::IntoResponse;
use axum_extra::extract::Multipart;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
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
    /// 前端任务 id：提供时记录进度，供 /progress 轮询
    pub task_id: Option<String>,
}

/// 远端文件流式落盘到本地目录
pub async fn fs_download_to_local(
    Json(req): Json<DownloadToLocalReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let res = local_fs::download_to_local(
        &req.sid,
        &req.remote_path,
        &req.local_dir,
        req.task_id.as_deref(),
    )
    .await;
    if let Some(id) = &req.task_id {
        local_fs::progress_remove(id);
    }
    let written = res?;
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
    /// 前端任务 id：提供时记录进度，供 /progress 轮询
    pub task_id: Option<String>,
}

/// 本地文件流式直传到远端（"上传 ->"按钮）
pub async fn fs_upload_to_remote(
    Json(req): Json<UploadToRemoteReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let res = local_fs::upload_to_remote(
        &req.sid,
        &req.local_path,
        &req.remote_path,
        req.task_id.as_deref(),
    )
    .await;
    if let Some(id) = &req.task_id {
        local_fs::progress_remove(id);
    }
    let written = res?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "bytes": written })),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ProgressReq {
    pub task_ids: Vec<String>,
}

/// 批量查询直传任务进度；未知（未开始 / 已结束清理）的 id 不出现在结果里
pub async fn fs_progress(
    Json(req): Json<ProgressReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let mut map = serde_json::Map::new();
    for id in req.task_ids {
        if let Some(bytes) = local_fs::progress_get(&id) {
            map.insert(id, serde_json::json!(bytes));
        }
    }
    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::Value::Object(map)),
    }))
}

#[derive(Debug, Deserialize)]
pub struct SaveLocalFileQuery {
    /// 本地目标目录（绝对路径，父目录按需创建）
    pub dir: String,
    /// 相对目标路径（可含子目录，如 "sub/a.txt"；不允许 ".." / 绝对路径）
    pub name: String,
}

/// OS 拖放文件落盘本地目录（双栏下拖进本地栏）。
/// multipart 字节流式写盘；同名文件覆盖，返回写入字节数。
pub async fn fs_save_file(
    Query(q): Query<SaveLocalFileQuery>,
    mut multipart: Multipart,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let target = local_fs::prepare_save_path(&q.dir, &q.name).await?;
    let file = tokio::fs::File::create(&target)
        .await
        .map_err(|e| AppError::BadRequest(format!("create {}: {e}", target.to_string_lossy())))?;
    let mut writer = tokio::io::BufWriter::new(file);

    let mut written: u64 = 0;
    while let Ok(Some(mut field)) = multipart.next_field().await {
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    writer
                        .write_all(&chunk)
                        .await
                        .map_err(|e| AppError::BadRequest(format!("write: {e}")))?;
                    written += chunk.len() as u64;
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(AppError::BadRequest(format!("read multipart: {e}")));
                }
            }
        }
    }
    writer
        .flush()
        .await
        .map_err(|e| AppError::BadRequest(format!("flush: {e}")))?;

    Ok(Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: Some(serde_json::json!({ "bytes": written })),
    }))
}
