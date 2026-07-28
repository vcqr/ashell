//! 主机图标资源服务
//!
//! 用户图标存放在 ~/.ashell/icons/，支持 svg / png / jpg / jpeg / gif / webp。
//! 文件名（含扩展名）即为图标的逻辑标识，存储到 host.icon。

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config;
use crate::errors::{AppError, AppResult};

/// 允许的扩展名（小写）
const ALLOWED_EXTS: &[&str] = &["svg", "png", "jpg", "jpeg", "gif", "webp"];

/// 内置默认图标。首次启动时会被复制到 ~/.ashell/icons/，
/// 用户后续删除也不会再写入（仅"不存在时写入"）。
const DEFAULT_ICONS: &[(&str, &str)] = &[
    (
        "alma.svg",
        include_str!("../../assets/icons-default/alma.svg"),
    ),
    (
        "alpine.svg",
        include_str!("../../assets/icons-default/alpine.svg"),
    ),
    (
        "centos.svg",
        include_str!("../../assets/icons-default/centos.svg"),
    ),
    (
        "debian.svg",
        include_str!("../../assets/icons-default/debian.svg"),
    ),
    (
        "docker.svg",
        include_str!("../../assets/icons-default/docker.svg"),
    ),
    (
        "linux.svg",
        include_str!("../../assets/icons-default/linux.svg"),
    ),
    (
        "macos.svg",
        include_str!("../../assets/icons-default/macos.svg"),
    ),
    (
        "redhat.svg",
        include_str!("../../assets/icons-default/redhat.svg"),
    ),
    (
        "rocky.svg",
        include_str!("../../assets/icons-default/rocky.svg"),
    ),
    (
        "server.svg",
        include_str!("../../assets/icons-default/server.svg"),
    ),
    (
        "ubuntu.svg",
        include_str!("../../assets/icons-default/ubuntu.svg"),
    ),
    (
        "windows.svg",
        include_str!("../../assets/icons-default/windows.svg"),
    ),
];

#[derive(Debug, serde::Serialize)]
pub struct IconItem {
    /// 文件名（含扩展名），用作 host.icon
    pub name: String,
    /// MIME 类型，便于前端 NSelect 直接给 <img>
    pub mime: &'static str,
    /// 文件字节大小
    pub size: u64,
    /// 修改时间（Unix 秒），前端用于缓存失效（URL 上拼 ?v=<mtime>）
    pub mtime: u64,
}

fn ext_to_mime(ext: &str) -> Option<&'static str> {
    match ext {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// 把 SystemTime 转成 Unix 秒；解析失败返回 0
fn mtime_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 启动时把内置默认图标写入 ~/.ashell/icons/，仅当文件不存在时写入。
/// 失败不阻塞主流程；调用方记录日志即可。
pub fn ensure_defaults() -> AppResult<()> {
    let dir = config::icons_dir().map_err(|e| AppError::Internal(e.to_string()))?;
    for (name, content) in DEFAULT_ICONS {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        if let Err(e) = std::fs::write(&path, content.as_bytes()) {
            log::warn!("写入默认图标失败 {name}: {e}");
        }
    }
    Ok(())
}

/// 列出所有有效图标，按文件名排序
pub fn list() -> AppResult<Vec<IconItem>> {
    let dir = config::icons_dir().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut out: Vec<IconItem> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(AppError::Io(e)),
    };
    for entry in entries.flatten() {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        let Some(ext) = ext else { continue };
        if !ALLOWED_EXTS.iter().any(|e| *e == ext) {
            continue;
        }
        let Some(mime) = ext_to_mime(&ext) else {
            continue;
        };
        let mtime = meta.modified().map(mtime_secs).unwrap_or(0);
        out.push(IconItem {
            name,
            mime,
            size: meta.len(),
            mtime,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 解析单个图标文件路径并返回 (绝对路径, mime)。
/// name 必须是单段文件名（防路径穿越），扩展名必须在白名单内。
pub fn resolve(name: &str) -> AppResult<(PathBuf, &'static str)> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(AppError::BadRequest(format!("invalid icon name: {name}")));
    }
    let dir = config::icons_dir().map_err(|e| AppError::Internal(e.to_string()))?;
    let path = dir.join(name);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| AppError::BadRequest(format!("missing extension: {name}")))?;
    let mime = ext_to_mime(&ext)
        .ok_or_else(|| AppError::BadRequest(format!("unsupported extension: {ext}")))?;
    if !path.is_file() {
        return Err(AppError::NotFound(format!("icon not found: {name}")));
    }
    Ok((path, mime))
}
