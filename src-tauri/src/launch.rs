//! 外部拉起的目录参数处理（资源管理器右键「在 Ashell 中打开」等场景）：
//! - 首次启动：argv 里的目录先存入 PENDING_DIR，前端主窗口挂载后
//!   invoke("launch_take_dir") 领取（拉模式，避免 emit 早于前端挂监听的竞态）
//! - 二次拉起：tauri-plugin-single-instance 回调里解析 argv，唤起主窗口
//!   并 emit("open-in-ashell") 推给前端（推模式）
//! 仅 Windows 桌面构建注册插件；命令本身跨平台无害（非 Windows 恒为 None）。

use std::path::Path;
use std::sync::Mutex;

use tauri::Emitter;

/// 首次启动待前端领取的目录
static PENDING_DIR: Mutex<Option<String>> = Mutex::new(None);

/// 解析 argv 中的目录参数：跳过 argv[0] 与 `-` 开头的开关，
/// 取第一个存在且为目录的参数。
pub fn parse_launch_dir(args: &[String]) -> Option<String> {
    args.iter().skip(1).find_map(|arg| {
        if arg.starts_with('-') {
            return None;
        }
        normalize_dir(arg)
    })
}

/// 首次启动：把 argv 中的目录存起来等前端来取（setup 里调用）
pub fn store_pending_dir_from_argv() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(dir) = parse_launch_dir(&args) {
        if let Ok(mut guard) = PENDING_DIR.lock() {
            *guard = Some(dir);
        }
    }
}

/// 前端领取首启目录（take 语义：只被主窗口领走一次）
#[tauri::command]
pub fn launch_take_dir() -> Option<String> {
    PENDING_DIR.lock().ok()?.take()
}

/// single-instance 回调（仅 Windows）：二次拉起时唤起主窗口（关闭即隐藏到
/// 托盘的常驻形态），带目录参数时再推事件让前端开一个本地标签
pub fn handle_second_instance(app: &tauri::AppHandle, args: &[String]) {
    crate::tray::show_main_window(app);
    if let Some(dir) = parse_launch_dir(args) {
        if let Err(e) = app.emit_to("main", "open-in-ashell", dir) {
            log::warn!("emit open-in-ashell: {e}");
        }
    }
}

/// 绝对路径化并去掉 Windows 规范化前缀（`\\?\`），终端里显示更自然
fn normalize_dir(raw: &str) -> Option<String> {
    let p = Path::new(raw);
    if !p.is_dir() {
        return None;
    }
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(p)
    };
    let s = abs.to_string_lossy();
    Some(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}
