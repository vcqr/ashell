//! Sidecar 二进制查找工厂。
//!
//! 根据 sidecar 类型（"claude" / "pi"）查找对应的编译后二进制文件。
//! 查找优先级：
//! 1. .env 中 SIDECAR_APP_PATH 配置的路径（所有类型共用）
//! 2. ~/.ashell/bin/ 下按类型命名的二进制（app / app-pi）
//! 3. 可执行文件同目录（生产模式）

use std::path::PathBuf;

/// Sidecar 类型常量
pub const TYPE_CLAUDE: &str = "claude";
pub const TYPE_PI: &str = "pi";

/// 根据 sidecar 类型返回二进制文件名（不含路径）。
///
/// - "pi" -> "app-pi" (Windows: "app-pi.exe")
/// - 其它（含 "claude"、空值）-> "app" (Windows: "app.exe")
fn binary_name(sidecar_type: &str) -> &str {
    if sidecar_type == TYPE_PI {
        if cfg!(target_os = "windows") {
            "app-pi.exe"
        } else {
            "app-pi"
        }
    } else {
        if cfg!(target_os = "windows") {
            "app-cc.exe"
        } else {
            "app-cc"
        }
    }
}

/// 查找 sidecar 二进制文件路径。
///
/// 优先级：
/// 1. 用户在 .env 中配置的 SIDECAR_APP_PATH（所有类型共用，指向具体二进制）
/// 2. ~/.ashell/bin/ 下按类型命名的二进制
/// 3. 可执行文件同目录（生产模式回退）
pub fn find_sidecar_binary(sidecar_type: &str) -> Result<PathBuf, String> {
    // 0. 用户在设置中配置的路径（~/.ashell/ai/.env -> SIDECAR_APP_PATH）
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

    let binary_name = binary_name(sidecar_type);

    // 1. ~/.ashell/bin/ 目录（用户手动放置的默认位置）
    if let Ok(ashell_dir) = crate::config::app_dir() {
        let bin_dir = ashell_dir.join("bin");
        let bin_path = bin_dir.join(binary_name);
        if bin_path.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = bin_path.metadata() {
                    let perms = metadata.permissions();
                    if perms.mode() & 0o111 == 0 {
                        let _ =
                            std::fs::set_permissions(&bin_path, PermissionsExt::from_mode(0o755));
                    }
                }
            }
            tracing::info!(
                "[SIDECAR_FACTORY] Found {} binary at: {:?}",
                sidecar_type,
                bin_path
            );
            return Ok(bin_path);
        }
    }

    // 2. 回退：在可执行文件同目录查找（生产模式）
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = current_exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let prod_path = exe_dir.join(binary_name);
    if prod_path.exists() {
        tracing::info!(
            "[SIDECAR_FACTORY] Found {} binary at: {:?}",
            sidecar_type,
            prod_path
        );
        return Ok(prod_path);
    }

    Err(format!(
        "Sidecar binary '{}' not found. Searched: ~/.ashell/bin/ and {:?}",
        binary_name, prod_path
    ))
}
