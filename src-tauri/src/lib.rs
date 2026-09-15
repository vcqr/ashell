// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
//
// 双目标：本 crate 同时服务桌面壳（bin/ashell，desktop feature）与 Web 服务器
// （bin/ashell-server，--no-default-features）。核心业务层（config/handlers/
// middleware/models/routers/service/sidecar 等）为纯 Rust 实现，两种形态共用；
// tray/hotkey/commands 中的原生对话框等桌面专属模块用 cfg(feature) 门控。

mod ai_env;
mod commands;
mod config;
mod errors;
mod handlers;
#[cfg(feature = "desktop")]
mod hotkey;
mod middleware;
mod models;
mod opener;
mod routers;
pub mod server;
mod service;
mod sidecar;
mod sidecar_factory;
#[cfg(feature = "desktop")]
mod tray;

#[cfg(feature = "desktop")]
use std::net::SocketAddr;
#[cfg(feature = "desktop")]
use std::sync::Arc;

#[cfg(feature = "desktop")]
use serde::Serialize;
#[cfg(feature = "desktop")]
use tauri::{
    image::Image,
    menu::{AboutMetadataBuilder, MenuBuilder, SubmenuBuilder},
    Manager, State,
};
#[cfg(feature = "desktop")]
use tokio::net::TcpListener;
#[cfg(feature = "desktop")]
use tokio::sync::OnceCell;

#[cfg(feature = "desktop")]
use crate::config::AppConfig;
#[cfg(feature = "desktop")]
use crate::service::AppState;

/// 暴露给前端的 API 信息
#[derive(Debug, Clone, Serialize)]
#[cfg(feature = "desktop")]
pub struct ApiInfo {
    pub addr: String,
    pub token: String,
    pub base_url: String,
    pub ws_url: String,
}

/// 应用全局上下文（在 Tauri setup 中初始化并 manage 给全局）
#[cfg(feature = "desktop")]
pub struct AppCtx {
    pub api: OnceCell<ApiInfo>,
}

#[cfg(feature = "desktop")]
impl AppCtx {
    pub fn new() -> Self {
        Self {
            api: OnceCell::new(),
        }
    }
}

/// 前端通过 invoke("get_api_info") 取得 API 地址与 Token
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_api_info(ctx: State<'_, AppCtx>) -> Result<ApiInfo, String> {
    ctx.api
        .get()
        .cloned()
        .ok_or_else(|| "api server not started yet".into())
}

/// 关闭 WebView2 的浏览器快捷键（F5 刷新 / F11 全屏 / Ctrl+F 查找 / Ctrl+P 打印等）。
/// 终端应用里误触 F5 会整页重载、丢失全部 UI 状态；同时这些键被 WebView2 拦截后
/// 前端永远收不到，无法录制成全局热键。仅 Windows 有此机制（macOS/Linux 无）。
#[cfg(all(windows, feature = "desktop"))]
fn disable_browser_accelerators(win: &tauri::WebviewWindow) {
    let result = win.with_webview(|webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows::core::Interface;

        let controller = webview.controller();
        let Ok(web) = (unsafe { controller.CoreWebView2() }) else {
            return;
        };
        let Ok(settings) = (unsafe { web.Settings() }) else {
            return;
        };
        if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
            unsafe {
                let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
        }
    });
    if let Err(e) = result {
        log::warn!("disable browser accelerators: {e}");
    }
}

/// 在系统默认文件管理器中打开 ~/.ashell/icons 目录。
/// 路径完全由后端控制（用户无法传任意路径），所以无需 opener scope 配置。
#[cfg(feature = "desktop")]
#[tauri::command]
fn open_icons_dir() -> Result<(), String> {
    let dir = config::icons_dir().map_err(|e| e.to_string())?;
    opener::open_path(&dir)
}

/// 返回 ~/.ashell/ai 目录的绝对路径（AI sidecar 工作目录）。
/// 用于前端在 spawn_sidecar 时传入 workspace 参数。
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_ai_dir() -> Result<String, String> {
    let dir = config::ai_dir().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[cfg(feature = "desktop")]
async fn start_api_server() -> anyhow::Result<ApiInfo> {
    // 1) 初始化配置
    let mut cfg: AppConfig = config::init()?;

    // 1.1) 内置默认图标按需写入 ~/.ashell/icons/（失败不阻塞）
    if let Err(e) = service::icons::ensure_defaults() {
        log::warn!("ensure default icons: {e}");
    }

    // 2) 初始化 DB
    let pool = models::init_pool(&cfg.db_path)
        .await
        .map_err(|e| anyhow::anyhow!("init db: {e}"))?;

    // 3) 绑定随机端口（127.0.0.1:0）
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let local_addr: SocketAddr = listener.local_addr()?;
    cfg.api_addr = local_addr.to_string();

    let token = cfg.token.clone();
    let cfg_arc = Arc::new(cfg.clone());

    // 4) 注入全局
    config::set_global(cfg);

    let state = AppState {
        db: pool,
        config: cfg_arc,
    };

    let app = routers::build_router(state);

    let info = ApiInfo {
        addr: local_addr.to_string(),
        token: token.clone(),
        base_url: format!("http://{}", local_addr),
        ws_url: format!("ws://{}", local_addr),
    };

    log::info!("ashell API listening on http://{}", local_addr);

    // 5) 后台运行 server
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("axum serve error: {e}");
        }
    });

    Ok(info)
}

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志
    // tracing-subscriber 处理 tracing 事件；env_logger 处理 log 宏输出（替代 tracing-log 桥接）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(true)
        .init();
    env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("RUST_LOG", "info"),
    )
    .write_style(env_logger::WriteStyle::Always)
    .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // 全局快捷键唤起（quake 式显示/隐藏主窗口）；注册的热键由 hotkey 模块管理
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        hotkey::toggle_main_window(app);
                    }
                })
                .build(),
        )
        .manage(AppCtx::new())
        .on_window_event(|window, event| {
            // 主窗口关闭策略：最小化到托盘时拦截关闭，仅隐藏窗口（SSH 会话保活）。
            // 其余动态窗口（ashell-win/sftp/ai-*）不拦截，照常关闭。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                let s = tray::settings();
                if s.enabled && s.close_action == tray::CloseAction::Hide {
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        log::warn!("hide main window to tray: {e}");
                    }
                }
            }
        })
        .setup(|app| {
            // 自定义应用菜单，使 macOS "关于" 面板显示正确的应用图标
            let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            let about_metadata = AboutMetadataBuilder::new()
                .name(Some("AShell".to_string()))
                .version(Some(env!("CARGO_PKG_VERSION").to_string()))
                .short_version(Some(env!("CARGO_PKG_VERSION").to_string()))
                .copyright(Some("© 2026 vcqr. All rights reserved.".to_string()))
                .credits(Some("GitHub: https://github.com/vcqr/ashell\nGitee: https://gitee.com/vcqr/ashell".to_string()))
                .authors(Some(vec![env!("CARGO_PKG_AUTHORS").to_string()]))
                .comments(Some(env!("CARGO_PKG_DESCRIPTION").to_string()))
                .license(Some(env!("CARGO_PKG_LICENSE").to_string()))
                .website(Some(env!("CARGO_PKG_REPOSITORY").to_string()))
                .website_label(Some("GitHub".to_string()))
                .icon(Some(icon))
                .build();

            let app_menu = SubmenuBuilder::new(app, "AShell")
                .about(Some(about_metadata))
                .separator()
                .services()
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;

            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;

            let window_menu = SubmenuBuilder::new(app, "Window")
                .minimize()
                .maximize()
                .separator()
                .bring_all_to_front()
                .build()?;

            let menu = MenuBuilder::new(app)
                .items(&[&app_menu, &edit_menu, &window_menu])
                .build()?;
            app.set_menu(menu)?;

            // 前端正常情况下会在首帧后主动 show；这里兜底：前端异常时避免窗口一直隐藏。
            if let Some(win) = app.get_webview_window("main") {
                // 关闭 WebView2 浏览器快捷键（仅 Windows 生效，其它平台无此机制）
                #[cfg(windows)]
                disable_browser_accelerators(&win);
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if win.is_visible().ok() == Some(false) {
                        if let Err(e) = win.show() {
                            log::warn!("fallback show main window: {e}");
                        }
                    }
                });
            }

            // 系统托盘（设置持久化在 ~/.ashell/tray.json，关闭策略在 Rust 侧拦截窗口关闭）
            if let Err(e) = tray::setup(app.handle()) {
                log::error!("setup tray: {e}");
            }

            // 全局快捷键唤起（设置持久化在 ~/.ashell/hotkey.json）
            if let Err(e) = hotkey::setup(app.handle()) {
                log::error!("setup global hotkey: {e}");
            }

            let handle = app.handle().clone();
            // 在 Tauri 自带的 tokio runtime 中启动 API server
            tauri::async_runtime::spawn(async move {
                match start_api_server().await {
                    Ok(info) => {
                        if let Some(ctx) = handle.try_state::<AppCtx>() {
                            let _ = ctx.api.set(info);
                        }
                    }
                    Err(e) => {
                        log::error!("failed to start api server: {e}");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_api_info,
            commands::fonts::list_system_fonts,
            open_icons_dir,
            get_ai_dir,
            commands::dialog::save_text_file,
            commands::dialog::pick_image_file,
            commands::dialog::pick_private_key_file,
            commands::dialog::open_text_file,
            commands::wallpaper::set_wallpaper,
            commands::wallpaper::get_wallpaper,
            commands::wallpaper::clear_wallpaper,
            ai_env::read_ai_paths,
            ai_env::write_ai_paths,
            ai_env::detect_claude_path,
            ai_env::fetch_models,
            sidecar::spawn_sidecar,
            sidecar::prewarm_ai_daemon,
            sidecar::write_to_sidecar,
            sidecar::kill_sidecar,
            sidecar::get_sidecar_pid,
            sidecar::has_sidecar,
            tray::tray_get_settings,
            tray::tray_set_settings,
            tray::tray_get_autostart,
            tray::tray_set_autostart,
            tray::tray_apply_locale,
            hotkey::hotkey_get_settings,
            hotkey::hotkey_set_settings,
            hotkey::hotkey_suspend,
            hotkey::hotkey_resume
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_handle, event| {
            match event {
                // 应用退出时清理所有 sidecar 子进程，避免僵尸进程
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                    sidecar::kill_all_sidecars();
                }
                // macOS：窗口最小化到托盘后点 Dock 图标恢复主窗口
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen {
                    has_visible_windows: false,
                    ..
                } => {
                    tray::show_main_window(_handle);
                }
                _ => {}
            }
        });
}
