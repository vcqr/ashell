/// 枚举系统已安装字体（去重 + 字典序）。
/// 失败或为空时返回空数组，前端会回退到内置预设。
/// 桌面端用 font-kit 枚举；Web 形态浏览器用客户端字体渲染终端，
/// 服务端字体列表无意义，返回空数组（同时免去 musl 交叉编译的
/// fontconfig/freetype 系统依赖）。
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn list_system_fonts() -> Vec<String> {
    #[cfg(not(feature = "desktop"))]
    return Vec::new();

    #[cfg(feature = "desktop")]
    {
    use font_kit::source::SystemSource;
    use std::collections::BTreeSet;

    let source = SystemSource::new();
    let families = match source.all_families() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("list_system_fonts: {e}");
            return Vec::new();
        }
    };

    let mut set: BTreeSet<String> = BTreeSet::new();
    for name in families {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        // 过滤掉以 '.' 开头的隐藏字体族（macOS 系统字体）
        if trimmed.starts_with('.') {
            continue;
        }
        set.insert(trimmed.to_string());
    }
    set.into_iter().collect()
    }
}