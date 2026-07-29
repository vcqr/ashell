pub mod forward;
pub mod group;
pub mod host;
pub mod icons;
pub mod local_pty;
pub mod serial;
pub mod sftp;
pub mod ssh;
pub mod ssh_config;
pub mod sysinfo;
pub mod telnet;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::models::DbPool;

/// 注入到 axum 中的全局状态
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub config: Arc<AppConfig>,
}
