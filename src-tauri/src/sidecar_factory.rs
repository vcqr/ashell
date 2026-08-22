//! Sidecar 二进制查找工厂。
//!
//! 优先使用统一二进制 sidecar-ai（内嵌全部引擎，引擎类型经启动参数下发），
//! 找不到时回退旧版按类型拆分的二进制（app-cc / app-pi，兼容已手动放置的用户）。
//! 查找优先级：
//! 1. .env 中 SIDECAR_APP_PATH 配置的路径（所有类型共用）
//! 2. 统一二进制 app-ai：~/.ashell/bin/ → 可执行文件同目录
//! 3. 旧版二进制 app-cc / app-pi：~/.ashell/bin/ → 可执行文件同目录

use std::path::PathBuf;

/// Sidecar 类型常量（引擎标识，同时用于旧版二进制命名）
pub const TYPE_CLAUDE: &str = "claude";
pub const TYPE_PI: &str = "pi";

/// 统一 sidecar 二进制名（sidecar-ai 编译产物，内嵌全部引擎）
fn unified_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "app-ai.exe"
    } else {
        "app-ai"
    }
}

/// 旧版按类型拆分的二进制名（向后兼容）
fn legacy_binary_name(sidecar_type: &str) -> String {
    let stem = if sidecar_type == TYPE_PI { "app-pi" } else { "app-cc" };
    if cfg!(target_os = "windows") {
        format!("{stem}.exe")
    } else {
        stem.to_string()
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
/// 2. 统一二进制 app-ai（引擎由启动参数选择）
/// 3. 旧版按类型拆分的 app-cc / app-pi
pub fn find_sidecar_binary(sidecar_type: &str) -> Result<PathBuf, String> {
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

    let legacy = legacy_binary_name(sidecar_type);
    if let Some(path) = locate_binary(&legacy) {
        tracing::warn!(
            "[SIDECAR_FACTORY] Unified binary '{}' not found, falling back to legacy '{}'",
            unified,
            legacy
        );
        return Ok(path);
    }

    Err(format!(
        "Sidecar binary not found. Searched '{unified}' and '{legacy}' in ~/.ashell/bin/ and the executable directory"
    ))
}
