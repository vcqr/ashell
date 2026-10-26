//! 通知推送：AI 助手审批/提问事件的 webhook 通知（飞书 / 企业微信 / 钉钉）
//!
//! 配置落 `~/.ashell/notify.json`。sidecar 事件转发处（sidecar.rs `forward_event`）
//! 检测到 `[TOOL_CONFIRM]` / `[AI_ASKUSERQUESTION]` 时调用 `spawn_event_notify`
//! 异步发送；发送失败仅记日志，不影响审批/提问交互链路。
//!
//! 飞书走 interactive 卡片（schema 2.0，彩色 header + markdown 正文），
//! 钉钉走 markdown 消息、企业微信走 markdown_v2 消息：三者均以 markdown
//! 渲染正文，钉钉不支持行内代码（code 行退化为粗体），企业微信/飞书一致。
//! 自定义平台：webhook 与消息体模板均可配，模板支持 {title}/{sid}/
//! {content}/{markdown} 占位（合法 JSON 模板在字符串值内替换后按 JSON
//! 发送，否则整体替换后按 text/plain 发送）。

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
pub const PLATFORM_CUSTOM: &str = "custom";

/// 正文长度基准（markdown 渲染按 2 倍截断）。企业微信 markdown_v2 content
/// 上限 4096 字节，1200 全角字符 ≈ 3600 字节，留余量。
const MAX_TEXT_CHARS: usize = 600;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    pub enabled: bool,
    /// `"feishu" | "wecom" | "dingtalk" | "custom"`
    pub platform: String,
    pub webhook: String,
    /// 钉钉「加签」/ 飞书「签名校验」密钥；企业微信机器人无密钥，留空
    #[serde(default)]
    pub secret: String,
    /// 自定义平台的消息体模板（{title}/{sid}/{content}/{markdown} 占位）
    #[serde(default)]
    pub custom_template: String,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            platform: PLATFORM_FEISHU.to_string(),
            webhook: String::new(),
            secret: String::new(),
            custom_template: String::new(),
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

/// sidecar 事件通知入口（在同步线程上调用）：先分发微信远程推送（独立配置），
/// 再读通知配置，未启用/未配置 webhook 则静默跳过；发送在独立线程上以
/// 临时 tokio runtime 进行，不阻塞事件转发。
pub fn spawn_event_notify(sid: &str, title: &str, template: &'static str, lines: Vec<NotifyLine>) {
    let event = NotifyEvent {
        title: title.to_string(),
        sid: sid.to_string(),
        template,
        lines,
    };

    // 微信远程推送（设置 → 远程；独立于 webhook 通知开关）
    {
        let body = plain_lines(&event);
        crate::service::weixin::spawn_event_push(title, sid, &body);
    }

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
        match rt.block_on(send_message(&cfg, &event)) {
            Ok(()) => tracing::info!("[NOTIFY] {title} 通知已发送（{}）", cfg.platform),
            Err(e) => tracing::warn!("[NOTIFY] {title} 通知发送失败: {e}"),
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
        PLATFORM_WECOM => post_json(&client, &cfg.webhook, &wecom_markdown_body(event)).await,
        PLATFORM_DINGTALK => {
            let url = dingtalk_signed_url(&cfg.webhook, cfg.secret.trim())?;
            post_json(&client, &url, &dingtalk_markdown_body(event)).await
        }
        PLATFORM_CUSTOM => send_custom(&client, cfg, event).await,
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

/// 企业微信 markdown_v2 消息体（支持行内代码/代码块/表格，正文与飞书一致）
fn wecom_markdown_body(event: &NotifyEvent) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "markdown_v2",
        "markdown_v2": { "content": markdown_body(event, true) },
    })
}

/// 钉钉 markdown 消息体。`title` 字段用于聊天列表/通知栏预览，正文自带头部
fn dingtalk_markdown_body(event: &NotifyEvent) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "markdown",
        "markdown": {
            "title": format!("【ashell】{}", event.title),
            "text": markdown_body(event, false),
        }
    })
}

/// 事件正文行渲染为 markdown（不含标题/会话头部），行间空行分隔。
/// `use_inline_code` 同 `markdown_line`
fn markdown_lines(event: &NotifyEvent, use_inline_code: bool) -> String {
    event
        .lines
        .iter()
        .map(|l| markdown_line(l, use_inline_code))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 事件正文行渲染为纯文本（不含标题/会话头部）
fn plain_lines(event: &NotifyEvent) -> String {
    event
        .lines
        .iter()
        .map(|l| {
            if l.label.is_empty() {
                l.text.clone()
            } else {
                format!("{}: {}", l.label, l.text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 钉钉/企业微信 markdown 正文：标题/会话置顶（单个换行不折行）
fn markdown_body(event: &NotifyEvent, use_inline_code: bool) -> String {
    let md = format!(
        "#### 【ashell】{}\n- 会话 {}\n\n{}",
        event.title,
        event.sid,
        markdown_lines(event, use_inline_code)
    );
    truncate_chars(&md, MAX_TEXT_CHARS * 2)
}

// ── 自定义平台 ──

/// 自定义平台变量取值
fn custom_vars(event: &NotifyEvent) -> Vec<(&'static str, String)> {
    vec![
        ("title", event.title.clone()),
        ("sid", event.sid.clone()),
        ("content", plain_lines(event)),
        ("markdown", markdown_lines(event, true)),
    ]
}

/// 在 JSON 字符串值内递归替换 {var} 占位：只动字符串值，serde_json 重
/// 序列化时自动转义，正文内容不会破坏 JSON 结构
fn replace_vars_in_json(v: &mut serde_json::Value, vars: &[(&'static str, String)]) {
    match v {
        serde_json::Value::String(s) => {
            for (k, val) in vars {
                let ph = format!("{{{k}}}");
                if s.contains(&ph) {
                    *s = s.replace(&ph, val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                replace_vars_in_json(item, vars);
            }
        }
        serde_json::Value::Object(map) => {
            for val in map.values_mut() {
                replace_vars_in_json(val, vars);
            }
        }
        _ => {}
    }
}

/// 非 JSON 模板：整体字符串替换
fn render_custom_text(template: &str, vars: &[(&'static str, String)]) -> String {
    let mut text = template.to_string();
    for (k, val) in vars {
        text = text.replace(&format!("{{{k}}}"), val);
    }
    text
}

/// 自定义平台推送：模板为合法 JSON 时在字符串值内替换占位后按 JSON 发送，
/// 否则整体替换后按 text/plain 发送
async fn send_custom(
    client: &reqwest::Client,
    cfg: &NotifyConfig,
    event: &NotifyEvent,
) -> Result<(), String> {
    let template = cfg.custom_template.trim();
    if template.is_empty() {
        return Err("消息体模板为空".into());
    }
    let vars = custom_vars(event);
    match serde_json::from_str::<serde_json::Value>(template) {
        Ok(mut body) => {
            replace_vars_in_json(&mut body, &vars);
            post_json(client, &cfg.webhook, &body).await
        }
        Err(_) => post_raw(
            client,
            &cfg.webhook,
            "text/plain; charset=utf-8",
            render_custom_text(template, &vars),
        )
        .await,
    }
}

/// 飞书 interactive 卡片（schema 2.0）：彩色 header + markdown 正文
fn feishu_card(event: &NotifyEvent) -> serde_json::Value {
    let markdown = event
        .lines
        .iter()
        .map(|l| markdown_line(l, true))
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
    check_response(resp).await
}

/// 自定义平台的非 JSON 请求体（text/plain）
async fn post_raw(
    client: &reqwest::Client,
    url: &str,
    content_type: &str,
    body: String,
) -> Result<(), String> {
    let resp = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    check_response(resp).await
}

/// 各平台共用响应校验：HTTP 非 2xx 报错；2xx 时若响应 JSON 含
/// errcode/code/StatusCode 且非 0 则视为业务失败（内置平台约定，字段名各异）
async fn check_response(resp: reqwest::Response) -> Result<(), String> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", truncate_chars(&text, 200)));
    }
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
/// URL 拼 `&timestamp={ts}&sign={urlencode(sign)}`。timestamp 为毫秒级
/// （官方示例 System.currentTimeMillis()），服务端校验 1 小时有效期
fn dingtalk_signed_url(webhook: &str, secret: &str) -> Result<String, String> {
    if secret.is_empty() {
        return Ok(webhook.to_string());
    }
    let ts = unix_millis();
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

/// 秒级时间戳（飞书签名校验用）
fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 毫秒级时间戳（钉钉加签用）
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
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

/// 单行渲染为 markdown。code 行的包裹方式按平台区分：飞书/企业微信支持
/// 行内代码，钉钉 markdown 无该语法，退化为粗体
fn markdown_line(l: &NotifyLine, use_inline_code: bool) -> String {
    match (l.label.as_str(), l.code) {
        ("", false) => l.text.clone(),
        ("", true) => wrap_code(&l.text, use_inline_code),
        (label, true) => format!("**{label}**:\n{}", wrap_code(&l.text, use_inline_code)),
        (label, false) => format!("**{label}**: {}", l.text),
    }
}

fn wrap_code(s: &str, use_inline_code: bool) -> String {
    if use_inline_code {
        format!("`{}`", escape_inline_code(s))
    } else {
        // 换行会截断粗体片段，折叠为空格
        format!("**{}**", s.replace('\n', " "))
    }
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
    fn dingtalk_url_signed_appends_millisecond_timestamp() {
        let url = dingtalk_signed_url(
            "https://oapi.dingtalk.com/robot/send?access_token=x",
            "SECsecret",
        )
        .unwrap();
        assert!(url.starts_with("https://oapi.dingtalk.com/robot/send?access_token=x&timestamp="));
        assert!(url.contains("&sign="));
        // 官方要求毫秒级时间戳（13 位），秒级会被服务端判为签名过期
        let ts = url
            .split("&timestamp=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .unwrap();
        assert_eq!(ts.len(), 13);
        assert!(ts.chars().all(|c| c.is_ascii_digit()));
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
    fn markdown_body_wechat_style_uses_inline_code() {
        let md = markdown_body(&sample_event(), true);
        assert!(md.starts_with("#### 【ashell】AI 助手等待审批"));
        assert!(md.contains("- 会话 sess-1"));
        assert!(md.contains("**命令**:\n`rm -rf /tmp/x`"));
        assert!(md.contains("**风险**: 递归删除"));
        assert!(md.contains("请在 ashell 中处理该审批"));
    }

    #[test]
    fn markdown_body_dingtalk_style_bold_code() {
        let md = markdown_body(&sample_event(), false);
        assert!(md.starts_with("#### 【ashell】AI 助手等待审批"));
        assert!(md.contains("- 会话 sess-1"));
        assert!(md.contains("**命令**:\n**rm -rf /tmp/x**"));
        assert!(md.contains("**风险**: 递归删除"));
        assert!(md.contains("请在 ashell 中处理该审批"));
    }

    #[test]
    fn markdown_body_dingtalk_code_folds_newlines() {
        let mut event = sample_event();
        event.lines[0].text = "echo `whoami`\ncd /tmp".into();
        let md = markdown_body(&event, false);
        // 钉钉无行内代码语法：退化为粗体，换行折叠为空格，不残留反引号包裹
        assert!(md.contains("**echo `whoami` cd /tmp**"));
        assert!(!md.contains(":\n`"));
    }

    #[test]
    fn wecom_markdown_body_shape() {
        let body = wecom_markdown_body(&sample_event());
        assert_eq!(body["msgtype"], "markdown_v2");
        let content = body["markdown_v2"]["content"].as_str().unwrap();
        assert!(content.starts_with("#### 【ashell】AI 助手等待审批"));
        assert!(content.contains("`rm -rf /tmp/x`"));
    }

    #[test]
    fn dingtalk_markdown_body_shape() {
        let body = dingtalk_markdown_body(&sample_event());
        assert_eq!(body["msgtype"], "markdown");
        assert_eq!(body["markdown"]["title"], "【ashell】AI 助手等待审批");
        let text = body["markdown"]["text"].as_str().unwrap();
        assert!(text.starts_with("#### 【ashell】AI 助手等待审批"));
    }

    #[test]
    fn custom_vars_plain_and_markdown() {
        let vars = custom_vars(&sample_event());
        let get = |k: &str| {
            vars.iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get("title"), "AI 助手等待审批");
        assert_eq!(get("sid"), "sess-1");
        assert!(get("content").contains("命令: rm -rf /tmp/x"));
        assert!(get("markdown").contains("**命令**:\n`rm -rf /tmp/x`"));
        // 正文变量不含标题/会话头部，由模板自行编排
        assert!(!get("content").contains("AI 助手等待审批"));
    }

    #[test]
    fn custom_json_template_substitutes_and_escapes() {
        let mut event = sample_event();
        event.lines[0].text = "echo \"hi\"\nrm -rf /".into();
        let template = r#"{"msgtype":"text","text":{"content":"【ashell】{title}\n{content}"}}"#;
        let mut body: serde_json::Value = serde_json::from_str(template).unwrap();
        replace_vars_in_json(&mut body, &custom_vars(&event));
        // 命令中的引号/换行经 serde 转义，替换后 JSON 结构不被破坏
        let content = body["text"]["content"].as_str().unwrap();
        assert!(content.contains("【ashell】AI 助手等待审批"));
        assert!(content.contains("命令: echo \"hi\"\nrm -rf /"));
        assert!(body.to_string().contains("\\n"));
    }

    #[test]
    fn custom_json_template_replaces_nested_values() {
        let template =
            r#"{"a":{"b":["{title}","x{sid}y"]},"c":"{markdown}","d":123,"e":true}"#;
        let mut body: serde_json::Value = serde_json::from_str(template).unwrap();
        replace_vars_in_json(&mut body, &custom_vars(&sample_event()));
        assert_eq!(body["a"]["b"][0], "AI 助手等待审批");
        assert_eq!(body["a"]["b"][1], "xsess-1y");
        assert!(body["c"].as_str().unwrap().contains("**命令**"));
        // 非字符串值不受影响
        assert_eq!(body["d"], 123);
        assert_eq!(body["e"], true);
    }

    #[test]
    fn custom_text_template_plain_substitution() {
        let text = render_custom_text(
            "ashell: {title} / {sid}\n{content}\n未知占位 {nope} 保留",
            &custom_vars(&sample_event()),
        );
        assert!(text.starts_with("ashell: AI 助手等待审批 / sess-1"));
        assert!(text.contains("命令: rm -rf /tmp/x"));
        assert!(text.contains("未知占位 {nope} 保留"));
    }
}
