//! Sidecar 二进制查找工厂。
//!
//! 统一使用 daemon 形态的 app-ai 二进制（`--serve`，内嵌全部引擎，引擎类型
//! 经 create 帧下发）。查找优先级：
//! 1. .env 中 SIDECAR_APP_PATH 配置的路径
//! 2. app-ai：~/.ashell/bin/ → 可执行文件同目录
//!
//! 注：旧版按类型拆分的 app-cc / app-pi 二进制不支持帧协议，已不再回退。

use std::path::PathBuf;

/// Sidecar 类型常量（引擎标识，经 create 帧下发；"pi" 由前端/配置直接传字符串）
pub const TYPE_CLAUDE: &str = "claude";

/// 统一 sidecar 二进制名（sidecar-ai 编译产物，daemon 形态）
fn unified_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "app-ai.exe"
    } else {
        "app-ai"
    }
}

/// 在默认位置查找指定二进制：~/.ashell/bin/（用户手动放置的默认位置）
/// → 可执行文件同目录（生产模式回退）
fn locate_binary(name: &str) -> Option<PathBuf> {
    if let Ok(ashell_dir) = crate::config::app_dir() {
        let bin_path = ashell_dir.join("bin").join(name);
        if bin_path.exists() {
            #[cfg(unix)]
            ensure_executable(&bin_path);
            tracing::info!("[SIDECAR_FACTORY] Found {} binary at: {:?}", name, bin_path);
            return Some(bin_path);
        }
    }

    let current_exe = std::env::current_exe().ok()?;
    let prod_path = current_exe.parent()?.join(name);
    if prod_path.exists() {
        tracing::info!("[SIDECAR_FACTORY] Found {} binary at: {:?}", name, prod_path);
        return Some(prod_path);
    }

    None
}

#[cfg(unix)]
fn ensure_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = path.metadata() {
        let perms = metadata.permissions();
        if perms.mode() & 0o111 == 0 {
            let _ = std::fs::set_permissions(path, PermissionsExt::from_mode(0o755));
        }
    }
}

/// 查找 sidecar 二进制文件路径。
///
/// 优先级：
/// 1. 用户在 .env 中配置的 SIDECAR_APP_PATH（指向具体二进制）
/// 2. 统一 daemon 二进制 app-ai（引擎由 create 帧选择）
pub fn find_sidecar_binary(_sidecar_type: &str) -> Result<PathBuf, String> {
    // 0. 用户显式配置的路径（~/.ashell/ai/.env -> SIDECAR_APP_PATH）
    if let Ok(paths) = crate::ai_env::read_ai_paths() {
        if !paths.sidecar_path.is_empty() {
            let p = PathBuf::from(&paths.sidecar_path);
            if p.exists() {
                tracing::info!("[SIDECAR_FACTORY] Using configured sidecar path: {:?}", p);
                return Ok(p);
            }
            tracing::warn!(
                "[SIDECAR_FACTORY] Configured sidecar path does not exist: {:?}",
                p
            );
        }
    }

    let unified = unified_binary_name();
    if let Some(path) = locate_binary(unified) {
        return Ok(path);
    }

    Err(format!(
        "Sidecar binary not found. Searched '{unified}' in ~/.ashell/bin/ and the executable directory"
    ))
}
