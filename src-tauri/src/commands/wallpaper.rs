/// 壁纸最大 10 MB
const MAX_WALLPAPER_SIZE: u64 = 10 * 1024 * 1024;

/// 允许的图片扩展名
const ALLOWED_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];

/// 在壁纸目录中查找当前壁纸文件（current.*）
fn find_current(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("current.") {
            return Some(entry.path());
        }
    }
    None
}

/// 清除壁纸目录中所有 current.* 文件
fn remove_all_current(dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("current.") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// 设置窗口背景壁纸：把用户选择的图片复制到 ~/.ashell/wallpaper/current.<ext>，
/// 返回文件绝对路径，前端通过 asset protocol 加载（避免 base64 全量读入内存）。
#[tauri::command]
pub fn set_wallpaper(source_path: String) -> Result<String, String> {
    let src = std::path::PathBuf::from(&source_path);
    if !src.exists() {
        return Err(format!("文件不存在: {}", source_path));
    }

    // 扩展名白名单校验
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !ALLOWED_EXTS.contains(&ext.as_str()) {
        return Err(format!(
            "不支持的图片格式: .{ext}（支持 {}）",
            ALLOWED_EXTS.join("/")
        ));
    }

    // 文件大小校验
    let meta = std::fs::metadata(&src).map_err(|e| format!("读取文件信息失败: {e}"))?;
    if meta.len() > MAX_WALLPAPER_SIZE {
        return Err(format!(
            "图片过大（{:.1} MB），最大支持 {} MB",
            meta.len() as f64 / 1024.0 / 1024.0,
            MAX_WALLPAPER_SIZE / 1024 / 1024
        ));
    }

    let dir = crate::config::wallpaper_dir().map_err(|e| e.to_string())?;
    // 清除旧壁纸（可能是不同扩展名）
    remove_all_current(&dir);
    // 保留扩展名，asset protocol 据此推断 MIME
    let dest = dir.join(format!("current.{ext}"));
    std::fs::copy(&src, &dest).map_err(|e| format!("复制壁纸失败: {e}"))?;
    Ok(dest.to_string_lossy().into_owned())
}

/// 获取当前壁纸的文件路径，不存在时返回 null。
/// 前端通过 convertFileSrc() 转为 asset protocol URL 加载。
#[tauri::command]
pub fn get_wallpaper() -> Result<Option<String>, String> {
    let dir = crate::config::wallpaper_dir().map_err(|e| e.to_string())?;
    Ok(find_current(&dir).map(|p| p.to_string_lossy().into_owned()))
}

/// 清除壁纸
#[tauri::command]
pub fn clear_wallpaper() -> Result<(), String> {
    let dir = crate::config::wallpaper_dir().map_err(|e| e.to_string())?;
    remove_all_current(&dir);
    Ok(())
}