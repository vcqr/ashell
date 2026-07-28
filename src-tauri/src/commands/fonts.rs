/// 枚举系统已安装字体（去重 + 字典序）。
/// 失败或为空时返回空数组，前端会回退到内置预设。
#[tauri::command]
pub fn list_system_fonts() -> Vec<String> {
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