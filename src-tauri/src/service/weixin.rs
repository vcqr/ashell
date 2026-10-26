//! 微信远程：扫码登录个人微信机器人，长轮询保活并把通知/文本推送到微信。
//!
//! 协议对齐腾讯官方 `@tencent-weixin/openclaw-weixin@2.4.10`（MIT），
//! 请求格式与其保持一致；只实现「登录 + 保活配对 + 文本推送」。
//! 注意：后端所有接口在 HTTP 200 下用 `errcode` 报错（-14 = token 失效），
//! `ret` 与 `errcode` 都要检查（官方插件 sendMessage 漏查 errcode）。
//!
//! 凭证/配对/断点落 `~/.ashell/weixin.json`。运行态（登录会话、轮询任务、
//! 冷却）挂在本模块全局 RUNTIME 上；长轮询任务在应用启动时（有凭证则）
//! 与登录确认后拉起，token 失效按官方插件同样策略冷却 1 小时，重新扫码
//! 换新 token 后自动解除。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;
use crate::errors::{AppError, AppResult};

const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
/// 对齐插件 package.json 顶层 ilink_appid
const ILINK_APP_ID: &str = "bot";
/// 对齐插件版本号（iLink-App-ClientVersion 由它编码）
const CHANNEL_VERSION: &str = "2.4.10";
/// 观测用 UA（不参与鉴权/路由）
const BOT_AGENT: &str = "ashell-weixin";
/// get_bot_qrcode / get_qrcode_status 的 bot_type（插件当前构建取值）
const BOT_TYPE: &str = "3";
/// 服务端判定 token 失效的错误码，按官方插件策略冷却 1 小时
const STALE_TOKEN_ERRCODE: i64 = -14;
const STALE_PAUSE_MS: u64 = 60 * 60 * 1000;
/// 登录轮询/长轮询客户端超时
const LONG_POLL_TIMEOUT: Duration = Duration::from_secs(35);
/// 常规请求超时
const API_TIMEOUT: Duration = Duration::from_secs(15);
/// 单条消息长度上限（字符），超出拆分为多条
const MAX_TEXT_CHARS: usize = 1800;

// ── 持久化配置 ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeixinPeer {
    #[serde(default)]
    pub context_token: String,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub last_text: String,
}

/// `~/.ashell/weixin.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub bot_id: String,
    #[serde(default)]
    pub base_url: String,
    /// 扫码者用户 ID，默认推送目标
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub saved_at: u64,
    /// 通知事件是否同步推送到微信（「远程」板块里的开关）
    #[serde(default)]
    pub notify_enabled: bool,
    /// userId → 最近一次下发的 context_token（回复时透传提高可达性）
    #[serde(default)]
    pub peers: HashMap<String, WeixinPeer>,
    /// getupdates 断点，重启续传不丢消息
    #[serde(default)]
    pub sync_buf: String,
}

impl Default for WeixinConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            bot_id: String::new(),
            base_url: String::new(),
            user_id: String::new(),
            saved_at: 0,
            notify_enabled: false,
            peers: HashMap::new(),
            sync_buf: String::new(),
        }
    }
}

impl WeixinConfig {
    pub fn logged_in(&self) -> bool {
        !self.token.trim().is_empty()
    }

    pub fn effective_base_url(&self) -> String {
        let b = self.base_url.trim();
        if b.is_empty() {
            DEFAULT_BASE_URL.to_string()
        } else {
            b.to_string()
        }
    }
}

fn config_path() -> AppResult<PathBuf> {
    Ok(config::app_dir()?.join("weixin.json"))
}

pub fn load_config() -> AppResult<WeixinConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(WeixinConfig::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    let cfg: WeixinConfig =
        serde_json::from_str(&raw).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(cfg)
}

fn persist(cfg: &WeixinConfig) -> AppResult<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| AppError::Internal(e.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn save_config(cfg: &WeixinConfig) -> AppResult<()> {
    persist(cfg)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── 运行态 ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoginStatus {
    /// 未在登录
    #[default]
    Idle,
    /// 已取码待扫码
    Waiting,
    /// 已扫码，等待确认（可能要求配对数字）
    Scanned,
    /// 需要输入手机微信上显示的配对数字
    NeedVerifycode,
    /// 登录成功（凭证已保存、轮询已启动）
    Confirmed,
    /// 该微信已绑定过且未返回新凭证（本地凭证仍有效，可直接使用）
    Binded,
    /// 二维码多次失效/多次输错，登录流程终止
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginView {
    pub status: LoginStatus,
    /// 二维码内容 URL（前端渲染成二维码图片）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_url: Option<String>,
    /// 展示给用户的信息（失败原因等）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerView {
    pub user_id: String,
    pub updated_at: u64,
    pub last_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusView {
    pub logged_in: bool,
    pub bot_id: String,
    pub user_id: String,
    pub saved_at: u64,
    pub notify_enabled: bool,
    /// 长轮询任务是否在跑
    pub polling: bool,
    /// token 失效冷却剩余毫秒（0 = 未冷却）
    pub paused_remaining_ms: u64,
    pub last_poll_at: u64,
    pub last_inbound_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub peers: Vec<PeerView>,
}

struct LoginSession {
    qrcode: String,
    qr_url: String,
    started_at_ms: u64,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
struct RuntimeInner {
    login: Option<LoginSession>,
    login_status: LoginStatus,
    login_message: Option<String>,
    /// 登录轮询任务持有的待提交配对码
    pending_verify_code: Option<String>,
    /// 长轮询保活任务
    poll_task: Option<tokio::task::JoinHandle<()>>,
    paused_until_ms: u64,
    /// 触发冷却时使用的 token；换新 token（重新扫码）后自动解除
    paused_token: Option<String>,
    last_poll_at: u64,
    last_inbound_at: u64,
    last_error: Option<String>,
}

struct Runtime {
    inner: Mutex<RuntimeInner>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime {
        inner: Mutex::new(RuntimeInner::default()),
    })
}

// ── HTTP 协议层 ──

/// iLink-App-ClientVersion: uint32 编码 0x00MMNNPP
fn client_version_u32() -> u32 {
    let parts: Vec<u32> = CHANNEL_VERSION
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect();
    let (maj, min, pat) = (parts.first().copied().unwrap_or(0), parts.get(1).copied().unwrap_or(0), parts.get(2).copied().unwrap_or(0));
    (maj & 0xff) << 16 | (min & 0xff) << 8 | (pat & 0xff)
}

/// X-WECHAT-UIN：随机 uint32 → 十进制字符串 → base64
fn random_wechat_uin() -> String {
    let n: u32 = rand::rng().random();
    base64::engine::general_purpose::STANDARD.encode(n.to_string().as_bytes())
}

fn base_info() -> Value {
    serde_json::json!({
        "channel_version": CHANNEL_VERSION,
        "bot_agent": format!("{BOT_AGENT}/{}", env!("CARGO_PKG_VERSION")),
    })
}

/// 与 notify 一致的代理感知 client 构建（长轮询与常规请求分开超时）
fn build_client(timeout: Duration) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
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

fn api_client() -> Result<reqwest::Client, String> {
    build_client(API_TIMEOUT)
}

fn headers(token: Option<&str>) -> Result<reqwest::header::HeaderMap, String> {
    let mut map = reqwest::header::HeaderMap::new();
    map.insert("Content-Type", "application/json".parse().unwrap());
    map.insert("iLink-App-Id", ILINK_APP_ID.parse().unwrap());
    map.insert(
        "iLink-App-ClientVersion",
        client_version_u32().to_string().parse().unwrap(),
    );
    map.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
    map.insert("X-WECHAT-UIN", random_wechat_uin().parse().unwrap());
    if let Some(t) = token.map(str::trim).filter(|t| !t.is_empty()) {
        let v = format!("Bearer {t}");
        map.insert("Authorization", v.parse().unwrap());
    }
    Ok(map)
}

async fn post_json(
    base_url: &str,
    endpoint: &str,
    body: &Value,
    token: Option<&str>,
    long_poll: bool,
) -> Result<Value, String> {
    let url = format!("{}/{}", base_url.trim_end_matches('/'), endpoint);
    let client = build_client(if long_poll { LONG_POLL_TIMEOUT } else { API_TIMEOUT })?;
    let resp = client
        .post(&url)
        .headers(headers(token)?)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("{endpoint} 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("{endpoint} 读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("{endpoint} HTTP {status}: {}", truncate(&text, 200)));
    }
    serde_json::from_str(&text).map_err(|e| format!("{endpoint} 响应解析失败: {e}"))
}

async fn get_json(base_url: &str, endpoint: &str) -> Result<Value, String> {
    let url = format!("{}/{}", base_url.trim_end_matches('/'), endpoint);
    let client = build_client(LONG_POLL_TIMEOUT)?;
    let resp = client
        .get(&url)
        .headers(headers(None)?)
        .send()
        .await
        .map_err(|e| format!("GET {endpoint} 失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("{endpoint} 读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("{endpoint} HTTP {status}: {}", truncate(&text, 200)));
    }
    serde_json::from_str(&text).map_err(|e| format!("{endpoint} 响应解析失败: {e}"))
}

/// 读取业务错误码：ret / errcode 任一非 0 都算失败；返回 Some((code, errmsg))
fn biz_error(v: &Value) -> Option<(i64, String)> {
    let code = v.get("errcode").and_then(|x| x.as_i64()).unwrap_or(0);
    let ret = v.get("ret").and_then(|x| x.as_i64()).unwrap_or(0);
    if code != 0 || ret != 0 {
        let msg = v
            .get("errmsg")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        return Some((if code != 0 { code } else { ret }, msg));
    }
    None
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut cut = max;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        &s[..cut]
    }
}

// ── 文本处理 ──

/// 轻量 markdown 转纯文本（微信机器人消息不渲染 markdown）
pub fn strip_markdown(text: &str) -> String {
    let mut s = text.to_string();
    // 代码围栏标记与行内代码反引号
    while let Some(pos) = s.find("```") {
        let end = s[pos + 3..].find('\n').map(|n| pos + 3 + n + 1).unwrap_or(s.len());
        s.replace_range(pos..end, "");
    }
    // 成对行内代码
    while let (Some(a), Some(rest)) = (s.find('`'), s.get(..)) {
        let after = &rest[a + 1..];
        match after.find('`') {
            Some(b) => {
                let inner = after[..b].to_string();
                s.replace_range(a..a + 1 + b + 1, &inner);
            }
            None => {
                s.replace_range(a..a + 1, "");
                break;
            }
        }
    }
    // 粗体
    while let Some(a) = s.find("**") {
        match s[a + 2..].find("**") {
            Some(b) => {
                let inner = s[a + 2..a + 2 + b].to_string();
                s.replace_range(a..a + 2 + b + 2, &inner);
            }
            None => {
                s.replace_range(a..a + 2, "");
                break;
            }
        }
    }
    // 标题井号 / 列表符号
    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        let trimmed = line.trim_start();
        let replaced = if trimmed.starts_with('#') {
            Some(trimmed.trim_start_matches('#').trim_start().to_string())
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            Some(format!("• {}", &trimmed[2..]))
        } else {
            None
        };
        let line = replaced.as_deref().unwrap_or(line);
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// 超长拆分为多条（优先在换行处断开）
fn split_text(text: &str) -> Vec<String> {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut rest = text.to_string();
    while rest.chars().count() > MAX_TEXT_CHARS {
        let cut_char = rest
            .char_indices()
            .nth(MAX_TEXT_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        let head = &rest[..cut_char];
        let cut = match head.rfind('\n') {
            Some(n) if n > cut_char / 2 => n,
            _ => cut_char,
        };
        chunks.push(rest[..cut].to_string());
        rest = rest[cut..].trim_start_matches('\n').to_string();
    }
    if !rest.is_empty() {
        chunks.push(rest);
    }
    chunks
}

fn new_client_id() -> String {
    let n: u32 = rand::rng().random();
    format!("ashell:{}-{:08x}", now_ms(), n)
}

// ── 登录 ──

async fn fetch_qrcode(local_token: Option<&str>) -> Result<(String, String), String> {
    let token_list: Vec<&str> = local_token.filter(|t| !t.trim().is_empty()).into_iter().collect();
    let body = serde_json::json!({ "local_token_list": token_list });
    let resp = post_json(
        DEFAULT_BASE_URL,
        &format!("ilink/bot/get_bot_qrcode?bot_type={BOT_TYPE}"),
        &body,
        None,
        false,
    )
    .await?;
    let qrcode = resp
        .get("qrcode")
        .and_then(|v| v.as_str())
        .ok_or("get_bot_qrcode 未返回 qrcode")?
        .to_string();
    let qr_url = resp
        .get("qrcode_img_content")
        .and_then(|v| v.as_str())
        .ok_or("get_bot_qrcode 未返回 qrcode_img_content")?
        .to_string();
    Ok((qrcode, qr_url))
}

/// 登录轮询任务：长轮询 get_qrcode_status 驱动状态机，confirmed 落盘并启动保活轮询
async fn login_task(session_qrcode: String) {
    const MAX_REFRESH: u32 = 3;
    const LOGIN_TIMEOUT_MS: u64 = 480_000;

    let initial_qrcode = session_qrcode.clone();
    let mut qrcode = session_qrcode;
    let mut poll_base = DEFAULT_BASE_URL.to_string();
    let mut refresh_count: u32 = 1;
    let deadline = now_ms() + LOGIN_TIMEOUT_MS;
    // 已扫码提示只发一次
    let mut scanned_printed = false;

    loop {
        if now_ms() >= deadline {
            set_login(LoginStatus::Failed, Some("登录超时，请重试".into()));
            return;
        }

        let verify = runtime()
            .inner
            .lock()
            .ok()
            .and_then(|g| g.pending_verify_code.clone());

        let mut endpoint = format!("ilink/bot/get_qrcode_status?qrcode={}", urlencode(&qrcode));
        if let Some(code) = verify.as_deref().filter(|c| !c.is_empty()) {
            endpoint.push_str(&format!("&verify_code={}", urlencode(code)));
        }

        let resp = match get_json(&poll_base, &endpoint).await {
            Ok(v) => v,
            Err(_) => {
                // 网络抖动/网关超时：视为等待，继续轮询
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("wait");
        tracing::debug!("[WEIXIN] qrcode status={status}");

        match status {
            "wait" => {}
            "scaned" => {
                if verify.is_some() {
                    // 携带配对码后返回 scaned：校验通过
                    if let Ok(mut g) = runtime().inner.lock() {
                        g.pending_verify_code = None;
                    }
                }
                if !scanned_printed {
                    scanned_printed = true;
                    set_login(LoginStatus::Scanned, Some("已扫码，正在验证…".into()));
                }
            }
            "need_verifycode" => {
                if let Ok(mut g) = runtime().inner.lock() {
                    let wrong = g.pending_verify_code.is_some();
                    g.login_status = LoginStatus::NeedVerifycode;
                    g.login_message = Some(if wrong {
                        "配对数字不匹配，请重新输入".into()
                    } else {
                        String::new()
                    });
                }
            }
            "expired" | "verify_code_blocked" => {
                refresh_count += 1;
                if refresh_count > MAX_REFRESH {
                    let msg = if status == "expired" {
                        "二维码多次失效，登录已停止，请重试".to_string()
                    } else {
                        "配对数字多次错误，登录已停止，请稍后重试".to_string()
                    };
                    set_login(LoginStatus::Failed, Some(msg));
                    return;
                }
                if status == "verify_code_blocked" {
                    if let Ok(mut g) = runtime().inner.lock() {
                        g.pending_verify_code = None;
                    }
                }
                let local = load_config().map(|c| c.token).ok();
                match fetch_qrcode(local.as_deref()).await {
                    Ok((q, url)) => {
                        qrcode = q;
                        if let Ok(mut g) = runtime().inner.lock() {
                            if let Some(s) = g.login.as_mut() {
                                s.qrcode = qrcode.clone();
                                s.qr_url = url.clone();
                                s.started_at_ms = now_ms();
                            }
                            g.login_message = Some("二维码已刷新，请重新扫描".into());
                        }
                        scanned_printed = false;
                    }
                    Err(e) => {
                        set_login(LoginStatus::Failed, Some(format!("刷新二维码失败: {e}")));
                        return;
                    }
                }
            }
            "binded_redirect" => {
                set_login(
                    LoginStatus::Binded,
                    Some("该微信已绑定过此机器人，本地凭证仍有效，无需重复登录".into()),
                );
                return;
            }
            "scaned_but_redirect" => {
                if let Some(host) = resp.get("redirect_host").and_then(|v| v.as_str()) {
                    poll_base = format!("https://{host}");
                }
            }
            "confirmed" => {
                let token = resp
                    .get("bot_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let bot_id = resp
                    .get("ilink_bot_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if token.is_empty() || bot_id.is_empty() {
                    set_login(LoginStatus::Failed, Some("登录失败：服务端未返回完整凭证".into()));
                    return;
                }
                let mut cfg = load_config().unwrap_or_default();
                cfg.token = token;
                cfg.bot_id = bot_id;
                if let Some(b) = resp.get("baseurl").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    cfg.base_url = b.to_string();
                }
                if let Some(u) = resp.get("ilink_user_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    cfg.user_id = u.to_string();
                }
                cfg.saved_at = now_ms();
                if let Err(e) = persist(&cfg) {
                    set_login(LoginStatus::Failed, Some(format!("凭证保存失败: {e}")));
                    return;
                }
                start_poll_loop();
                set_login(LoginStatus::Confirmed, Some("已连接微信".into()));
                tracing::info!("[WEIXIN] 登录成功 bot_id={}", cfg.bot_id);
                return;
            }
            _ => {}
        }

        // 已被新登录会话替换/取消则退出
        let replaced = runtime()
            .inner
            .lock()
            .map(|g| match &g.login {
                Some(s) => s.qrcode != initial_qrcode && g.login_status == LoginStatus::Idle,
                None => true,
            })
            .unwrap_or(true);
        if replaced {
            return;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn set_login(status: LoginStatus, message: Option<String>) {
    if let Ok(mut g) = runtime().inner.lock() {
        g.login_status = status;
        g.login_message = message;
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 发起扫码登录：取二维码 + 启动登录轮询任务
pub async fn login_start() -> AppResult<LoginView> {
    let cfg = load_config()?;
    let (qrcode, qr_url) = fetch_qrcode(Some(&cfg.token))
        .await
        .map_err(AppError::Internal)?;

    let task = tokio::spawn(login_task(qrcode.clone()));

    if let Ok(mut g) = runtime().inner.lock() {
        // 取消旧会话任务
        if let Some(old) = g.login.take() {
            old.task.abort();
        }
        g.login_status = LoginStatus::Waiting;
        g.login_message = Some("请用手机微信扫描二维码".into());
        g.pending_verify_code = None;
        g.login = Some(LoginSession {
            qrcode,
            qr_url: qr_url.clone(),
            started_at_ms: now_ms(),
            task,
        });
    }

    Ok(LoginView {
        status: LoginStatus::Waiting,
        qr_url: Some(qr_url),
        message: Some("请用手机微信扫描二维码".into()),
    })
}

/// 读取登录状态（前端 1~2s 轮询一次）
pub fn login_view() -> LoginView {
    let g = runtime().inner.lock().ok();
    match g {
        Some(g) => LoginView {
            status: g.login_status.clone(),
            qr_url: g.login.as_ref().map(|s| s.qr_url.clone()),
            message: g.login_message.clone().filter(|m| !m.is_empty()),
        },
        None => LoginView {
            status: LoginStatus::Idle,
            qr_url: None,
            message: None,
        },
    }
}

/// 提交配对数字（need_verifycode 时由前端调用）
pub fn login_submit_code(code: &str) {
    if let Ok(mut g) = runtime().inner.lock() {
        g.pending_verify_code = Some(code.trim().to_string());
    }
}

/// 取消登录流程
pub fn login_cancel() {
    if let Ok(mut g) = runtime().inner.lock() {
        if let Some(s) = g.login.take() {
            s.task.abort();
        }
        g.login_status = LoginStatus::Idle;
        g.login_message = None;
        g.pending_verify_code = None;
    }
}

/// 退出登录：停轮询、清凭证
pub fn logout() -> AppResult<()> {
    login_cancel();
    if let Ok(mut g) = runtime().inner.lock() {
        if let Some(t) = g.poll_task.take() {
            t.abort();
        }
        g.paused_until_ms = 0;
        g.paused_token = None;
        g.last_error = None;
        g.last_poll_at = 0;
        g.last_inbound_at = 0;
    }
    let path = config_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ── 保活长轮询 ──

/// 应用启动时调用（有凭证则恢复长轮询）。须在 tokio runtime 上下文中调用。
pub fn init_polling() {
    match load_config() {
        Ok(cfg) if cfg.logged_in() => start_poll_loop(),
        _ => {}
    }
}

/// 启动（或替换）保活长轮询任务
fn start_poll_loop() {
    if let Ok(mut g) = runtime().inner.lock() {
        if let Some(old) = g.poll_task.take() {
            old.abort();
        }
        g.paused_until_ms = 0;
        g.paused_token = None;
        g.last_error = None;
        g.poll_task = Some(tokio::spawn(poll_loop()));
    }
    // 上线告知（best-effort）
    tokio::spawn(async {
        if let Err(e) = notify_lifecycle("notifystart").await {
            tracing::debug!("[WEIXIN] notifystart 失败: {e}");
        }
    });
}

async fn notify_lifecycle(endpoint: &str) -> Result<(), String> {
    let cfg = load_config().map_err(|e| e.to_string())?;
    if !cfg.logged_in() {
        return Err("未登录".into());
    }
    let body = serde_json::json!({ "base_info": base_info() });
    let resp = post_json(
        &cfg.effective_base_url(),
        &format!("ilink/bot/msg/{endpoint}"),
        &body,
        Some(&cfg.token),
        false,
    )
    .await?;
    if let Some((code, msg)) = biz_error(&resp) {
        return Err(format!("{endpoint} errcode={code} errmsg={msg}"));
    }
    Ok(())
}

/// 单次 getupdates：返回新 buf；成功时采集 context token 落盘
async fn poll_once(buf: &str) -> Result<String, String> {
    let cfg = load_config().map_err(|e| e.to_string())?;
    if !cfg.logged_in() {
        return Err("未登录".into());
    }
    let body = serde_json::json!({
        "get_updates_buf": buf,
        "base_info": base_info(),
    });
    let resp = post_json(
        &cfg.effective_base_url(),
        "ilink/bot/getupdates",
        &body,
        Some(&cfg.token),
        true,
    )
    .await?;

    if let Some((code, msg)) = biz_error(&resp) {
        if code == STALE_TOKEN_ERRCODE {
            let until = now_ms() + STALE_PAUSE_MS;
            if let Ok(mut g) = runtime().inner.lock() {
                g.paused_until_ms = until;
                g.paused_token = Some(cfg.token.clone());
                g.last_error = Some(format!("token 失效(errcode -14)，已冷却至 1 小时，请重新扫码登录"));
            }
            tracing::error!("[WEIXIN] token 失效(-14)，冷却 1 小时；重新扫码登录后自动恢复");
            return Ok(buf.to_string());
        }
        return Err(format!("getupdates errcode={code} errmsg={msg}"));
    }

    if let Ok(mut g) = runtime().inner.lock() {
        g.last_poll_at = now_ms();
        g.last_error = None;
    }

    let new_buf = resp
        .get("get_updates_buf")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 采集入站消息的 context token（配对）与文本（指令路由）
    let mut inbound: Vec<(String, String, String, i64)> = Vec::new();
    if let Some(msgs) = resp.get("msgs").and_then(|v| v.as_array()) {
        for m in msgs {
            let from = m.get("from_user_id").and_then(|v| v.as_str()).unwrap_or("");
            if from.is_empty() {
                continue;
            }
            let ctx = m
                .get("context_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let msg_type = m.get("message_type").and_then(|v| v.as_i64()).unwrap_or(0);
            // TEXT 项取全文（指令路由用），非文本项记类型名
            let mut text = String::new();
            if let Some(items) = m.get("item_list").and_then(|v| v.as_array()) {
                for it in items {
                    if it.get("type").and_then(|v| v.as_i64()) == Some(1) {
                        if let Some(s) = it.pointer("/text_item/text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(s);
                        }
                    } else if text.is_empty() {
                        let t = it.get("type").and_then(|v| v.as_i64()).unwrap_or(0);
                        text = format!("(type {t})");
                    }
                }
            }
            inbound.push((from.to_string(), ctx, text, msg_type));
        }
    }

    if !inbound.is_empty() || !new_buf.is_empty() {
        let mut cfg = load_config().map_err(|e| e.to_string())?;
        let mut changed = false;
        for (from, ctx, text, _) in &inbound {
            if ctx.is_empty() {
                continue;
            }
            cfg.peers.insert(
                from.clone(),
                WeixinPeer {
                    context_token: ctx.clone(),
                    updated_at: now_ms(),
                    last_text: truncate(text, 50).to_string(),
                },
            );
            tracing::info!("[WEIXIN] 配对更新 from={from} text=\"{}\"", truncate(text, 50));
            changed = true;
        }
        if !new_buf.is_empty() {
            cfg.sync_buf = new_buf.clone();
            changed = true;
        }
        if changed {
            if let Err(e) = persist(&cfg) {
                tracing::warn!("[WEIXIN] 状态落盘失败: {e}");
            }
        }
        if !inbound.is_empty() {
            if let Ok(mut g) = runtime().inner.lock() {
                g.last_inbound_at = now_ms();
            }
        }
    }

    // 指令路由：用户消息里 @sid 开头的当作远程答复转发进对应会话（不阻塞轮询）
    for (from, _, text, msg_type) in inbound {
        // message_type: 1=USER；跳过机器人自己发出的（防回环）
        if msg_type != 0 && msg_type != 1 {
            continue;
        }
        if let Some((sid, content)) = parse_weixin_command(&text) {
            let from = from.clone();
            tokio::spawn(async move {
                handle_weixin_command(&from, &sid, &content).await;
            });
        }
    }

    Ok(if new_buf.is_empty() {
        buf.to_string()
    } else {
        new_buf
    })
}

async fn poll_loop() {
    let mut buf = load_config().map(|c| c.sync_buf).unwrap_or_default();
    let mut consecutive_failures: u32 = 0;
    tracing::info!(
        "[WEIXIN] 长轮询启动（{}）",
        if buf.is_empty() { "无历史 buf" } else { "沿用历史 buf" }
    );

    loop {
        // 冷却中：每 10s 检查一次，换新 token 立即恢复
        {
            let paused = runtime()
                .inner
                .lock()
                .map(|g| g.paused_until_ms > now_ms())
                .unwrap_or(false);
            if paused {
                let token_changed = load_config()
                    .map(|c| {
                        runtime()
                            .inner
                            .lock()
                            .map(|g| g.paused_token.as_deref() != Some(c.token.as_str()))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if token_changed {
                    if let Ok(mut g) = runtime().inner.lock() {
                        g.paused_until_ms = 0;
                        g.paused_token = None;
                    }
                    tracing::info!("[WEIXIN] 检测到新凭证，解除冷却");
                    continue;
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        }

        match poll_once(&buf).await {
            Ok(new_buf) => {
                buf = new_buf;
                consecutive_failures = 0;
            }
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!(
                    "[WEIXIN] getupdates 失败 ({consecutive_failures}/5): {e}"
                );
                if let Ok(mut g) = runtime().inner.lock() {
                    g.last_error = Some(truncate(&e, 200).to_string());
                }
                let delay = if consecutive_failures >= 5 {
                    consecutive_failures = 0;
                    Duration::from_secs(30)
                } else {
                    Duration::from_secs(2)
                };
                tokio::time::sleep(delay).await;
            }
        }
    }
}

// ── 微信远程答复（@sid 指令） ──

/// 解析 `@<sid> 内容` 指令文本（兼容全角 ＠ 与前导空白）。
/// 内容为指令后剩余文本，换行折叠为空格（应答是单行 stdin）。
/// 不带 @ 前缀的普通消息返回 None（仅用于配对，不路由）。
fn parse_weixin_command(text: &str) -> Option<(String, String)> {
    let t = text.trim_start();
    let t = t
        .strip_prefix('＠')
        .or_else(|| t.strip_prefix('@'))?
        .trim_start();
    let mut it = t.splitn(2, char::is_whitespace);
    let sid = it.next()?.trim().to_string();
    if sid.is_empty() {
        return None;
    }
    let content = it
        .next()
        .unwrap_or("")
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some((sid, content))
}

/// 处理一条 @sid 远程答复：鉴权（仅扫码绑定的微信账号）→ 转发进会话 → 回执。
async fn handle_weixin_command(from: &str, sid: &str, content: &str) {
    let cfg = load_config().unwrap_or_default();
    if !cfg.user_id.is_empty() && from != cfg.user_id {
        tracing::warn!("[WEIXIN] 拒绝非绑定账号 {from} 的远程答复");
        let _ = send_text(Some(from), "仅扫码绑定的微信账号可远程答复 AShell 会话").await;
        return;
    }
    if content.is_empty() {
        let _ = send_text(
            Some(from),
            &format!("用法：@{sid} + 内容（审批回选项序号，提问回序号或自定义答案）"),
        )
        .await;
        return;
    }
    match crate::sidecar::write_to_sidecar(sid.to_string(), content.to_string()) {
        Ok(()) => {
            tracing::info!("[WEIXIN] 微信答复已转发 ssid={sid} content=\"{}\"", truncate(content, 50));
            let _ = send_text(Some(from), &format!("已转发至会话 {sid}")).await;
        }
        Err(e) => {
            let _ = send_text(Some(from), &format!("转发失败：{e}")).await;
        }
    }
}

// ── 发送 ──

/// sidecar 事件推送入口（在同步线程上调用）：读取远程配置，启用且已登录时
/// 在独立线程上以临时 tokio runtime 推送纯文本，不阻塞事件转发。
/// 文本携带 `@{sid}` 标识：用户在微信里回复 `@{sid} 序号/答案` 可远程答复
/// 该会话的审批/提问（入站解析见 poll_once → handle_weixin_command）。
pub fn spawn_event_push(title: &str, sid: &str, body_plain: &str) {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[WEIXIN] 读取远程配置失败: {e}");
            return;
        }
    };
    if !cfg.notify_enabled || !cfg.logged_in() {
        return;
    }
    let text = strip_markdown(&format!(
        "【ashell】{title}\n@{sid}\n\n{body_plain}\n\n微信快捷回复：@{sid} 序号或答案"
    ));
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::warn!("[WEIXIN] 构建 tokio runtime 失败: {e}");
                return;
            }
        };
        match rt.block_on(send_text(None, &text)) {
            Ok(n) => tracing::info!("[WEIXIN] 事件已推送到微信（{n} 条）"),
            Err(e) => tracing::warn!("[WEIXIN] 事件推送微信失败: {e}"),
        }
    });
}

fn default_target(cfg: &WeixinConfig) -> Option<String> {
    if !cfg.user_id.trim().is_empty() {
        return Some(cfg.user_id.trim().to_string());
    }
    // 回退到最近活跃的配对用户
    cfg.peers
        .iter()
        .max_by_key(|(_, p)| p.updated_at)
        .map(|(k, _)| k.clone())
}

/// 发送文本到指定用户（None 用默认目标）。返回消息条数。
pub async fn send_text(to: Option<&str>, text: &str) -> Result<usize, String> {
    let cfg = load_config().map_err(|e| e.to_string())?;
    if !cfg.logged_in() {
        return Err("未登录：请先在 设置 → 远程 中扫码登录微信".into());
    }

    // 冷却检查（换新 token 自动解除）
    {
        let (paused, paused_token) = runtime()
            .inner
            .lock()
            .map(|g| (g.paused_until_ms > now_ms(), g.paused_token.clone()))
            .unwrap_or((false, None));
        if paused {
            if paused_token.as_deref() != Some(cfg.token.trim()) {
                if let Ok(mut g) = runtime().inner.lock() {
                    g.paused_until_ms = 0;
                    g.paused_token = None;
                }
            } else {
                let min = runtime()
                    .inner
                    .lock()
                    .map(|g| g.paused_until_ms.saturating_sub(now_ms()) / 60_000 + 1)
                    .unwrap_or(60);
                return Err(format!("微信会话冷却中（token 失效），约 {min} 分钟后重试或重新扫码登录"));
            }
        }
    }

    let target = to
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| default_target(&cfg))
        .ok_or("没有可用的推送目标：请先在微信里给机器人发一条消息完成配对")?;

    let context_token = cfg.peers.get(&target).map(|p| p.context_token.clone());
    let client = api_client()?;
    let url = format!(
        "{}/ilink/bot/sendmessage",
        cfg.effective_base_url().trim_end_matches('/')
    );

    let mut count = 0usize;
    for chunk in split_text(text) {
        let body = serde_json::json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": target,
                "client_id": new_client_id(),
                "message_type": 2,
                "message_state": 2,
                "item_list": [{ "type": 1, "text_item": { "text": chunk } }],
                "context_token": context_token,
            },
            "base_info": base_info(),
        });
        let resp = client
            .post(&url)
            .headers(headers(Some(&cfg.token))?)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("sendMessage 请求失败: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("sendMessage 读取失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("sendMessage HTTP {status}: {}", truncate(&text, 200)));
        }
        let v: Value =
            serde_json::from_str(&text).map_err(|e| format!("sendMessage 响应解析失败: {e}"))?;
        if let Some((code, msg)) = biz_error(&v) {
            if code == STALE_TOKEN_ERRCODE {
                let until = now_ms() + STALE_PAUSE_MS;
                if let Ok(mut g) = runtime().inner.lock() {
                    g.paused_until_ms = until;
                    g.paused_token = Some(cfg.token.clone());
                    g.last_error = Some("token 失效(errcode -14)，已进入冷却".into());
                }
                return Err(
                    "微信 token 已失效（errcode -14）：请到 设置 → 远程 重新扫码登录".into(),
                );
            }
            return Err(format!("sendMessage errcode={code} errmsg={msg}"));
        }
        count += 1;
    }
    Ok(count)
}

// ── 状态视图 ──

pub fn status_view() -> StatusView {
    let cfg = load_config().unwrap_or_default();
    let g = runtime().inner.lock().ok();
    let now = now_ms();
    StatusView {
        logged_in: cfg.logged_in(),
        bot_id: cfg.bot_id.clone(),
        user_id: cfg.user_id.clone(),
        saved_at: cfg.saved_at,
        notify_enabled: cfg.notify_enabled,
        polling: g
            .as_ref()
            .map(|g| g.poll_task.as_ref().map(|t| !t.is_finished()).unwrap_or(false))
            .unwrap_or(false),
        paused_remaining_ms: g
            .as_ref()
            .map(|g| g.paused_until_ms.saturating_sub(now))
            .unwrap_or(0),
        last_poll_at: g.as_ref().map(|g| g.last_poll_at).unwrap_or(0),
        last_inbound_at: g.as_ref().map(|g| g.last_inbound_at).unwrap_or(0),
        last_error: g.as_ref().and_then(|g| g.last_error.clone()),
        peers: {
            let mut list: Vec<PeerView> = cfg
                .peers
                .iter()
                .map(|(k, p)| PeerView {
                    user_id: k.clone(),
                    updated_at: p.updated_at,
                    last_text: p.last_text.clone(),
                })
                .collect();
            list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            list
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_basic() {
        let (sid, content) = parse_weixin_command("@sess-abc 1").unwrap();
        assert_eq!(sid, "sess-abc");
        assert_eq!(content, "1");
    }

    #[test]
    fn parse_command_fullwidth_and_spaces() {
        let (sid, content) = parse_weixin_command("＠ abc-1 自定义 答案").unwrap();
        assert_eq!(sid, "abc-1");
        assert_eq!(content, "自定义 答案");
    }

    #[test]
    fn parse_command_multiline_content_folds() {
        let (sid, content) = parse_weixin_command("@s1 第一行\n第二行").unwrap();
        assert_eq!(sid, "s1");
        assert_eq!(content, "第一行 第二行");
    }

    #[test]
    fn parse_command_no_content() {
        let (sid, content) = parse_weixin_command("@s1").unwrap();
        assert_eq!(sid, "s1");
        assert_eq!(content, "");
    }

    #[test]
    fn parse_command_rejects_plain_text() {
        assert!(parse_weixin_command("你好").is_none());
        assert!(parse_weixin_command("hello @s1 hi").is_none());
        // @ 后无 sid 不路由
        assert!(parse_weixin_command("@").is_none());
        assert!(parse_weixin_command("＠  ").is_none());
    }
}
