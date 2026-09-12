//! 系统托盘：图标、右键菜单、关闭策略与开机自启。
//!
//! 设置持久化在 `~/.ashell/tray.json`（后端权威）：窗口 CloseRequested 拦截
//! 发生在 Rust 侧，且可能早于前端加载完成，不能依赖 localStorage。

use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuItem, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt as _;

use crate::config;

const TRAY_ID: &str = "ashell-tray";

const ID_SHOW: &str = "tray-show";
const ID_AUTOSTART: &str = "tray-autostart";
const ID_QUIT_ON_CLOSE: &str = "tray-quit-on-close";
const ID_HIDE_ON_CLOSE: &str = "tray-hide-on-close";
const ID_QUIT: &str = "tray-quit";

/// 点击主窗口关闭按钮（含 Alt+F4 / 任务栏关闭）时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseAction {
    /// 直接退出应用（默认，保持历史行为）
    Quit,
    /// 隐藏窗口到托盘，应用继续在后台运行（SSH 会话保活）
    Hide,
}

/// 托盘设置（~/.ashell/tray.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TraySettings {
    /// 是否启用系统托盘
    pub enabled: bool,
    /// 关闭主窗口时的行为
    pub close_action: CloseAction,
    /// 托盘菜单语言（前端启动/切换语言时同步过来），如 "zh-CN" / "en-US"
    pub locale: String,
}

impl Default for TraySettings {
    fn default() -> Self {
        // 托盘默认开启；关闭策略默认直接退出，与无托盘时代的行为一致
        Self {
            enabled: true,
            close_action: CloseAction::Quit,
            locale: "zh-CN".into(),
        }
    }
}

static SETTINGS: RwLock<Option<TraySettings>> = RwLock::new(None);

fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(config::app_dir()?.join("tray.json"))
}

/// 读取托盘设置（带缓存；文件缺失/损坏走默认值）
pub fn settings() -> TraySettings {
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

fn read_from_disk() -> TraySettings {
    let path = match settings_path() {
        Ok(p) => p,
        Err(_) => return TraySettings::default(),
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn persist(s: &TraySettings) -> Result<(), String> {
    let path = settings_path().map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(s.clone());
    }
    Ok(())
}

/// 托盘菜单文案（跟随界面语言）
fn t(locale: &str, key: &str) -> &'static str {
    let zh = locale.starts_with("zh");
    match (key, zh) {
        ("show", true) => "显示主窗口",
        ("show", false) => "Show Main Window",
        ("autostart", true) => "开机自启",
        ("autostart", false) => "Launch at Login",
        ("closeAction", true) => "关闭窗口时",
        ("closeAction", false) => "On Window Close",
        ("quitOnClose", true) => "退出 AShell",
        ("quitOnClose", false) => "Quit AShell",
        ("hideOnClose", true) => "最小化到托盘",
        ("hideOnClose", false) => "Minimize to Tray",
        ("quit", true) => "退出",
        ("quit", false) => "Quit",
        _ => "",
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let s = settings();
    let l = s.locale.as_str();
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);

    let show = MenuItem::with_id(app, ID_SHOW, t(l, "show"), true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        ID_AUTOSTART,
        t(l, "autostart"),
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit_on_close = CheckMenuItem::with_id(
        app,
        ID_QUIT_ON_CLOSE,
        t(l, "quitOnClose"),
        true,
        s.close_action == CloseAction::Quit,
        None::<&str>,
    )?;
    let hide_on_close = CheckMenuItem::with_id(
        app,
        ID_HIDE_ON_CLOSE,
        t(l, "hideOnClose"),
        true,
        s.close_action == CloseAction::Hide,
        None::<&str>,
    )?;
    let close_submenu = SubmenuBuilder::new(app, t(l, "closeAction"))
        .item(&quit_on_close)
        .item(&hide_on_close)
        .build()?;
    let quit = MenuItem::with_id(app, ID_QUIT, t(l, "quit"), true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    MenuBuilder::new(app)
        .item(&show)
        .item(&sep1)
        .item(&autostart)
        .item(&close_submenu)
        .item(&sep2)
        .item(&quit)
        .build()
}

/// 显示主窗口（从托盘 / Dock 恢复）
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn rebuild_menu<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match build_menu(app) {
            Ok(menu) => {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    log::error!("rebuild tray menu: {e}");
                }
            }
            Err(e) => log::error!("build tray menu: {e}"),
        }
    }
}

fn set_close_action<R: Runtime>(app: &AppHandle<R>, action: CloseAction) {
    let mut s = settings();
    if s.close_action == action {
        return;
    }
    s.close_action = action;
    if let Err(e) = persist(&s) {
        log::error!("save tray settings: {e}");
    }
    rebuild_menu(app);
}

fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("AShell")
        .menu(&menu)
        .on_tray_icon_event(|tray, event| {
            // 左键显示主窗口（右键弹菜单）。不做点击切换隐藏：
            // Windows 下双击会先触发 Click 再触发 DoubleClick，切换语义会抖动。
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                #[cfg(not(target_os = "macos"))]
                show_main_window(tray.app_handle());
                #[cfg(target_os = "macos")]
                let _ = &tray;
            }
        });
    // macOS 左键默认弹菜单（菜单栏习惯）；Windows/Linux 左键留给显示窗口。
    // shadowing 替代 let mut：macOS 上该行 cfg 掉后不会产生 unused_mut 警告
    #[cfg(not(target_os = "macos"))]
    let builder = builder.show_menu_on_left_click(false);
    builder.build(app)?;
    Ok(())
}

/// 前端/菜单修改设置后的统一入口：持久化并按开关重建托盘
fn apply_settings<R: Runtime>(app: &AppHandle<R>, s: TraySettings) -> Result<(), String> {
    persist(&s)?;
    app.remove_tray_by_id(TRAY_ID);
    if s.enabled {
        create_tray(app).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 在 setup 中调用：注册托盘菜单事件处理器，按设置创建托盘
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.on_menu_event(move |_app, event| match event.id().as_ref() {
        ID_SHOW => show_main_window(&handle),
        ID_AUTOSTART => {
            let autolaunch = handle.autolaunch();
            let next = !autolaunch.is_enabled().unwrap_or(false);
            let res = if next {
                autolaunch.enable()
            } else {
                autolaunch.disable()
            };
            match res {
                Ok(_) => rebuild_menu(&handle),
                Err(e) => log::error!("toggle autostart from tray: {e}"),
            }
        }
        ID_QUIT_ON_CLOSE => set_close_action(&handle, CloseAction::Quit),
        ID_HIDE_ON_CLOSE => set_close_action(&handle, CloseAction::Hide),
        ID_QUIT => handle.exit(0),
        _ => {}
    });

    if settings().enabled {
        create_tray(app)?;
    }
    Ok(())
}

// ---------- Tauri commands ----------

#[tauri::command]
pub fn tray_get_settings() -> TraySettings {
    settings()
}

#[tauri::command]
pub fn tray_set_settings(
    app: AppHandle,
    enabled: bool,
    close_action: String,
) -> Result<TraySettings, String> {
    let close_action = match close_action.as_str() {
        "hide" => CloseAction::Hide,
        _ => CloseAction::Quit,
    };
    let mut s = settings();
    s.enabled = enabled;
    s.close_action = close_action;
    apply_settings(&app, s.clone())?;
    Ok(s)
}

#[tauri::command]
pub fn tray_get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tray_set_autostart(app: AppHandle, enable: bool) -> Result<bool, String> {
    let autolaunch = app.autolaunch();
    if enable {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }
    rebuild_menu(&app);
    autolaunch.is_enabled().map_err(|e| e.to_string())
}

/// 前端把解析后的界面语言同步给托盘菜单
#[tauri::command]
pub fn tray_apply_locale(app: AppHandle, locale: String) {
    let normalized = if locale.starts_with("zh") {
        "zh-CN"
    } else {
        "en-US"
    };
    let mut s = settings();
    if s.locale == normalized {
        return;
    }
    s.locale = normalized.to_string();
    if persist(&s).is_ok() {
        rebuild_menu(&app);
    }
}
