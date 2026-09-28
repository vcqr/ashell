//! 网络代理设置：跟随系统 / 直连 / 自定义。
//!
//! 设置持久化在 `~/.ashell/proxy.json`（后端权威）。生效点：
//! - `ai_env::fetch_models` 拉取模型列表的 reqwest 客户端
//! - S3 备份客户端（rust-s3）
//! - AI sidecar 子进程的 HTTP(S)_PROXY 环境变量（Claude Code 等读环境变量）
//!
//! SSH 终端连接是原始 TCP，不经过 HTTP 代理，不受影响。

use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::config;

/// 代理模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// 跟随系统代理（默认）
    System,
    /// 自定义代理地址
    Custom,
    /// 强制直连
    Direct,
}

/// 代理设置（~/.ashell/proxy.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxySettings {
    /// 代理模式
    pub mode: ProxyMode,
    /// custom 模式下的代理地址，如 http://127.0.0.1:7890 / socks5://127.0.0.1:7890
    pub url: String,
    /// custom 模式下不走代理的主机规则，逗号分隔，如 localhost,.example.com
    pub no_proxy: String,
}

impl Default for ProxySettings {
    fn default() -> Self {
        // 默认跟随系统代理，与未引入该设置前的 reqwest 默认行为一致
        Self {
            mode: ProxyMode::System,
            url: String::new(),
            no_proxy: String::new(),
        }
    }
}

static SETTINGS: RwLock<Option<ProxySettings>> = RwLock::new(None);

fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(config::app_dir()?.join("proxy.json"))
}

/// 读取代理设置（带缓存；文件缺失/损坏走默认值）
pub fn settings() -> ProxySettings {
    if let Ok(guard) = SETTINGS.read() {
        if let Some(s) = guard.as_ref() {
            return s.clone();
        }
    }
    let loaded = read_from_disk();
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(loaded.clone());
    }
    loaded
}

fn read_from_disk() -> ProxySettings {
    let path = match settings_path() {
        Ok(p) => p,
        Err(_) => return ProxySettings::default(),
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn persist(s: &ProxySettings) -> Result<(), String> {
    let path = settings_path().map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    if let Ok(mut guard) = SETTINGS.write() {
        *guard = Some(s.clone());
    }
    Ok(())
}

/// 一次请求的代理决策，各生效点据此决定是否走代理
#[derive(Debug, Clone)]
pub enum EffectiveProxy {
    /// 强制直连：显式禁用一切代理（含环境变量回退）
    Direct,
    /// 经代理访问
    Proxy {
        /// 代理地址，如 http://127.0.0.1:7890
        url: String,
        /// 不走代理的主机规则（逗号分隔，已并入本地回环）
        no_proxy: String,
    },
    /// 无显式决策：保持 reqwest 默认行为（跟随环境变量）
    Auto,
}

/// 本地回环始终直连：界面流量与本机 axum 服务都在回环上
const LOCAL_NO_PROXY: &str = "localhost,127.0.0.1,::1";

/// 解析当前的代理决策。
///
/// - system：读 OS 级代理设置（Windows 注册表 / macOS / Linux 桌面），读不到
///   时保持 Auto（reqwest 默认仍会读环境变量）
/// - custom：取用户配置，地址留空视为直连
/// - direct：强制直连
pub fn effective() -> EffectiveProxy {
    let s = settings();
    match s.mode {
        ProxyMode::Direct => EffectiveProxy::Direct,
        ProxyMode::System => match detect_system_proxy() {
            Some(url) => EffectiveProxy::Proxy {
                url,
                no_proxy: merge_rules(&system_bypass_rules()),
            },
            None => EffectiveProxy::Auto,
        },
        ProxyMode::Custom => {
            let url = s.url.trim().to_string();
            if url.is_empty() {
                EffectiveProxy::Direct
            } else {
                EffectiveProxy::Proxy {
                    url,
                    no_proxy: merge_rules(&s.no_proxy),
                }
            }
        }
    }
}

/// 探测系统代理：先读 OS 级设置（Windows 注册表 / macOS / Linux 桌面），
/// 未启用或读取失败时回退 HTTP(S)_PROXY / ALL_PROXY 环境变量。
fn detect_system_proxy() -> Option<String> {
    if let Ok(p) = sysproxy::Sysproxy::get_system_proxy() {
        if p.enable && !p.host.trim().is_empty() && p.port > 0 {
            return Some(format!("http://{}:{}", p.host.trim(), p.port));
        }
    }
    // HTTPS 请求为主，HTTPS_PROXY 优先；大小写两种命名都试
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// 系统代理自带的绕过列表（Windows ProxyOverride 等，分号分隔）
fn system_bypass_rules() -> String {
    sysproxy::Sysproxy::get_system_proxy()
        .map(|p| p.bypass)
        .unwrap_or_default()
}

/// 合并绕过规则：并入本地回环、统一逗号分隔、去重、丢弃 `<local>` 这类
/// 两端（reqwest / Node）都不认识的通配写法
fn merge_rules(user: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    for rule in format!("{LOCAL_NO_PROXY},{user}").split([',', ';']) {
        let rule = rule.trim();
        if rule.is_empty() || rule.eq_ignore_ascii_case("<local>") {
            continue;
        }
        if !seen.iter().any(|r| r.eq_ignore_ascii_case(rule)) {
            seen.push(rule.to_string());
        }
    }
    seen.join(",")
}

/// 校验代理地址能否被 reqwest 解析（http/https/socks5）。
pub fn validate_url(url: &str) -> Result<(), String> {
    reqwest::Proxy::all(url)
        .map(|_| ())
        .map_err(|e| format!("无效的代理地址: {e}"))
}

// ---------- sidecar 环境变量 ----------

/// sidecar 子进程需设置的代理环境变量。Node 侧 SDK 普遍不读 OS 级代理
/// 设置，统一转成 HTTP(S)_PROXY 注入。
pub fn sidecar_env_set() -> Vec<(String, String)> {
    let EffectiveProxy::Proxy { url, no_proxy } = effective() else {
        return Vec::new();
    };
    let mut envs: Vec<(String, String)> = ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"]
        .iter()
        .map(|k| (k.to_string(), url.clone()))
        .collect();
    envs.push(("NO_PROXY".into(), no_proxy.clone()));
    envs.push(("no_proxy".into(), no_proxy));
    envs
}

/// sidecar 子进程需移除的代理环境变量（直连时清掉继承自父进程的设置）
pub fn sidecar_env_unset() -> Vec<String> {
    match effective() {
        EffectiveProxy::Direct => ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy"]
            .iter()
            .map(|k| k.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

// ---------- Tauri commands ----------

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn proxy_get_settings() -> ProxySettings {
    settings()
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn proxy_set_settings(
    mode: String,
    url: String,
    no_proxy: String,
) -> Result<ProxySettings, String> {
    let mode = match mode.as_str() {
        "custom" => ProxyMode::Custom,
        "direct" => ProxyMode::Direct,
        _ => ProxyMode::System,
    };
    let url = url.trim().to_string();
    if mode == ProxyMode::Custom && !url.is_empty() {
        validate_url(&url)?;
    }
    let mut s = settings();
    s.mode = mode;
    s.url = url;
    s.no_proxy = no_proxy.trim().to_string();
    persist(&s)?;
    Ok(s)
}
