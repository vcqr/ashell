//! SFTP REST 接口（基于 sid 复用同一 SSH 连接）
//!
//! - POST /api/ssh/sftp/open                  显式打开 sftp 子通道（终端 WS 已自动打开，可省略）
//! - GET  /api/ssh/sftp                       列目录                ?sid=&path=
//! - POST /api/ssh/sftp/mkdir                 递归 mkdir            { sid, path }
//! - POST /api/ssh/sftp/touch                 创建文件              { sid, path }
//! - POST /api/ssh/sftp/remove_file           删除文件              { sid, path }
//! - POST /api/ssh/sftp/remove_dir            递归删除目录          { sid, path }
//! - POST /api/ssh/sftp/rename                重命名                { sid, old_path, new_path }
//! - GET  /api/ssh/sftp/download              下载文件流            ?sid=&filename=
//! - POST /api/ssh/sftp/upload                上传文件流(multipart) ?sid=&filename=
//! - POST /api/ssh/sftp/close                 释放 sid 关联会话     { sid }

use std::path::Path as StdPath;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::Multipart;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::handlers::{ok_msg, ApiResponse};
use crate::service::sftp as sftp_svc;
use crate::service::ssh as ssh_svc;
use crate::service::{self, AppState};

#[derive(Debug, Deserialize)]
pub struct OpenReq {
    pub host_id: i64,
    pub sid: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OpenResp {
    pub sid: String,
}

/// 显式打开一个独立的 sftp 会话（不通过终端 WS 时使用）
pub async fn open(
    State(state): State<AppState>,
    Json(req): Json<OpenReq>,
) -> AppResult<Json<ApiResponse<OpenResp>>> {
    let sid = req.sid.unwrap_or_else(|| Uuid::new_v4().to_string());
    let host =
        service::host::get_with_credentials(&state.db, &state.config.crypto_key, req.host_id)
            .await?;
    let session =
        ssh_svc::Session::connect(&state.db, &state.config.crypto_key, &host).await?;
    let session_arc = Arc::new(session);
    ssh_svc::set_client(sid.clone(), session_arc.clone()).await;
    session_arc.open_sftp(&sid).await?;
    Ok(ApiResponse::ok(OpenResp { sid }))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub sid: String,
    pub path: Option<String>,
}

pub async fn list(
    Query(q): Query<ListQuery>,
) -> AppResult<Json<ApiResponse<sftp_svc::SftpListResp>>> {
    let resp = sftp_svc::list(&q.sid, q.path).await?;
    Ok(ApiResponse::ok(resp))
}

#[derive(Debug, Deserialize)]
pub struct PathReq {
    pub sid: String,
    pub path: String,
}

pub async fn mkdir(
    Json(req): Json<PathReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::mkdir(&req.sid, &req.path).await?;
    Ok(ok_msg("ok"))
}

pub async fn touch(
    Json(req): Json<PathReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::touch(&req.sid, &req.path).await?;
    Ok(ok_msg("ok"))
}

pub async fn remove_file(
    Json(req): Json<PathReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::remove_file(&req.sid, &req.path).await?;
    Ok(ok_msg("ok"))
}

pub async fn remove_dir(
    Json(req): Json<PathReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::remove_dir(&req.sid, &req.path).await?;
    Ok(ok_msg("ok"))
}

#[derive(Debug, Deserialize)]
pub struct RenameReq {
    pub sid: String,
    pub old_path: String,
    pub new_path: String,
}

pub async fn rename(
    Json(req): Json<RenameReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::rename(&req.sid, &req.old_path, &req.new_path).await?;
    Ok(ok_msg("ok"))
}

/// 移动（跨目录 rename；同文件系统内瞬时完成）
pub async fn move_path(
    Json(req): Json<RenameReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::move_path(&req.sid, &req.old_path, &req.new_path).await?;
    Ok(ok_msg("ok"))
}

#[derive(Debug, Deserialize)]
pub struct DuplicateReq {
    pub sid: String,
    pub src_path: String,
    pub dst_dir: String,
}

/// 远程内部复制（目录递归，Rust 进程内流式中转）
pub async fn duplicate(
    Json(req): Json<DuplicateReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    sftp_svc::duplicate(&req.sid, &req.src_path, &req.dst_dir).await?;
    Ok(ok_msg("ok"))
}

#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    pub sid: String,
    pub filename: String,
}

/// 流式下载远程文件
pub async fn download(Query(q): Query<DownloadQuery>) -> Result<axum::response::Response, AppError> {
    let (meta, stream) = sftp_svc::open_for_read(&q.sid, &q.filename).await?;

    let basename = StdPath::new(&q.filename)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid filename".into()))?;
    let disposition = format!("attachment; filename=\"{}\"", basename);
    let len = meta.size.unwrap_or(0).to_string();

    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_LENGTH, len.as_str()),
            (header::CONTENT_DISPOSITION, disposition.as_str()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    pub sid: String,
    pub filename: String,
}

/// multipart 流式上传
pub async fn upload(
    Query(q): Query<UploadQuery>,
    mut multipart: Multipart,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let mut file = sftp_svc::open_for_write(&q.sid, &q.filename).await?;

    while let Ok(Some(mut field)) = multipart.next_field().await {
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk)
                        .await
                        .map_err(|e| sftp_svc::sftp_err("write", e))?;
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(AppError::BadRequest(format!("read multipart: {e}")));
                }
            }
        }
    }

    file.flush()
        .await
        .map_err(|e| sftp_svc::sftp_err("flush", e))?;
    file.shutdown()
        .await
        .map_err(|e| sftp_svc::sftp_err("close", e))?;

    Ok(ok_msg("ok"))
}

#[derive(Debug, Deserialize)]
pub struct CloseReq {
    pub sid: String,
}

pub async fn close(
    Json(req): Json<CloseReq>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ssh_svc::remove(&req.sid).await;
    Ok(ok_msg("closed"))
}

// 兼容路径参数风格的 close
pub async fn close_by_path(
    Path(sid): Path<String>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    ssh_svc::remove(&sid).await;
    Ok(ok_msg("closed"))
}
