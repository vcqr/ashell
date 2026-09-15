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
    // 健康检查（不鉴权）；附带版本号供 Web 端“关于”页替代 getVersion
    let public = Router::new()
        .route(
            "/health",
            get(|| async {
                axum::Json(serde_json::json!({
                    "status": "ok",
                    "version": env!("CARGO_PKG_VERSION"),
                }))
            }),
        )
        // Web 版登录（免鉴权）：校验访问令牌，签发 Bearer token
        .route("/api/auth/login", post(handlers::auth::login));

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
        .route("/api/hosts/ssh-config", get(handlers::host::ssh_config))
        // 查看主机加密凭证（需操作密码验证）
        .route("/api/hosts/{id}/reveal", post(handlers::host::reveal))
        // SSH 终端 WebSocket
        .route(
            "/api/ssh/terminal/{host_id}",
            get(handlers::terminal::ws_handler),
        )
        // 本地 PTY 终端 WebSocket
        .route("/api/local/terminal", get(handlers::local::ws_handler))
        .route(
            "/api/local/fs/list",
            get(handlers::local::fs_list),
        )
        .route(
            "/api/local/fs/roots",
            get(handlers::local::fs_roots),
        )
        .route(
            "/api/local/fs/download_to_local",
            post(handlers::local::fs_download_to_local),
        )
        .route(
            "/api/local/fs/upload_to_remote",
            post(handlers::local::fs_upload_to_remote),
        )
        .route(
            "/api/local/fs/save_file",
            post(handlers::local::fs_save_file),
        )
        .route(
            "/api/local/fs/trash",
            post(handlers::local::fs_trash),
        )
        .route(
            "/api/local/fs/remove",
            post(handlers::local::fs_remove),
        )
        .route("/api/local/fs/mkdir", post(handlers::local::fs_mkdir))
        .route(
            "/api/local/fs/create_file",
            post(handlers::local::fs_create_file),
        )
        .route("/api/local/fs/rename", post(handlers::local::fs_rename))
        .route("/api/local/fs/copy", post(handlers::local::fs_copy))
        .route("/api/local/fs/move", post(handlers::local::fs_move))
        .route("/api/local/fs/reveal", post(handlers::local::fs_reveal))
        .route("/api/local/fs/open", post(handlers::local::fs_open))
        .route(
            "/api/local/fs/progress",
            post(handlers::local::fs_progress),
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
        .route("/api/ssh/sftp/elevate", post(handlers::sftp::elevate))
        .route("/api/ssh/sftp", get(handlers::sftp::list))
        .route("/api/ssh/sftp/mkdir", post(handlers::sftp::mkdir))
        .route("/api/ssh/sftp/touch", post(handlers::sftp::touch))
        .route(
            "/api/ssh/sftp/remove_file",
            post(handlers::sftp::remove_file),
        )
        .route("/api/ssh/sftp/remove_dir", post(handlers::sftp::remove_dir))
        .route("/api/ssh/sftp/rename", post(handlers::sftp::rename))
        .route("/api/ssh/sftp/move", post(handlers::sftp::move_path))
        .route("/api/ssh/sftp/duplicate", post(handlers::sftp::duplicate))
        .route("/api/ssh/sftp/chmod", post(handlers::sftp::chmod))
        .route("/api/ssh/sftp/du", post(handlers::sftp::du))
        .route("/api/ssh/sftp/compress", post(handlers::sftp::compress))
        .route("/api/ssh/sftp/extract", post(handlers::sftp::extract))
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
        // AI 常用语
        .route(
            "/api/ai-phrases",
            get(handlers::phrase::list)
                .post(handlers::phrase::create)
                .delete(handlers::phrase::clear_all),
        )
        .route(
            "/api/ai-phrases/{id}",
            axum::routing::delete(handlers::phrase::delete),
        )
        // AI sidecar 进程管理（Web 形态替代 spawn/write/kill 等 Tauri commands）
        .route(
            "/api/ai/sidecar/spawn",
            post(handlers::ai_sidecar::spawn),
        )
        .route("/api/ai/sidecar/write", post(handlers::ai_sidecar::write))
        .route("/api/ai/sidecar/kill", post(handlers::ai_sidecar::kill))
        .route("/api/ai/sidecar/{ssid}", get(handlers::ai_sidecar::status))
        .route(
            "/api/ai/sidecar/{ssid}/stream",
            get(handlers::ai_sidecar::stream),
        )
        // AI 配置/路径（Web 形态替代 read_ai_paths / write_ai_paths / detect_claude_path / fetch_models / get_ai_dir）
        .route("/api/ai/dir", get(handlers::webapi::ai_dir))
        .route(
            "/api/ai/paths",
            get(handlers::webapi::ai_paths_get).post(handlers::webapi::ai_paths_set),
        )
        .route(
            "/api/ai/detect-claude-path",
            post(handlers::webapi::ai_detect_claude),
        )
        .route("/api/ai/fetch-models", post(handlers::webapi::ai_fetch_models))
        // Web 端私钥上传（浏览器拿不到本地文件绝对路径，改为传内容落服务端）
        .route("/api/keys/upload", post(handlers::webapi::key_upload))
        // 系统字体（Web 形态替代 list_system_fonts 命令）
        .route("/api/system/fonts", get(handlers::webapi::system_fonts))
        // 壁纸（Web 形态替代 wallpaper 命令组 + asset protocol）
        .route(
            "/api/wallpaper",
            get(handlers::webapi::wallpaper_get)
                .post(handlers::webapi::wallpaper_set)
                .delete(handlers::webapi::wallpaper_clear),
        )
        .route("/api/wallpaper/file", get(handlers::webapi::wallpaper_file))
        // 模板命令
        .route(
            "/api/command-templates",
            get(handlers::template::list).post(handlers::template::create),
        )
        .route(
            "/api/command-templates/{id}",
            axum::routing::put(handlers::template::update).delete(handlers::template::delete),
        )
        // 操作密码管理
        .route(
            "/api/op-password",
            get(handlers::op_password::status)
                .post(handlers::op_password::set)
                .put(handlers::op_password::change)
                .delete(handlers::op_password::clear),
        )
        // 已信任 SSH 主机指纹（TOFU）管理
        .route(
            "/api/known-hosts",
            get(handlers::known_host::list).post(handlers::known_host::trust),
        )
        .route(
            "/api/known-hosts/{id}",
            axum::routing::delete(handlers::known_host::remove),
        )
        // 备份与恢复（S3）
        .route(
            "/api/backup/config",
            get(handlers::backup::get_config).put(handlers::backup::save_config),
        )
        .route("/api/backup/test", post(handlers::backup::test_connection))
        .route("/api/backup/create", post(handlers::backup::create_backup))
        .route("/api/backup/export", post(handlers::backup::export_backup))
        .route("/api/backup/list", get(handlers::backup::list_backups))
        .route(
            "/api/backup/restore",
            post(handlers::backup::restore_backup),
        )
        .route("/api/backup/import", post(handlers::backup::import_backup))
        .route("/api/backup/delete", post(handlers::backup::delete_backup))
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
