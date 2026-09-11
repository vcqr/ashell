//! AShell Web Server —— AShell 的 Web 服务器形态。
//!
//! 复用桌面版同一套核心业务层（SSH/SFTP/本地终端/Telnet/串口/AI sidecar 等
//! 全部经 axum REST/WS 暴露），以固定端口对外提供服务，并可选托管前端
//! 静态资源（dist/）实现同源部署。
//!
//! 用法：
//!   ashell-server [--bind 127.0.0.1:8090] [--token <token>] [--dist ./dist]
//!                 [--force-key-file]
//!
//! 访问令牌优先级：--token > ASHELL_WEB_TOKEN > ~/.ashell/web-token（首次自动生成）。
//! 数据目录与桌面版相同（~/.ashell/），请勿与桌面版同时运行以免 SQLite 争用。

use std::path::PathBuf;

use ashell_lib::server::{self, ServerOptions};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = ServerOptions::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" if i + 1 < args.len() => {
                opts.bind = args[i + 1].clone();
                i += 2;
            }
            "--token" if i + 1 < args.len() => {
                opts.token = Some(args[i + 1].clone());
                i += 2;
            }
            "--dist" if i + 1 < args.len() => {
                opts.dist = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--force-key-file" => {
                opts.force_key_file = true;
                i += 1;
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            other => {
                eprintln!("unknown argument: {other}\n");
                print_help();
                std::process::exit(2);
            }
        }
    }

    init_logging();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    if let Err(e) = rt.block_on(server::run(opts)) {
        eprintln!("ashell-server: fatal: {e:#}");
        std::process::exit(1);
    }
}

fn init_logging() {
    // tracing-subscriber 处理 tracing 事件；env_logger 处理 log 宏输出（替代 tracing-log 桥接）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(false)
        .init();
    env_logger::Builder::from_env(env_logger::Env::default().filter_or("RUST_LOG", "info"))
        .write_style(env_logger::WriteStyle::Never)
        .init();
}

fn print_help() {
    println!(
        "AShell Web Server v{}

USAGE:
    ashell-server [OPTIONS]

OPTIONS:
    --bind <ADDR>    监听地址（默认 127.0.0.1:8090；对外暴露用 0.0.0.0:8090）
    --token <TOKEN>  访问令牌（默认读 ASHELL_WEB_TOKEN，或 ~/.ashell/web-token，无则生成）
    --dist <DIR>     前端构建产物目录（托管后同源部署，前端免配置 API 地址）
    --force-key-file 跳过 OS 钥匙串，强制使用 ~/.ashell/secret.key 文件密钥
                     （无头部署/容器环境推荐；注意与桌面版钥匙串密钥不互通）
    -h, --help       显示帮助
",
        env!("CARGO_PKG_VERSION")
    );
}
