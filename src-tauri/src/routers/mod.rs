use axum::extract::DefaultBodyLimit;
use axum::middleware as axum_mw;
use axum::routing::{get, post};
use axum::Router;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::Level;

use crate::handlers;
use crate::middleware::{auth_middleware, cors};
use crate::service::AppState;

/// 请求日志 MakeSpan：
/// - dev（debug_assertions）：记录完整 URI（含 query string），方便调试
/// - release：只记录 path，避免 token 等敏感参数泄露到日志
#[derive(Clone, Debug)]
struct RequestMakeSpan;

impl<B> MakeSpan<B> for RequestMakeSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        if cfg!(debug_assertions) {
            tracing::info_span!(
                "request",
                method = %request.method(),
                uri = %request.uri(),
                version = ?request.version(),
            )
        } else {
            tracing::info_span!(
                "request",
                method = %request.method(),
                path = %request.uri().path(),
                version = ?request.version(),
            )
        }
    }
}

/// 上传单文件最大 100GB（流式上传，不占内存）
const UPLOAD_LIMIT: usize = 100 * 1024 * 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    // 健康检查（不鉴权）
    let public = Router::new().route(
        "/health",
        get(|| async { axum::Json(serde_json::json!({"status": "ok"})) }),
    );

    // 业务路由（需鉴权）
    let api = Router::new()
        // groups
        .route(
            "/api/groups",
            get(handlers::group::list).post(handlers::group::create),
        )
        .route(
            "/api/groups/{id}",
            get(handlers::group::detail)
                .put(handlers::group::update)
                .delete(handlers::group::delete),
        )
        // hosts
        .route(
            "/api/hosts",
            get(handlers::host::list).post(handlers::host::create),
        )
        .route(
            "/api/hosts/{id}",
            get(handlers::host::detail)
                .put(handlers::host::update)
                .delete(handlers::host::delete),
        )
        // 从 ~/.ssh/config 导入主机
        .route(
            "/api/hosts/ssh-config",
            get(handlers::host::ssh_config),
        )
        // SSH 终端 WebSocket
        .route(
            "/api/ssh/terminal/{host_id}",
            get(handlers::terminal::ws_handler),
        )
        // 本地 PTY 终端 WebSocket
        .route(
            "/api/local/terminal",
            get(handlers::local::ws_handler),
        )
        // Telnet 终端 WebSocket
        .route(
            "/api/telnet/terminal/{host_id}",
            get(handlers::telnet::ws_handler),
        )
        // 串口终端 WebSocket
        .route(
            "/api/serial/terminal/{host_id}",
            get(handlers::serial::ws_handler),
        )
        // 向已有终端会话注入命令并收集输出
        .route(
            "/api/ssh/send/{sid}",
            post(handlers::terminal::send_handler),
        )
        // SFTP REST 系列
        .route("/api/ssh/sftp/open", post(handlers::sftp::open))
        .route("/api/ssh/sftp", get(handlers::sftp::list))
        .route("/api/ssh/sftp/mkdir", post(handlers::sftp::mkdir))
        .route("/api/ssh/sftp/touch", post(handlers::sftp::touch))
        .route(
            "/api/ssh/sftp/remove_file",
            post(handlers::sftp::remove_file),
        )
        .route(
            "/api/ssh/sftp/remove_dir",
            post(handlers::sftp::remove_dir),
        )
        .route("/api/ssh/sftp/rename", post(handlers::sftp::rename))
        .route("/api/ssh/sftp/download", get(handlers::sftp::download))
        .route("/api/ssh/sftp/upload", post(handlers::sftp::upload))
        .route("/api/ssh/sftp/close", post(handlers::sftp::close))
        .route(
            "/api/ssh/sftp/close/{sid}",
            post(handlers::sftp::close_by_path),
        )
        // 主机系统信息（复用已建立的 sid 会话）
        .route("/api/ssh/sysinfo", get(handlers::sysinfo::get))
        // 端口转发（-L / -R / -D）
        .route(
            "/api/ssh/forward",
            get(handlers::forward::list).post(handlers::forward::create),
        )
        .route(
            "/api/ssh/forward/{rule_id}",
            axum::routing::delete(handlers::forward::delete),
        )
        // 主机图标资源（用户放置在 ~/.ashell/icons/）
        .route("/api/icons", get(handlers::icons::list))
        .route("/api/icons/{name}", get(handlers::icons::get))
        // AI 供应商管理
        .route(
            "/api/ai-providers",
            get(handlers::ai_provider::list).post(handlers::ai_provider::create),
        )
        .route(
            "/api/ai-providers/{id}",
            get(handlers::ai_provider::detail)
                .put(handlers::ai_provider::update)
                .delete(handlers::ai_provider::delete),
        )
        // AI 引擎（sidecar）配置
        .route("/api/ai-engines", get(handlers::ai_provider::list_engines))
        .route(
            "/api/ai-engines/active",
            axum::routing::put(handlers::ai_provider::activate_engine),
        )
        .route(
            "/api/ai-engines/{engine}",
            axum::routing::put(handlers::ai_provider::update_engine),
        )
        // 上传体积上限放在 Router 级别（避免 MethodRouter::layer 类型推导歧义）
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(UPLOAD_LIMIT))
        .layer(axum_mw::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .merge(public)
        .merge(api)
        .layer(cors::cors_layer())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(RequestMakeSpan)
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}
