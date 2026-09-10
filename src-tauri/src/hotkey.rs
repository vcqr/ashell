//! 全局快捷键唤起：在系统任意位置按下快捷键显示/隐藏主窗口（quake 式切换）。
//!
//! 设置持久化在 `~/.ashell/hotkey.json`（后端权威）：全局热键的注册与回调
//! 发生在 Rust 侧，与前端是否加载完成无关，不能依赖 localStorage。

use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::config;

/// 全局快捷键设置（~/.ashell/hotkey.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeySettings {
    /// 是否启用全局快捷键
    pub enabled: bool,
    /// 快捷键字符串，如 "Ctrl+Shift+K"（global-hotkey 解析格式）；空串表示未设置
    pub accelerator: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        // 默认关闭：不预设具体按键，避免与用户系统/输入法既有快捷键冲突
        Self {
            enabled: false,
            accelerator: String::new(),
        }
    }
}

static SETTINGS: RwLock<Option<HotkeySettings>> = RwLock::new(None);

fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(config::app_dir()?.join("hotkey.json"))
}

/// 读取热键设置（带缓存；文件缺失/损坏走默认值）
pub fn settings() -> HotkeySettings {
    if let Ok(guard) = SETTINGS.read() {
        if let Some(s) = guard.as_ref() {
            return s.clone();
        }
    }
    let loaded = read_from_disk();
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(loaded.clone());
    }
    loaded
}

fn read_from_disk() -> HotkeySettings {
    let path = match settings_path() {
        Ok(p) => p,
        Err(_) => return HotkeySettings::default(),
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn persist(s: &HotkeySettings) -> Result<(), String> {
    let path = settings_path().map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(s.clone());
    }
    Ok(())
}

/// quake 式切换：窗口可见且聚焦时隐藏；否则（最小化/隐藏/失焦）唤起并聚焦
pub fn toggle_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(false);
        let focused = win.is_focused().unwrap_or(false);
        if visible && focused {
            let _ = win.hide();
        } else {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// 校验快捷键字符串：必须是「修饰键 + 普通按键」的组合（裸 F1-F24 功能键除外，
/// 它们不参与打字、不会劫持输入），且能被 global-hotkey 解析。
/// 裸字母（如 "K"）注册成全局热键会劫持全系统输入，必须拒绝。
fn validate_accelerator(accelerator: &str) -> Result<(), String> {
    let parts: Vec<&str> = accelerator
        .split('+')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() == 1 {
        // 单键：仅放行 F1-F24 功能键
        let up = parts[0].to_ascii_uppercase();
        let is_function_key = up.len() > 1
            && up.starts_with('F')
            && up[1..]
                .parse::<u8>()
                .map(|n| (1..=24).contains(&n))
                .unwrap_or(false);
        if !is_function_key {
            return Err(
                "快捷键需要修饰键（Ctrl/Alt/Shift/Meta）加普通按键的组合；仅 F1-F24 可单独使用"
                    .into(),
            );
        }
    } else {
        let has_modifier = parts[..parts.len() - 1].iter().any(|p| {
            matches!(
                p.to_ascii_lowercase().as_str(),
                "ctrl" | "control" | "alt" | "option" | "shift" | "meta" | "cmd" | "command"
                    | "super" | "win"
            )
        });
        if !has_modifier {
            return Err("快捷键需要修饰键（Ctrl/Alt/Shift/Meta）加普通按键的组合".into());
        }
    }
    accelerator
        .parse::<Shortcut>()
        .map_err(|e| format!("无效快捷键 \"{accelerator}\": {e}"))?;
    Ok(())
}

/// 按设置重新注册全局热键（本应用只有一个全局快捷键，全量注销再注册）
fn apply<R: Runtime>(app: &AppHandle<R>, s: &HotkeySettings) -> Result<(), String> {
    let gs = app.global_shortcut();
    if let Err(e) = gs.unregister_all() {
        log::warn!("unregister global shortcuts: {e}");
    }
    if s.enabled && !s.accelerator.is_empty() {
        let shortcut: Shortcut = s
            .accelerator
            .parse()
            .map_err(|e| format!("无效快捷键 \"{}\": {e}", s.accelerator))?;
        gs.register(shortcut)
            .map_err(|e| format!("注册全局快捷键失败（可能被其他应用占用）: {e}"))?;
        log::info!("global hotkey registered: {}", s.accelerator);
    }
    Ok(())
}

/// 在 setup 中调用：按已持久化的设置注册热键（热键回调在插件 Builder 的 with_handler 中）
pub fn setup(app: &AppHandle) -> Result<(), String> {
    apply(app, &settings())
}

// ---------- Tauri commands ----------

#[tauri::command]
pub fn hotkey_get_settings() -> HotkeySettings {
    settings()
}

#[tauri::command]
pub fn hotkey_set_settings(
    app: AppHandle,
    enabled: bool,
    accelerator: String,
) -> Result<HotkeySettings, String> {
    let accelerator = accelerator.trim().to_string();
    if enabled && accelerator.is_empty() {
        return Err("启用全局快捷键前请先录制按键".into());
    }
    if !accelerator.is_empty() {
        validate_accelerator(&accelerator)?;
    }
    let prev = settings();
    let next = HotkeySettings {
        enabled,
        accelerator,
    };
    if let Err(e) = apply(&app, &next) {
        // 注册失败（冲突等）时回滚旧热键，避免用户失去唤起手段
        let _ = apply(&app, &prev);
        return Err(e);
    }
    if !next.enabled || next.accelerator.is_empty() {
        // 热键被关闭/清除时确保主窗口可见：否则托盘也关着的场景下窗口隐藏后将无法找回
        crate::tray::show_main_window(&app);
    }
    persist(&next)?;
    Ok(next)
}

/// 录制期间临时注销当前热键：否则已注册的组合键会被 OS 级 RegisterHotKey
/// 拦截（按下去直接触发唤起/隐藏，窗口一闪就没），前端录制器永远收不到。
/// 结束后必须调用 [`hotkey_resume`] 恢复。
#[tauri::command]
pub fn hotkey_suspend(app: AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())
}

/// 录制取消/组件卸载后按持久化设置恢复注册（保存路径由 hotkey_set_settings
/// 自己 apply 新设置，无需 resume）
#[tauri::command]
pub fn hotkey_resume(app: AppHandle) -> Result<(), String> {
    apply(&app, &settings())
}
