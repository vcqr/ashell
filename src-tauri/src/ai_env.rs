//! AI sidecar 工作目录下 `.env` 配置文件的读写。
//!
//! 设计要点：
//! - 仅前端"AI 模型设置"弹窗感兴趣的 key 暴露给前端（见 [`AiModelConfig`]）。
//! - 写入时**保留**所有未识别的行（注释、其它 sidecar 自己维护的 key），
//!   只对感兴趣的 key 做 in-place 替换/追加，避免覆盖外部修改。
//! - 解析支持 `KEY=VALUE` / `KEY="VALUE"` / `KEY='VALUE'` 三种形态，
//!   写入对含特殊字符的 value 自动用双引号包裹并转义 `\` 与 `"`。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config;

/// sidecar 实际读取的 anthropic 兼容字段
const KEY_BASE_URL: &str = "ANTHROPIC_BASE_URL";
const KEY_AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";
const KEY_MODEL: &str = "ANTHROPIC_MODEL";
/// 仅前端用：候选模型 id 列表（逗号分隔），sidecar 不读
const KEY_MODEL_IDS: &str = "PROVIDE_MODEL_IDS";
/// sidecar app 二进制路径（Rust 端 find_sidecar_binary 读取）
const KEY_SIDECAR_APP_PATH: &str = "SIDECAR_APP_PATH";
/// Claude Code CLI 路径（sidecar-cc 读取）
const KEY_CLAUDE_CLI_PATH: &str = "CLAUDE_CLI_PATH";
/// Sidecar 类型（"claude" / "pi"），决定 Rust 端查找哪个二进制
const KEY_SIDECAR_TYPE: &str = "SIDECAR_TYPE";

/// Pi sidecar 专属配置
const KEY_PI_PROVIDER: &str = "PI_PROVIDER";
const KEY_PI_MODEL: &str = "PI_MODEL";
const KEY_PI_BASE_URL: &str = "PI_BASE_URL";
const KEY_PI_API_KEY: &str = "PI_API_KEY";
const KEY_PI_API: &str = "PI_API";
/// Pi sidecar: 候选模型 ID 列表（逗号分隔）
const KEY_PI_MODEL_IDS: &str = "PI_MODEL_IDS";
/// Pi sidecar: thinking level
const KEY_PI_THINKING_LEVEL: &str = "PI_THINKING_LEVEL";

/// 前端 AI 设置弹窗的 schema
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelConfig {
    pub url: String,
    pub key: String,
    /// 逗号分隔的候选模型 id 列表
    pub model_ids: String,
    pub active_model_id: String,
    /// sidecar 类型："claude" / "pi"，空字符串视为 "claude"
    pub sidecar_type: String,
    /// Pi sidecar: provider 名称（如 "custom"、"openai"）
    pub pi_provider: String,
    /// Pi sidecar: 模型 ID
    pub pi_model: String,
    /// Pi sidecar: 候选模型 ID 列表（逗号分隔）
    pub pi_model_ids: String,
    /// Pi sidecar: API base URL
    pub pi_base_url: String,
    /// Pi sidecar: API key
    pub pi_api_key: String,
    /// Pi sidecar: API 类型（如 "openai-completions"、"anthropic-messages"）
    pub pi_api: String,
    /// Pi sidecar: thinking level（"off" / "minimal" / "low" / "medium" / "high" / "xhigh" / "max"）
    pub pi_thinking_level: String,
}

fn env_path() -> Result<PathBuf, String> {
    let dir = config::ai_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(".env"))
}

/// 读取 .env 并提取前端感兴趣的字段。
///
/// 文件不存在时返回全空配置（不创建文件）。
#[tauri::command]
pub fn read_ai_env() -> Result<AiModelConfig, String> {
    read_env_config()
}

/// 读取 .env 配置（pub(crate) 供 service 层迁移逻辑复用）
pub(crate) fn read_env_config() -> Result<AiModelConfig, String> {
    let path = env_path()?;
    if !path.exists() {
        return Ok(AiModelConfig::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 {:?} 失败: {e}", path))?;
    let map = parse_env(&raw);

    Ok(AiModelConfig {
        url: map.get(KEY_BASE_URL).cloned().unwrap_or_default(),
        key: map.get(KEY_AUTH_TOKEN).cloned().unwrap_or_default(),
        model_ids: map.get(KEY_MODEL_IDS).cloned().unwrap_or_default(),
        active_model_id: map.get(KEY_MODEL).cloned().unwrap_or_default(),
        sidecar_type: map.get(KEY_SIDECAR_TYPE).cloned().unwrap_or_default(),
        pi_provider: map.get(KEY_PI_PROVIDER).cloned().unwrap_or_default(),
        pi_model: map.get(KEY_PI_MODEL).cloned().unwrap_or_default(),
        pi_model_ids: map.get(KEY_PI_MODEL_IDS).cloned().unwrap_or_default(),
        pi_base_url: map.get(KEY_PI_BASE_URL).cloned().unwrap_or_default(),
        pi_api_key: map.get(KEY_PI_API_KEY).cloned().unwrap_or_default(),
        pi_api: map.get(KEY_PI_API).cloned().unwrap_or_default(),
        pi_thinking_level: map.get(KEY_PI_THINKING_LEVEL).cloned().unwrap_or_default(),
    })
}

/// 写入 .env：仅替换/插入感兴趣的 key，其它行原样保留。
///
/// 空字符串视为"清空"--会移除对应行。
#[tauri::command]
pub fn write_ai_env(config: AiModelConfig) -> Result<(), String> {
    write_env_config(config)
}

/// 写入 .env 核心逻辑（pub(crate) 供 service 层 activate 复用）
pub(crate) fn write_env_config(config: AiModelConfig) -> Result<(), String> {
    let path = env_path()?;
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("读取 {:?} 失败: {e}", path))?
    } else {
        String::new()
    };

    let mut updates: BTreeMap<&str, String> = BTreeMap::new();
    updates.insert(KEY_BASE_URL, config.url.trim().to_string());
    updates.insert(KEY_AUTH_TOKEN, config.key.trim().to_string());
    updates.insert(KEY_MODEL, config.active_model_id.trim().to_string());
    updates.insert(KEY_MODEL_IDS, config.model_ids.trim().to_string());
    updates.insert(KEY_SIDECAR_TYPE, config.sidecar_type.trim().to_string());
    updates.insert(KEY_PI_PROVIDER, config.pi_provider.trim().to_string());
    updates.insert(KEY_PI_MODEL, config.pi_model.trim().to_string());
    updates.insert(KEY_PI_MODEL_IDS, config.pi_model_ids.trim().to_string());
    updates.insert(KEY_PI_BASE_URL, config.pi_base_url.trim().to_string());
    updates.insert(KEY_PI_API_KEY, config.pi_api_key.trim().to_string());
    updates.insert(KEY_PI_API, config.pi_api.trim().to_string());
    updates.insert(KEY_PI_THINKING_LEVEL, config.pi_thinking_level.trim().to_string());

    let merged = merge_env(&existing, &updates);

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录 {:?} 失败: {e}", parent))?;
        }
    }
    fs::write(&path, merged).map_err(|e| format!("写入 {:?} 失败: {e}", path))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 把 .env 文本解析成 KV map（仅暴露 pub(crate) 方便测试）。
fn parse_env(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        if let Some((k, v)) = parse_line(line) {
            map.insert(k, v);
        }
    }
    map
}

/// 解析单行；返回 None 表示该行不是有效的 KV（注释、空行、不合规）。
fn parse_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let eq = trimmed.find('=')?;
    let key = trimmed[..eq].trim();
    if key.is_empty() || !is_valid_key(key) {
        return None;
    }
    let raw_value = trimmed[eq + 1..].trim_start();
    let value = unquote_value(raw_value);
    Some((key.to_string(), value))
}

fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 把 `"..."` / `'...'` / 裸值 还原为字符串内容，并裁掉行尾注释。
fn unquote_value(raw: &str) -> String {
    let raw = raw.trim_start();
    if raw.is_empty() {
        return String::new();
    }
    let bytes = raw.as_bytes();
    if bytes[0] == b'"' {
        // 双引号：支持 \\ \" \n \r \t 转义
        let mut out = String::new();
        let mut iter = raw[1..].chars();
        while let Some(c) = iter.next() {
            match c {
                '"' => return out,
                '\\' => match iter.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => break,
                },
                _ => out.push(c),
            }
        }
        out
    } else if bytes[0] == b'\'' {
        // 单引号：原样取至下一个单引号
        if let Some(end) = raw[1..].find('\'') {
            return raw[1..1 + end].to_string();
        }
        raw[1..].to_string()
    } else {
        // 裸值：截断 ` #`（前面有空白的注释）和行尾空白
        let bare = match raw.find(" #") {
            Some(idx) => &raw[..idx],
            None => raw,
        };
        bare.trim_end().to_string()
    }
}

/// 把 value 序列化成 .env 一行的右侧表示。
fn encode_value(v: &str) -> String {
    let needs_quote = v.is_empty()
        || v.chars()
            .any(|c| matches!(c, ' ' | '\t' | '"' | '\'' | '#' | '\n' | '\r' | '\\' | '='));
    if !needs_quote {
        return v.to_string();
    }
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// 把 `updates` 合并进现有 .env 文本：
/// - 对于已存在的 key，原地替换那一行（保留缩进）；空字符串则删除该行
/// - 对于不存在的 key，且新值非空，追加到末尾
/// - 其它行（注释、未识别 key）一律原样保留
fn merge_env(existing: &str, updates: &BTreeMap<&str, String>) -> String {
    let mut handled: BTreeMap<&str, bool> = updates.keys().map(|k| (*k, false)).collect();
    let mut out_lines: Vec<String> = Vec::new();

    for line in existing.lines() {
        let parsed = parse_line(line).map(|(k, _)| k);
        match parsed {
            Some(key) => {
                let hit = updates
                    .iter()
                    .find(|(k, _)| **k == key.as_str())
                    .map(|(k, v)| (*k, v.clone()));
                if let Some((target_key, new_value)) = hit {
                    handled.insert(target_key, true);
                    if new_value.is_empty() {
                        // 跳过该行 = 删除
                        continue;
                    }
                    out_lines.push(format!("{}={}", target_key, encode_value(&new_value)));
                } else {
                    out_lines.push(line.to_string());
                }
            }
            None => out_lines.push(line.to_string()),
        }
    }

    // 末尾追加新引入的 key
    for (k, done) in handled.iter() {
        if *done {
            continue;
        }
        let v = updates.get(k).map(String::as_str).unwrap_or("");
        if v.is_empty() {
            continue;
        }
        out_lines.push(format!("{}={}", k, encode_value(v)));
    }

    let mut text = out_lines.join("\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

// ── AI 路径配置 ──

/// 前端 AI 路径设置的 schema
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPathsConfig {
    /// sidecar app 二进制路径（Rust 端 find_sidecar_binary 读取）
    pub sidecar_path: String,
    /// Claude Code CLI 路径（sidecar-cc 读取）
    pub claude_path: String,
}

/// 读取 .env 中的路径配置。
#[tauri::command]
pub fn read_ai_paths() -> Result<AiPathsConfig, String> {
    let path = env_path()?;
    if !path.exists() {
        return Ok(AiPathsConfig::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 {:?} 失败: {e}", path))?;
    let map = parse_env(&raw);

    Ok(AiPathsConfig {
        sidecar_path: map.get(KEY_SIDECAR_APP_PATH).cloned().unwrap_or_default(),
        claude_path: map.get(KEY_CLAUDE_CLI_PATH).cloned().unwrap_or_default(),
    })
}

/// 写入路径配置到 .env（仅替换这两个 key，其它行原样保留）。
#[tauri::command]
pub fn write_ai_paths(config: AiPathsConfig) -> Result<(), String> {
    let path = env_path()?;
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("读取 {:?} 失败: {e}", path))?
    } else {
        String::new()
    };

    let mut updates: BTreeMap<&str, String> = BTreeMap::new();
    updates.insert(
        KEY_SIDECAR_APP_PATH,
        normalize_path(config.sidecar_path.trim()),
    );
    updates.insert(KEY_CLAUDE_CLI_PATH, normalize_path(config.claude_path.trim()));

    let merged = merge_env(&existing, &updates);

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录 {:?} 失败: {e}", parent))?;
        }
    }
    fs::write(&path, merged).map_err(|e| format!("写入 {:?} 失败: {e}", path))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 把 Windows 路径里的反斜杠统一成正斜杠。
///
/// .env 中的反斜杠会被 sidecar（Node dotenv）当作转义字符解析，
/// 例如 `C:\Users\new` 里的 `\U` / `\n` 会被错误转义导致路径报错，
/// 因此写入前统一替换为 `/`（Windows 与 Node 都能正确识别正斜杠）。
pub(crate) fn normalize_path(v: &str) -> String {
    v.replace('\\', "/")
}

/// 检测系统中是否安装了 `claude` 命令，返回其完整路径。
#[tauri::command]
pub fn detect_claude_path() -> Option<String> {
    let cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let output = std::process::Command::new(cmd)
        .arg("claude")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout.lines().next()?.trim();
    if path.is_empty() {
        None
    } else {
        Some(normalize_path(path))
    }
}

/// 从供应商 API 获取可用模型列表。
///
/// api_type 决定请求端点和认证头：
/// - "anthropic": GET {base_url}/v1/models, header x-api-key + anthropic-version
/// - "openai":    GET {base_url}/models, header Authorization: Bearer
/// - "google":    GET https://generativelanguage.googleapis.com/v1beta/models?key=
#[tauri::command]
pub async fn fetch_models(
    base_url: String,
    api_key: String,
    api_type: String,
) -> Result<Vec<String>, String> {
    if base_url.trim().is_empty() || api_key.trim().is_empty() {
        return Err("base_url 和 api_key 不能为空".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client 构建失败: {e}"))?;

    let models: Vec<String> = match api_type.as_str() {
        "anthropic" => {
            let url = build_url(&base_url, "/v1/models");
            let resp = client
                .get(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| format!("请求失败: {e}"))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("HTTP {status}: {body}"));
            }
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("解析响应失败: {e}"))?;
            json["data"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m["id"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        }
        "openai" => {
            let url = build_url(&base_url, "/models");
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .send()
                .await
                .map_err(|e| format!("请求失败: {e}"))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("HTTP {status}: {body}"));
            }
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("解析响应失败: {e}"))?;
            json["data"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m["id"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        }
        "google" => {
            let url = "https://generativelanguage.googleapis.com/v1beta/models";
            let resp = client
                .get(url)
                .query(&[("key", &api_key)])
                .send()
                .await
                .map_err(|e| format!("请求失败: {e}"))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("HTTP {status}: {body}"));
            }
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("解析响应失败: {e}"))?;
            json["models"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            m["name"]
                                .as_str()
                                .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => return Err(format!("不支持的 api_type: {api_type}")),
    };

    Ok(models)
}

/// 拼接 models 端点 URL，避免 base_url 已含 /v1 时重复
fn build_url(base_url: &str, suffix: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');
    if suffix.starts_with("v1/") && base.ends_with("/v1") {
        format!("{}/{}", base, &suffix[3..])
    } else {
        format!("{base}/{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let m = parse_env("FOO=bar\nBAZ=\"qux quux\"\n# comment\nEMPTY=\n");
        assert_eq!(m.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(m.get("BAZ"), Some(&"qux quux".to_string()));
        assert_eq!(m.get("EMPTY"), Some(&"".to_string()));
        assert!(m.get("comment").is_none());
    }

    #[test]
    fn quoted_value_with_escape() {
        let m = parse_env(r#"K="line1\nline2""#);
        assert_eq!(m.get("K"), Some(&"line1\nline2".to_string()));
    }

    #[test]
    fn merge_preserves_comments_and_unknown() {
        let original = "# header\nOTHER=keep\nANTHROPIC_BASE_URL=old\n";
        let mut updates: BTreeMap<&str, String> = BTreeMap::new();
        updates.insert("ANTHROPIC_BASE_URL", "https://api.example.com".into());
        updates.insert("ANTHROPIC_AUTH_TOKEN", "sk-xyz".into());
        updates.insert("ANTHROPIC_MODEL", "".into());
        updates.insert("PROVIDE_MODEL_IDS", "".into());
        updates.insert("SIDECAR_TYPE", "".into());
        updates.insert("PI_PROVIDER", "".into());
        updates.insert("PI_MODEL", "".into());
        updates.insert("PI_MODEL_IDS", "".into());
        updates.insert("PI_BASE_URL", "".into());
        updates.insert("PI_API_KEY", "".into());
        updates.insert("PI_API", "".into());
        updates.insert("PI_THINKING_LEVEL", "".into());

        let merged = merge_env(original, &updates);
        assert!(merged.contains("# header"));
        assert!(merged.contains("OTHER=keep"));
        assert!(merged.contains("ANTHROPIC_BASE_URL=https://api.example.com"));
        assert!(merged.contains("ANTHROPIC_AUTH_TOKEN=sk-xyz"));
        assert!(!merged.contains("ANTHROPIC_MODEL"));
    }

    #[test]
    fn encode_handles_special_chars() {
        assert_eq!(encode_value("plain"), "plain");
        assert_eq!(encode_value("with space"), "\"with space\"");
        assert_eq!(encode_value("a\"b"), "\"a\\\"b\"");
    }
}
