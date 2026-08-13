// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod ai_env;
mod commands;
mod config;
mod errors;
mod handlers;
mod middleware;
mod models;
mod routers;
mod service;
mod sidecar;
mod sidecar_factory;

use std::net::SocketAddr;
use std::sync::Arc;

use serde::Serialize;
use tauri::{
    image::Image,
    menu::{AboutMetadataBuilder, MenuBuilder, SubmenuBuilder},
    Manager, State,
};
use tokio::net::TcpListener;
use tokio::sync::OnceCell;

use crate::config::AppConfig;
use crate::service::AppState;

/// 暴露给前端的 API 信息
#[derive(Debug, Clone, Serialize)]
pub struct ApiInfo {
    pub addr: String,
    pub token: String,
    pub base_url: String,
    pub ws_url: String,
}

/// 应用全局上下文（在 Tauri setup 中初始化并 manage 给全局）
pub struct AppCtx {
    pub api: OnceCell<ApiInfo>,
}

impl AppCtx {
    pub fn new() -> Self {
        Self {
            api: OnceCell::new(),
        }
    }
}

/// 前端通过 invoke("get_api_info") 取得 API 地址与 Token
#[tauri::command]
fn get_api_info(ctx: State<'_, AppCtx>) -> Result<ApiInfo, String> {
    ctx.api
        .get()
        .cloned()
        .ok_or_else(|| "api server not started yet".into())
}

/// 在系统默认文件管理器中打开 ~/.ashell/icons 目录。
/// 路径完全由后端控制（用户无法传任意路径），所以无需 opener scope 配置。
#[tauri::command]
fn open_icons_dir() -> Result<(), String> {
    let dir = config::icons_dir().map_err(|e| e.to_string())?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|e| e.to_string())
}

/// 返回 ~/.ashell/ai 目录的绝对路径（AI sidecar 工作目录）。
/// 用于前端在 spawn_sidecar 时传入 workspace 参数。
#[tauri::command]
fn get_ai_dir() -> Result<String, String> {
    let dir = config::ai_dir().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

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
        .manage(AppCtx::new())
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
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if win.is_visible().ok() == Some(false) {
                        if let Err(e) = win.show() {
                            log::warn!("fallback show main window: {e}");
                        }
                    }
                });
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
            sidecar::write_to_sidecar,
            sidecar::kill_sidecar,
            sidecar::get_sidecar_pid,
            sidecar::has_sidecar
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_handle, event| {
            // 应用退出时清理所有 sidecar 子进程，避免僵尸进程
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                sidecar::kill_all_sidecars();
            }
        });
}
