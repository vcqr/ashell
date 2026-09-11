//! Web 形态专用的辅助 REST 端点（桌面端同样挂载，但桌面 UI 不使用）。
//!
//! 这些端点把桌面 Tauri commands 能力升级为 HTTP：
//! - GET  /api/system/fonts       系统字体列表（原 list_system_fonts 命令）
//! - GET  /api/wallpaper          当前壁纸路径（原 get_wallpaper 命令）
//! - POST /api/wallpaper          multipart 上传并设置壁纸（Web 无本地路径概念）
//! - DELETE /api/wallpaper        清除壁纸（原 clear_wallpaper 命令）
//! - GET  /api/wallpaper/file     壁纸图片内容（Web 端替代 asset protocol）
//! - GET  /api/ai/dir             AI sidecar 工作目录（原 get_ai_dir 命令）
//! - GET  /api/ai/paths           读 AI 路径配置（原 read_ai_paths 命令）
//! - POST /api/ai/paths           写 AI 路径配置（原 write_ai_paths 命令）
//! - POST /api/ai/detect-claude-path  探测本机 claude CLI（原 detect_claude_path 命令）
//! - POST /api/ai/fetch-models    从供应商 API 拉取模型列表（原 fetch_models 命令）
//! - POST /api/keys/upload        上传私钥文件到 ~/.ashell/keys/ 并返回服务端路径
//!                                （Web 端浏览器拿不到本地绝对路径，改为传内容）

use axum::body::Body;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::multipart::Field;
use axum_extra::extract::Multipart;
use serde::{Deserialize, Serialize};

use crate::commands::{fonts, wallpaper};
use crate::errors::{AppError, AppResult};
use crate::handlers::ApiResponse;

// ── 字体 ──

pub async fn system_fonts() -> Json<ApiResponse<Vec<String>>> {
    ApiResponse::ok(fonts::list_system_fonts())
}

// ── 壁纸 ──

#[derive(Debug, Serialize)]
pub struct WallpaperInfo {
    pub path: Option<String>,
}

pub async fn wallpaper_get() -> AppResult<Json<ApiResponse<WallpaperInfo>>> {
    let path = wallpaper::get_wallpaper().map_err(AppError::BadRequest)?;
    Ok(ApiResponse::ok(WallpaperInfo { path }))
}

pub async fn wallpaper_clear() -> AppResult<axum::response::Response> {
    wallpaper::clear_wallpaper().map_err(AppError::BadRequest)?;
    Ok((axum::http::StatusCode::OK, "ok").into_response())
}

/// multipart 上传设置壁纸；字段名 file，文件名扩展名决定格式
pub async fn wallpaper_set(mut multipart: Multipart) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let mut saved: Option<String> = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("read multipart: {e}")))?
    {
        let ext = field
            .file_name()
            .and_then(|n| std::path::Path::new(n).extension())
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_string();
        let data = read_field_bytes(&mut field).await?;
        saved = Some(wallpaper::save_wallpaper_bytes(&ext, &data).map_err(AppError::BadRequest)?);
    }
    match saved {
        Some(path) => Ok(ApiResponse::ok(serde_json::json!({ "path": path }))),
        None => Err(AppError::BadRequest("missing file field".into())),
    }
}

/// 当前壁纸图片内容；无壁纸时 404
pub async fn wallpaper_file() -> AppResult<axum::response::Response> {
    let path = wallpaper::find_current_wallpaper()
        .map_err(AppError::BadRequest)?
        .ok_or_else(|| AppError::NotFound("no wallpaper".into()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    let bytes = tokio::fs::read(&path).await?;
    Ok((
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")],
        Body::from(bytes),
    )
        .into_response())
}

// ── AI 配置 ──

pub async fn ai_dir() -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let dir = crate::config::ai_dir().map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(serde_json::json!({
        "path": dir.to_string_lossy(),
    })))
}

pub async fn ai_paths_get() -> AppResult<Json<ApiResponse<crate::ai_env::AiPathsConfig>>> {
    let paths = crate::ai_env::read_ai_paths().map_err(AppError::BadRequest)?;
    Ok(ApiResponse::ok(paths))
}

pub async fn ai_paths_set(
    Json(config): Json<crate::ai_env::AiPathsConfig>,
) -> AppResult<axum::response::Response> {
    crate::ai_env::write_ai_paths(config).map_err(AppError::BadRequest)?;
    Ok((axum::http::StatusCode::OK, "ok").into_response())
}

pub async fn ai_detect_claude() -> Json<ApiResponse<serde_json::Value>> {
    let path = crate::ai_env::detect_claude_path();
    ApiResponse::ok(serde_json::json!({ "path": path }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchModelsReq {
    pub base_url: String,
    pub api_key: String,
    pub api_type: String,
}

pub async fn ai_fetch_models(
    Json(req): Json<FetchModelsReq>,
) -> AppResult<Json<ApiResponse<Vec<String>>>> {
    let models = crate::ai_env::fetch_models(req.base_url, req.api_key, req.api_type)
        .await
        .map_err(AppError::BadRequest)?;
    Ok(ApiResponse::ok(models))
}

// ── 私钥上传（Web 端 HostForm 选私钥用）──

/// 上传私钥文件内容，写入 ~/.ashell/keys/<timestamp>-<filename>（0600），
/// 返回服务端绝对路径供主机配置 private_key_path 使用。
/// 仅允许上传，不提供读取接口——路径只进不出，避免被当作任意文件下载通道。
pub async fn key_upload(mut multipart: Multipart) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let mut saved: Option<String> = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("read multipart: {e}")))?
    {
        let name = field.file_name().unwrap_or("key").to_string();
        // 只取文件名部分，防路径穿越
        let safe_name = name
            .rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
            .unwrap_or("key")
            .to_string();
        let data = read_field_bytes(&mut field).await?;
        let dir = keys_dir()?;
        let dest = dir.join(format!("{}-{}", chrono::Utc::now().timestamp_millis(), safe_name));
        std::fs::write(&dest, &data).map_err(|e| AppError::Internal(format!("write key: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
        }
        saved = Some(dest.to_string_lossy().into_owned());
    }
    match saved {
        Some(path) => Ok(ApiResponse::ok(serde_json::json!({ "path": path }))),
        None => Err(AppError::BadRequest("missing file field".into())),
    }
}

fn keys_dir() -> AppResult<std::path::PathBuf> {
    let dir = crate::config::app_dir()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .join("keys");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::Internal(format!("create keys dir: {e}")))?;
    }
    Ok(dir)
}

/// 把 multipart field 的内容读成字节（16MB 上限，壁纸/私钥场景足够）
async fn read_field_bytes<'a>(field: &'a mut Field) -> AppResult<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                if out.len() + chunk.len() > 16 * 1024 * 1024 {
                    return Err(AppError::BadRequest("file too large (max 16MB)".into()));
                }
                out.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                return Err(AppError::BadRequest(format!("read multipart: {e}")));
            }
        }
    }
    Ok(out)
}

