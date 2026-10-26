//! 通知推送：AI 助手审批/提问事件的 webhook 通知（飞书 / 企业微信 / 钉钉）
//!
//! 配置落 `~/.ashell/notify.json`。sidecar 事件转发处（sidecar.rs `forward_event`）
//! 检测到 `[TOOL_CONFIRM]` / `[AI_ASKUSERQUESTION]` 时调用 `spawn_event_notify`
//! 异步发送；发送失败仅记日志，不影响审批/提问交互链路。
//!
//! 飞书走 interactive 卡片（schema 2.0，彩色 header + markdown 正文），
//! 企业微信/钉钉机器人无对应卡片协议，保持纯文本。

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::config;
use crate::errors::{AppError, AppResult};

pub const PLATFORM_FEISHU: &str = "feishu";
pub const PLATFORM_WECOM: &str = "wecom";
pub const PLATFORM_DINGTALK: &str = "dingtalk";

/// 文本长度上限。企业微信 text 最严格（2048 字节，中文约 680 字），留余量。
const MAX_TEXT_CHARS: usize = 600;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    pub enabled: bool,
    /// `"feishu" | "wecom" | "dingtalk"`
    pub platform: String,
    pub webhook: String,
    /// 钉钉「加签」/ 飞书「签名校验」密钥；企业微信机器人无密钥，留空
    #[serde(default)]
    pub secret: String,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            platform: PLATFORM_FEISHU.to_string(),
            webhook: String::new(),
            secret: String::new(),
        }
    }
}

/// 一条正文行。`label` 为空表示纯内容行；`code` 在飞书卡片中以行内代码渲染。
#[derive(Debug, Clone)]
pub struct NotifyLine {
    pub label: String,
    pub text: String,
    pub code: bool,
}

impl NotifyLine {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            label: String::new(),
            text: text.into(),
            code: false,
        }
    }

    pub fn labeled(label: impl Into<String>, text: impl Into<String>, code: bool) -> Self {
        Self {
            label: label.into(),
            text: text.into(),
            code,
        }
    }
}

/// 待推送的通知事件；同一结构按平台渲染成卡片或纯文本。
#[derive(Debug, Clone)]
pub struct NotifyEvent {
    /// 标题，如 "AI 助手等待审批"
    pub title: String,
    /// 触发会话标识，放卡片副标题 / 文本首行
    pub sid: String,
    /// 飞书卡片 header 模板色："red" | "orange" | "blue" | "green" …
    pub template: &'static str,
    pub lines: Vec<NotifyLine>,
}

// ── Config persistence ──

fn config_path() -> AppResult<PathBuf> {
    Ok(config::app_dir()?.join("notify.json"))
}

pub fn load_config() -> AppResult<NotifyConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(NotifyConfig::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    let cfg: NotifyConfig =
        serde_json::from_str(&raw).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(cfg)
}

pub fn save_config(cfg: &NotifyConfig) -> AppResult<()> {
    let path = config_path()?;
    let json =
        serde_json::to_string_pretty(cfg).map_err(|e| AppError::Internal(e.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(())
}

// ── Sending ──

/// sidecar 事件通知入口（在同步线程上调用）：读配置，未启用/未配置 webhook 则
/// 静默跳过；通知在独立线程上以临时 tokio runtime 发送，不阻塞事件转发。
pub fn spawn_event_notify(sid: &str, title: &str, template: &'static str, lines: Vec<NotifyLine>) {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[NOTIFY] 读取通知配置失败: {e}");
            return;
        }
    };
    if !cfg.enabled || cfg.webhook.trim().is_empty() {
        return;
    }
    let event = NotifyEvent {
        title: title.to_string(),
        sid: sid.to_string(),
        template,
        lines,
    };
    let title = title.to_string();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::warn!("[NOTIFY] 构建 tokio runtime 失败: {e}");
                return;
            }
        };
        if let Err(e) = rt.block_on(send_message(&cfg, &event)) {
            tracing::warn!("[NOTIFY] {title} 通知发送失败: {e}");
        }
    });
}

/// 按平台协议推送一条通知，返回平台侧错误（HTTP 错误或业务错误码）。
pub async fn send_message(cfg: &NotifyConfig, event: &NotifyEvent) -> Result<(), String> {
    if cfg.webhook.trim().is_empty() {
        return Err("webhook 地址为空".into());
    }
    let client = build_client()?;
    match cfg.platform.as_str() {
        PLATFORM_FEISHU => {
            let mut body = feishu_card(event);
            let secret = cfg.secret.trim();
            if !secret.is_empty() {
                let (ts, sign) = feishu_sign(secret)?;
                body["timestamp"] = serde_json::Value::from(ts);
                body["sign"] = serde_json::Value::from(sign);
            }
            post_json(&client, &cfg.webhook, &body).await
        }
        PLATFORM_WECOM => {
            let body =
                serde_json::json!({ "msgtype": "text", "text": { "content": render_text(event) } });
            post_json(&client, &cfg.webhook, &body).await
        }
        PLATFORM_DINGTALK => {
            let url = dingtalk_signed_url(&cfg.webhook, cfg.secret.trim())?;
            let body =
                serde_json::json!({ "msgtype": "text", "text": { "content": render_text(event) } });
            post_json(&client, &url, &body).await
        }
        other => Err(format!("不支持的通知平台: {other}")),
    }
}

fn build_client() -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(10));
    // 与 fetch_models/webdav 一致：按应用内代理设置显式注入
    match crate::proxy::effective() {
        crate::proxy::EffectiveProxy::Direct => builder = builder.no_proxy(),
        crate::proxy::EffectiveProxy::Proxy { url, no_proxy } => {
            let mut proxy =
                reqwest::Proxy::all(&url).map_err(|e| format!("代理地址无效: {e}"))?;
            proxy = proxy.no_proxy(reqwest::NoProxy::from_string(&no_proxy));
            builder = builder.proxy(proxy);
        }
        crate::proxy::EffectiveProxy::Auto => {}
    }
    builder
        .build()
        .map_err(|e| format!("HTTP client 构建失败: {e}"))
}

/// 纯文本渲染（企业微信 / 钉钉）
fn render_text(event: &NotifyEvent) -> String {
    let mut lines = vec![format!("【ashell】{}", event.title)];
    lines.push(format!("会话: {}", event.sid));
    for line in &event.lines {
        if line.label.is_empty() {
            lines.push(line.text.clone());
        } else {
            lines.push(format!("{}: {}", line.label, line.text));
        }
    }
    truncate_chars(&lines.join("\n"), MAX_TEXT_CHARS)
}

/// 飞书 interactive 卡片（schema 2.0）：彩色 header + markdown 正文
fn feishu_card(event: &NotifyEvent) -> serde_json::Value {
    let markdown = event
        .lines
        .iter()
        .map(|l| match (l.label.as_str(), l.code) {
            ("", false) => l.text.clone(),
            ("", true) => format!("`{}`", escape_inline_code(&l.text)),
            (label, true) => format!("**{label}**:\n`{}`", escape_inline_code(&l.text)),
            (label, false) => format!("**{label}**: {}", l.text),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = truncate_chars(&markdown, MAX_TEXT_CHARS * 2);
    serde_json::json!({
        "msg_type": "interactive",
        "card": {
            "schema": "2.0",
            "config": { "update_multi": true },
            "header": {
                "title": { "tag": "plain_text", "content": format!("【ashell】{}", event.title) },
                "subtitle": { "tag": "plain_text", "content": format!("会话 {}", event.sid) },
                "template": event.template,
                "padding": "12px 12px 12px 12px"
            },
            "body": {
                "direction": "vertical",
                "padding": "12px 12px 12px 12px",
                "elements": [
                    {
                        "tag": "markdown",
                        "content": markdown,
                        "text_align": "left"
                    }
                ]
            }
        }
    })
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    let resp = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", truncate_chars(&text, 200)));
    }
    // 三个平台都约定 HTTP 200 + 业务错误码（字段名各异）
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    let errcode = v
        .get("errcode")
        .or_else(|| v.get("code"))
        .or_else(|| v.get("StatusCode"))
        .and_then(|x| x.as_i64());
    if let Some(code) = errcode {
        if code != 0 {
            let msg = v
                .get("errmsg")
                .or_else(|| v.get("msg"))
                .or_else(|| v.get("StatusMessage"))
                .and_then(|x| x.as_str())
                .unwrap_or("unknown");
            return Err(format!("平台返回错误 {code}: {msg}"));
        }
    }
    Ok(())
}

// ── Signing ──

/// 钉钉加签：sign = base64(HMAC-SHA256(key=secret, msg="{ts}\n{secret}"))，
/// URL 拼 `&timestamp={ts}&sign={urlencode(sign)}`
fn dingtalk_signed_url(webhook: &str, secret: &str) -> Result<String, String> {
    if secret.is_empty() {
        return Ok(webhook.to_string());
    }
    let ts = unix_secs();
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("加签密钥无效: {e}"))?;
    mac.update(format!("{ts}\n{secret}").as_bytes());
    let sign = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    Ok(format!(
        "{webhook}&timestamp={ts}&sign={}",
        percent_encode_b64(&sign)
    ))
}

/// 飞书签名校验：sign = base64(HMAC-SHA256(key="{ts}\n{secret}", msg=""))，
/// timestamp 与 sign 放进请求体
fn feishu_sign(secret: &str) -> Result<(String, String), String> {
    let ts = unix_secs().to_string();
    let string_to_sign = format!("{ts}\n{secret}");
    let mut mac = HmacSha256::new_from_slice(string_to_sign.as_bytes())
        .map_err(|e| format!("签名密钥无效: {e}"))?;
    mac.update(b"");
    let sign = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    Ok((ts, sign))
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// base64 字母表中仅 + / = 需要转义，直接替换即可满足钉钉对 sign 的 urlencode 要求
fn percent_encode_b64(b64: &str) -> String {
    b64.replace('+', "%2B").replace('/', "%2F").replace('=', "%3D")
}

/// 行内代码内容清洗：反引号会截断 code 片段，换行在行内代码中不合法
fn escape_inline_code(s: &str) -> String {
    s.replace('`', "'").replace('\n', " ")
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_text_unchanged() {
        assert_eq!(truncate_chars("abc", 10), "abc");
    }

    #[test]
    fn truncate_long_text_adds_ellipsis() {
        let s = "中".repeat(20);
        let out = truncate_chars(&s, 10);
        assert_eq!(out.chars().count(), 11);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn dingtalk_url_unsigned_passthrough() {
        assert_eq!(
            dingtalk_signed_url("https://oapi.dingtalk.com/robot/send?access_token=x", "").unwrap(),
            "https://oapi.dingtalk.com/robot/send?access_token=x"
        );
    }

    #[test]
    fn dingtalk_url_signed_appends_query() {
        let url = dingtalk_signed_url(
            "https://oapi.dingtalk.com/robot/send?access_token=x",
            "SECsecret",
        )
        .unwrap();
        assert!(url.starts_with(
            "https://oapi.dingtalk.com/robot/send?access_token=x&timestamp="
        ));
        assert!(url.contains("&sign="));
    }

    #[test]
    fn feishu_sign_is_stable_base64() {
        // 算法确定性：同 ts 同 secret 必出同签名
        let (_, sign) = feishu_sign("SECabc").unwrap();
        let (_, sign2) = feishu_sign("SECabc").unwrap();
        assert_eq!(sign, sign2);
        assert!(base64::engine::general_purpose::STANDARD.decode(&sign).is_ok());
    }

    fn sample_event() -> NotifyEvent {
        NotifyEvent {
            title: "AI 助手等待审批".into(),
            sid: "sess-1".into(),
            template: "orange",
            lines: vec![
                NotifyLine::labeled("命令", "rm -rf /tmp/x", true),
                NotifyLine::labeled("风险", "递归删除", false),
                NotifyLine::plain("请在 ashell 中处理该审批"),
            ],
        }
    }

    #[test]
    fn feishu_card_structure() {
        let card = feishu_card(&sample_event());
        assert_eq!(card["msg_type"], "interactive");
        assert_eq!(card["card"]["schema"], "2.0");
        assert_eq!(
            card["card"]["header"]["title"]["content"],
            "【ashell】AI 助手等待审批"
        );
        assert_eq!(card["card"]["header"]["template"], "orange");
        assert_eq!(card["card"]["header"]["subtitle"]["content"], "会话 sess-1");
        let content = card["card"]["body"]["elements"][0]["content"]
            .as_str()
            .unwrap();
        assert!(content.contains("**命令**:\n`rm -rf /tmp/x`"));
        assert!(content.contains("**风险**: 递归删除"));
        assert!(content.contains("请在 ashell 中处理该审批"));
    }

    #[test]
    fn feishu_card_escapes_backticks_in_code() {
        let mut event = sample_event();
        event.lines[0].text = "echo `whoami`".into();
        let card = feishu_card(&event);
        let content = card["card"]["body"]["elements"][0]["content"]
            .as_str()
            .unwrap();
        assert!(content.contains("`echo 'whoami'`"));
    }

    #[test]
    fn text_render_ignores_code_flag() {
        let text = render_text(&sample_event());
        assert!(text.starts_with("【ashell】AI 助手等待审批"));
        assert!(text.contains("会话: sess-1"));
        assert!(text.contains("命令: rm -rf /tmp/x"));
        assert!(text.contains("风险: 递归删除"));
        assert!(!text.contains('`'));
    }
}
