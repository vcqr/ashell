//! Windows 资源管理器右键菜单「在 Ashell 中打开」的注册表开关（HKCU，无需管理员）。
//! 三组键：Directory（右键文件夹）、Directory\Background（文件夹空白处）、Drive（驱动器），
//! command 值用 `"%V"`：两种语境下 Explorer 都会替换为目标目录。
//! NSIS 安装器 hook（nsis/installer-hooks.nsh）装/卸时写/删同一组键；
//! 本模块供设置页开关在运行期读写——便携版（zip）没有安装器，这里是唯一入口。
//! 非 Windows 平台命令仍注册，恒返回未安装/不支持，前端按平台隐藏入口。

#[cfg(windows)]
use windows_registry::CURRENT_USER;

/// 三组注册表键（HKCU 下，键名 AshellOpen 同时用于安装器写入，保持一致）
#[cfg(windows)]
const KEY_PATHS: &[&str] = &[
    r"Software\Classes\Directory\shell\AshellOpen",
    r"Software\Classes\Directory\Background\shell\AshellOpen",
    r"Software\Classes\Drive\shell\AshellOpen",
];

/// 菜单文本跟随托盘侧固化的界面语言（tray_apply_locale 已归一化为 zh-CN/en）
#[cfg(windows)]
fn menu_text() -> &'static str {
    if crate::tray::settings().locale.starts_with("zh") {
        "在 Ashell 中打开"
    } else {
        "Open in Ashell"
    }
}

/// 右键菜单是否已注册（任一键存在即视为已开，覆盖安装器写入的情况）
#[tauri::command]
pub fn context_menu_installed() -> Result<bool, String> {
    installed_impl()
}

/// 开/关右键菜单
#[tauri::command]
pub fn context_menu_set(enable: bool) -> Result<(), String> {
    set_impl(enable)
}

#[cfg(windows)]
fn installed_impl() -> Result<bool, String> {
    Ok(KEY_PATHS
        .iter()
        .any(|path| windows_registry::CURRENT_USER.open(path).is_ok()))
}

#[cfg(windows)]
fn set_impl(enable: bool) -> Result<(), String> {
    if enable {
        install()
    } else {
        uninstall()
    }
}

#[cfg(windows)]
fn install() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("current_exe: {e}"))?
        .to_string_lossy()
        .into_owned();
    let command = format!("\"{exe}\" \"%V\"");
    for path in KEY_PATHS {
        let key = CURRENT_USER
            .create(path)
            .map_err(|e| format!("create {path}: {e}"))?;
        key.set_string("", menu_text())
            .map_err(|e| format!("set name {path}: {e}"))?;
        key.set_string("Icon", &exe)
            .map_err(|e| format!("set icon {path}: {e}"))?;
        key.create("command")
            .map_err(|e| format!("create command {path}: {e}"))?
            .set_string("", &command)
            .map_err(|e| format!("set command {path}: {e}"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn uninstall() -> Result<(), String> {
    for path in KEY_PATHS {
        // 不存在也视为成功（幂等；NSIS 卸载可能已删过）
        let _ = CURRENT_USER.remove_tree(path);
    }
    Ok(())
}

#[cfg(not(windows))]
fn installed_impl() -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(windows))]
fn set_impl(_enable: bool) -> Result<(), String> {
    Err("only supported on Windows".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// 写/读/删往返：临时占用真实键路径（毫秒级自清理），验证
    /// windows-registry 的 set_string 默认值、command 子键与 remove_tree 行为。
    /// 键已存在（用户已在用此功能）时跳过，避免动真实状态。
    #[test]
    fn roundtrip_install_uninstall() {
        if installed_impl().unwrap() {
            eprintln!("skip: AshellOpen keys already present, skip roundtrip");
            return;
        }

        install().unwrap();
        assert!(installed_impl().unwrap());
        // 默认值（菜单文本）与 command 都要能读回
        let key = CURRENT_USER.open(KEY_PATHS[0]).unwrap();
        assert_eq!(key.get_string("").unwrap(), menu_text());
        let cmd = key.open("command").unwrap();
        let command = cmd.get_string("").unwrap();
        assert!(command.starts_with('"') && command.ends_with("\" \"%V\""), "command 格式: {command}");

        uninstall().unwrap();
        assert!(!installed_impl().unwrap(), "卸载后键应不存在");
    }
}
