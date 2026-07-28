/// 弹出系统"另存为"对话框，让用户选择路径，再把文本内容写入选中的文件。
///
/// 用于前端"导出会话内容"等场景。返回值：
/// - `Ok(Some(path))`：用户选择了路径并写入成功，返回保存的绝对路径
/// - `Ok(None)`：用户取消了对话框
/// - `Err(msg)`：选了路径但写入时报错
#[tauri::command]
pub async fn save_text_file(
    app: tauri::AppHandle,
    default_filename: Option<String>,
    content: String,
) -> Result<Option<String>, String> {
    use std::sync::mpsc;
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = mpsc::channel::<Option<std::path::PathBuf>>();
    let mut builder = app.dialog().file().add_filter("Text", &["txt", "log"]);
    if let Some(name) = default_filename.as_deref() {
        builder = builder.set_file_name(name);
    }
    builder.save_file(move |path| {
        let pb = path.and_then(|p| p.into_path().ok());
        let _ = tx.send(pb);
    });
    let chosen = rx
        .recv()
        .map_err(|e| format!("dialog channel closed: {e}"))?;
    let Some(path) = chosen else {
        return Ok(None);
    };
    std::fs::write(&path, content.as_bytes())
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// 弹出系统"打开文件"对话框让用户选择一张图片，返回选中文件的绝对路径。
/// 用户取消时返回 None。
#[tauri::command]
pub async fn pick_image_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use std::sync::mpsc;
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = mpsc::channel::<Option<std::path::PathBuf>>();
    app.dialog()
        .file()
        .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"])
        .pick_file(move |path| {
            let pb = path.and_then(|p| p.into_path().ok());
            let _ = tx.send(pb);
        });
    let chosen = rx
        .recv()
        .map_err(|e| format!("dialog channel closed: {e}"))?;
    Ok(chosen.map(|p| p.to_string_lossy().into_owned()))
}

/// 弹出系统"打开文件"对话框让用户选择一个私钥文件，返回选中文件的绝对路径。
/// 用户取消时返回 None。
#[tauri::command]
pub async fn pick_private_key_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use std::sync::mpsc;
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = mpsc::channel::<Option<std::path::PathBuf>>();
    app.dialog()
        .file()
        .add_filter("私钥", &["pem", "key", "id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"])
        .pick_file(move |path| {
            let pb = path.and_then(|p| p.into_path().ok());
            let _ = tx.send(pb);
        });
    let chosen = rx
        .recv()
        .map_err(|e| format!("dialog channel closed: {e}"))?;
    Ok(chosen.map(|p| p.to_string_lossy().into_owned()))
}