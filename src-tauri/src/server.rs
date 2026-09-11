//! Web 服务器形态的启动引导（`ashell-server` 二进制使用）。
//!
//! 与桌面壳的 `start_api_server`（随机端口 + Tauri 事件）相对，这里：
//! - 绑定固定地址（默认 127.0.0.1:8090，`ASHELL_BIND` / `--bind` 可覆盖）
//! - 访问令牌持久化（首次启动生成写入 ~/.ashell/web-token，之后沿用），
//!   也可用 `ASHELL_WEB_TOKEN` / `--token` 显式指定
//! - 可选托管前端构建产物 dist/（`ASHELL_DIST` / `--dist`），同源部署
//! - 打印访问地址与令牌，Ctrl+C 优雅退出

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use tower_http::services::ServeDir;

use crate::config::AppConfig;
use crate::service::AppState;

pub struct ServerOptions {
    /// 监听地址，如 "127.0.0.1:8090" 或 "0.0.0.0:8090"
    pub bind: String,
    /// 显式访问令牌（None = 环境变量/持久化文件）
    pub token: Option<String>,
    /// 前端 dist 目录（None = 仅提供 API）
    pub dist: Option<PathBuf>,
    /// 跳过 OS 钥匙串，强制使用文件密钥（无头部署免钥匙串授权弹窗）
    pub force_key_file: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8090".to_string(),
            token: None,
            dist: None,
            force_key_file: false,
        }
    }
}

/// 运行 Web 服务器（阻塞直到 Ctrl+C / SIGTERM）。
pub async fn run(opts: ServerOptions) -> anyhow::Result<()> {
    if opts.force_key_file {
        // config::keyring_entry 依据此环境变量跳过 OS 钥匙串
        std::env::set_var("ASHHELL_FORCE_KEY_FILE", "1");
    }

    // 1) 初始化配置
    let mut cfg: AppConfig = crate::config::init()?;

    // 1.1) 内置默认图标按需写入 ~/.ashell/icons/（失败不阻塞）
    if let Err(e) = crate::service::icons::ensure_defaults() {
        log::warn!("ensure default icons: {e}");
    }

    // 1.2) 解析访问令牌：显式参数 > 环境变量 > 持久化文件（首次生成）
    let token = resolve_web_token(opts.token)?;
    cfg.token = token;

    // 2) 初始化 DB
    let pool = crate::models::init_pool(&cfg.db_path)
        .await
        .map_err(|e| anyhow::anyhow!("init db: {e}"))?;

    // 3) 绑定监听地址
    let listener = tokio::net::TcpListener::bind(&opts.bind).await?;
    let local_addr: SocketAddr = listener.local_addr()?;
    cfg.api_addr = local_addr.to_string();

    let token = cfg.token.clone();
    let cfg_arc = Arc::new(cfg.clone());

    // 4) 注入全局
    crate::config::set_global(cfg);

    let state = AppState {
        db: pool,
        config: cfg_arc,
    };

    let app: Router = crate::routers::build_router(state);

    // 5) 可选托管前端静态资源（同源部署：base_url = ''）
    let app = match opts.dist {
        Some(dist) if dist.exists() => {
            log::info!("serving frontend from {}", dist.display());
            app.fallback_service(ServeDir::new(dist).append_index_html_on_directories(true))
        }
        Some(dist) => {
            anyhow::bail!("dist directory not found: {}", dist.display());
        }
        None => app,
    };

    log::info!("==============================================");
    log::info!("  AShell Web Server v{}", env!("CARGO_PKG_VERSION"));
    log::info!("  listening on http://{}", local_addr);
    log::info!("  access token: {}", token);
    log::info!("==============================================");

    // 6) 优雅退出
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // 清理 sidecar 子进程，避免僵尸进程
    crate::sidecar::kill_all_sidecars();
    Ok(())
}

/// 解析 Web 访问令牌。
///
/// 与桌面端"每次启动随机生成"不同，服务器场景下浏览器需要稳定的登录密码：
/// 首次生成后写入 ~/.ashell/web-token（0600），后续启动沿用；重启不影响已登录客户端。
fn resolve_web_token(explicit: Option<String>) -> anyhow::Result<String> {
    // 1) 命令行参数
    if let Some(t) = explicit.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        return Ok(t.to_string());
    }
    // 2) 环境变量
    if let Ok(t) = std::env::var("ASHELL_WEB_TOKEN") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    // 3) 持久化文件（不存在则生成）
    let path = crate::config::app_dir()?.join("web-token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let t = existing.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let token = generate_token();
    std::fs::write(&path, &token).map_err(|e| anyhow::anyhow!("write {:?}: {e}", path))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    log::info!("generated access token, saved to {:?}", path);
    Ok(token)
}

fn generate_token() -> String {
    use rand::RngExt;
    let mut buf = [0u8; 32];
    rand::rng().fill(&mut buf);
    hex::encode(buf)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    log::info!("shutdown signal received");
}
