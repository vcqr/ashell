//! 用系统命令打开文件 / 在文件管理器中定位（std 实现，无第三方依赖）。
//!
//! 替代 tauri-plugin-opener 的 Rust 侧用途：opener 插件在 Linux 上会把
//! tauri→GTK/GLib 整条依赖链拖进 server 目标。行为与插件一致：
//! - open：macOS `open` / Windows `explorer` / Linux `xdg-open`
//! - reveal：macOS `open -R` / Windows `explorer /select,` / Linux 打开父目录

use std::path::Path;
use std::process::Command;

/// 用系统默认程序打开路径（文件或目录）
pub fn open_path(path: &Path) -> Result<(), String> {
    let displayed = path.display().to_string();
    #[cfg(target_os = "macos")]
    spawn("open", &[displayed.as_str()])?;

    #[cfg(target_os = "windows")]
    spawn("explorer", &[displayed.as_str()])?;

    #[cfg(all(unix, not(target_os = "macos")))]
    spawn("xdg-open", &[displayed.as_str()])?;

    Ok(())
}

/// 在系统文件管理器中定位显示该路径
pub fn reveal_item(path: &Path) -> Result<(), String> {
    let displayed = path.display().to_string();

    #[cfg(target_os = "macos")]
    spawn("open", &["-R", &displayed])?;

    #[cfg(target_os = "windows")]
    // explorer 的 /select 与路径必须是单个参数
    spawn("explorer", &[format!("/select,{displayed}")])?;

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // 无统一的"定位并选中"标准，退化为打开父目录
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".into());
        spawn("xdg-open", &[parent.as_str()])?;
    }

    Ok(())
}

fn spawn(program: &str, args: &[impl AsRef<str>]) -> Result<(), String> {
    let mut cmd = Command::new(program);
    for a in args {
        cmd.arg(a.as_ref());
    }
    cmd.status()
        .map_err(|e| format!("spawn {program}: {e}"))?
        .success()
        .then_some(())
        .ok_or_else(|| format!("{program} exited non-zero"))
}
