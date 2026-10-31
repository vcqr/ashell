use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use crate::errors::{AppError, KbdPrompt};
use crate::handlers;
use crate::models::Host;
use crate::service::ssh::{self as ssh_svc, Session};
use crate::service::{self, AppState};
use crate::util;

#[derive(Debug, Deserialize)]
pub struct TerminalQuery {
    /// 客户端可指定 sid（与 SFTP REST 复用）；省略则自动生成
    pub sid: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u32,
    #[serde(default = "default_rows")]
    pub rows: u32,
    #[serde(default = "default_term")]
    pub term: String,
    /// 是否注入 cwd 上报钩子（前端按 SFTP 目录跟随开关在连接时声明）
    #[serde(default)]
    pub cwd_report: bool,
}

fn default_cols() -> u32 {
    80
}
fn default_rows() -> u32 {
    24
}
fn default_term() -> String {
    "xterm-256color".into()
}

/// `GET /api/ssh/terminal/{host_id}?sid=&cols=&rows=&term=&token=`
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(host_id): Path<i64>,
    Query(q): Query<TerminalQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, host_id, q))
}

async fn handle_socket(socket: WebSocket, state: AppState, host_id: i64, q: TerminalQuery) {
    let sid = q.sid.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Err(e) = run_terminal(socket, state, host_id, q, &sid).await {
        log::error!("terminal session error sid={sid}: {e}");
    }
    // 清理会话
    ssh_svc::remove(&sid).await;
}

/// 客户端控制消息
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ClientMsg {
    /// 用户键盘输入
    Cmd { data: String },
    /// 终端尺寸变更
    Resize { cols: u32, rows: u32 },
    /// 心跳
    Ping,
    /// 用户确认自动填充 sudo 密码
    #[serde(rename = "sudo_fill")]
    SudoFill,
    /// 用户提交新密码（密码过期/变更后的重新认证）
    #[serde(rename = "auth_response")]
    AuthResponse {
        password: String,
        #[serde(default)]
        remember: bool,
    },
    /// 用户提交 keyboard-interactive（2FA/TOTP）逐条应答，
    /// 与 auth_kb_request.prompts 中的提示按序对应
    #[serde(rename = "auth_kb_response")]
    KbdAuthResponse { answers: Vec<String> },
    /// 用户确认/拒绝信任主机密钥（TOFU 弹窗结果）
    #[serde(rename = "hostkey_response")]
    HostKeyResponse { trust: bool },
    /// 用户取消重新认证
    #[serde(rename = "auth_cancel")]
    AuthCancel,
}

/// 服务端首条消息：把生成的 sid 通知客户端
#[derive(Debug, Serialize)]
struct ReadyMsg<'a> {
    kind: &'a str,
    sid: &'a str,
}

/// 服务端消息：SSH 认证失败（密码过期/被修改），请求前端弹窗重新输入
#[derive(Debug, Serialize)]
struct AuthRequiredMsg<'a> {
    kind: &'a str,
    host_id: i64,
    /// user@addr:port 形式的目标描述，供弹窗展示
    label: &'a str,
}

/// 服务端消息：私钥需要 passphrase（未设置或口令错误），请求前端弹窗输入。
/// 前端复用 auth_response 帧应答（password 字段携带 passphrase）
#[derive(Debug, Serialize)]
struct PassphraseRequiredMsg<'a> {
    kind: &'a str,
    host_id: i64,
    /// user@addr:port 形式的目标描述，供弹窗展示
    label: &'a str,
    /// 解码失败的原始原因（口令错误 / 密钥损坏等），供弹窗展示提示
    message: &'a str,
}

/// 服务端消息：keyboard-interactive 认证需要用户应答（2FA/TOTP 等场景），
/// 请求前端弹窗按提示逐条收集输入；前端用 auth_kb_response 帧应答
#[derive(Debug, Serialize)]
struct KbdAuthRequestMsg<'a> {
    kind: &'a str,
    host_id: i64,
    /// user@addr:port 形式的目标描述，供弹窗展示
    label: &'a str,
    /// 设备/方法名（如 pam_google_authenticator），可为空
    name: &'a str,
    /// 服务器附带的说明文字，可为空
    instructions: &'a str,
    /// 待用户输入的提示 [{prompt, echo}]（已自动应答项不含在内）
    prompts: serde_json::Value,
}

/// 服务端消息：主机密钥待确认（首次连接 / 指纹变更），请求前端弹窗核对指纹
#[derive(Debug, Serialize)]
struct HostKeyConfirmMsg<'a> {
    kind: &'a str,
    host_id: i64,
    /// addr:port 形式的目标描述，供弹窗展示
    label: &'a str,
    key_type: &'a str,
    fingerprint: &'a str,
    /// 库中已存指纹；Some 即指纹变更（弹窗需强警告）
    previous: Option<&'a str>,
}

/// 等待用户在弹窗中输入密码的超时时间
const AUTH_INPUT_TIMEOUT: Duration = Duration::from_secs(300);

/// 认证重试次数上限：防止错误密码经 WS 无限暴力尝试
const MAX_AUTH_ATTEMPTS: u32 = 5;

/// 下发致命错误帧。`code` 是可选的错误分类码（如 connect_refused），
/// 前端据此映射 i18n 文案做全局提示；无码时展示原始 message。
async fn send_fatal(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    message: &str,
    code: Option<&str>,
) {
    let mut payload = serde_json::json!({ "kind": "fatal", "message": message });
    if let Some(code) = code {
        payload["code"] = serde_json::json!(code);
    }
    let _ = ws_tx.send(Message::Text(payload.to_string().into())).await;
}

/// 下发建连进度帧（阶段文案由前端 i18n）
async fn send_progress(ws_tx: &mut SplitSink<WebSocket, Message>, stage: &str) {
    let payload = serde_json::json!({ "kind": "progress", "stage": stage }).to_string();
    let _ = ws_tx.send(Message::Text(payload.into())).await;
}

/// 建连总超时：TCP 阶段在 ssh.rs 已有 15s 限时，这里兜底握手/认证悬挂
/// （认证的用户交互发生在 connect_prepared 返回之后的弹窗循环里，不占此处预算）
const CONNECT_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

/// 将建连失败的底层错误归类为面向用户的错误码（前端据此做 i18n 全局提示）。
/// 命中常见网络错误时返回分类码；其余（罕见 IO 错误等）返回 None，前端展示原文。
fn classify_connect_failure(err: &crate::errors::AppError) -> Option<&'static str> {
    let msg = err.to_string().to_ascii_lowercase();
    if msg.contains("refused") {
        Some("connect_refused")
    } else if msg.contains("timed out") || msg.contains("timeout") {
        Some("connect_timeout")
    } else if msg.contains("failed to lookup")
        || msg.contains("name or service not known")
        || msg.contains("nodename nor servname")
        || msg.contains("no address")
        || msg.contains("name resolution")
    {
        Some("connect_dns")
    } else if msg.contains("unreachable") || msg.contains("network is down") {
        Some("connect_unreachable")
    } else {
        None
    }
}

/// 将会话建立后的中断错误归类为面向用户的错误码（前端 i18n 展示断开原因）。
/// 心跳超时对用户有直接诊断价值：提示调大 keepalive 间隔可缓解 NAT 超时断连。
fn classify_session_end_err(e: &russh::Error) -> &'static str {
    match e {
        russh::Error::KeepaliveTimeout => "keepalive_timeout",
        russh::Error::Disconnect => "server_disconnected",
        _ => "network_error",
    }
}

async fn send_pong(ws_tx: &mut SplitSink<WebSocket, Message>) {
    let _ = ws_tx
        .send(Message::Text("{\"kind\":\"pong\"}".to_string().into()))
        .await;
}

/// 认证失败后向前端请求新密码。
///
/// 返回 `Some((password, remember))` 表示用户提交；`None` 表示用户取消、
/// 输入超时或连接已断开。等待期间仅处理 auth 帧与心跳，忽略键盘输入。
async fn request_new_password(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    ws_rx: &mut SplitStream<WebSocket>,
    host_id: i64,
    label: &str,
) -> Option<(String, bool)> {
    let msg = serde_json::to_string(&AuthRequiredMsg {
        kind: "auth_required",
        host_id,
        label,
    })
    .ok()?;
    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
        return None;
    }

    let deadline = tokio::time::sleep(AUTH_INPUT_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                send_fatal(ws_tx, "等待输入新密码超时", Some("input_timeout")).await;
                return None;
            }
            frame = ws_rx.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let s = text.as_str();
                        if !(s.starts_with('{') && s.ends_with('}')) {
                            continue;
                        }
                        match serde_json::from_str::<ClientMsg>(s) {
                            Ok(ClientMsg::AuthResponse { password, remember }) => {
                                return Some((password, remember));
                            }
                            Ok(ClientMsg::AuthCancel) => return None,
                            Ok(ClientMsg::Ping) => send_pong(ws_tx).await,
                            _ => {} // 建连阶段忽略其它输入帧
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return None,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("ws recv error during auth: {e}");
                        return None;
                    }
                }
            }
        }
    }
}

/// 认证失败后向前端请求私钥 passphrase（解码带口令的私钥）。
///
/// 返回 `Some((passphrase, remember))` 表示用户提交；`None` 表示用户取消、
/// 输入超时或连接已断开。等待期间仅处理 auth 帧与心跳，忽略键盘输入。
/// 前端复用 auth_response 帧应答（password 字段携带 passphrase）。
async fn request_new_passphrase(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    ws_rx: &mut SplitStream<WebSocket>,
    host_id: i64,
    label: &str,
    message: &str,
) -> Option<(String, bool)> {
    let msg = serde_json::to_string(&PassphraseRequiredMsg {
        kind: "passphrase_required",
        host_id,
        label,
        message,
    })
    .ok()?;
    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
        return None;
    }

    let deadline = tokio::time::sleep(AUTH_INPUT_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                send_fatal(ws_tx, "等待输入 passphrase 超时", Some("input_timeout")).await;
                return None;
            }
            frame = ws_rx.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let s = text.as_str();
                        if !(s.starts_with('{') && s.ends_with('}')) {
                            continue;
                        }
                        match serde_json::from_str::<ClientMsg>(s) {
                            Ok(ClientMsg::AuthResponse { password, remember }) => {
                                return Some((password, remember));
                            }
                            Ok(ClientMsg::AuthCancel) => return None,
                            Ok(ClientMsg::Ping) => send_pong(ws_tx).await,
                            _ => {} // 建连阶段忽略其它输入帧
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return None,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("ws recv error during auth: {e}");
                        return None;
                    }
                }
            }
        }
    }
}

/// keyboard-interactive（2FA/TOTP）需要用户应答时向前端请求逐条输入。
///
/// 返回 `Some(answers)` 表示用户提交，与待应答提示按序对应；`None` 表示
/// 用户取消、输入超时或连接已断开。应答仅用于本次重连重放，不落库。
async fn request_kbd_answers(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    ws_rx: &mut SplitStream<WebSocket>,
    host_id: i64,
    label: &str,
    name: &str,
    instructions: &str,
    prompts: &[KbdPrompt],
) -> Option<Vec<String>> {
    let pending: Vec<&KbdPrompt> = prompts.iter().filter(|p| !p.answered).collect();
    let msg = serde_json::to_string(&KbdAuthRequestMsg {
        kind: "auth_kb_request",
        host_id,
        label,
        name,
        instructions,
        prompts: serde_json::Value::Array(
            pending
                .iter()
                .map(|p| serde_json::json!({ "prompt": p.prompt, "echo": p.echo }))
                .collect(),
        ),
    })
    .ok()?;
    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
        return None;
    }

    let deadline = tokio::time::sleep(AUTH_INPUT_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                send_fatal(ws_tx, "等待输入认证信息超时", Some("input_timeout")).await;
                return None;
            }
            frame = ws_rx.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let s = text.as_str();
                        if !(s.starts_with('{') && s.ends_with('}')) {
                            continue;
                        }
                        match serde_json::from_str::<ClientMsg>(s) {
                            Ok(ClientMsg::KbdAuthResponse { answers }) => {
                                if answers.len() != pending.len() {
                                    continue; // 应答数与提示数不符，视为无效帧继续等待
                                }
                                return Some(answers);
                            }
                            Ok(ClientMsg::AuthCancel) => return None,
                            Ok(ClientMsg::Ping) => send_pong(ws_tx).await,
                            _ => {} // 建连阶段忽略其它输入帧
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return None,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("ws recv error during auth: {e}");
                        return None;
                    }
                }
            }
        }
    }
}

/// 主机密钥待确认时向前端请求指纹核对结果。
///
/// 返回 `Some(true)` 表示用户信任（调用方写库后重试建连）；
/// `Some(false)` / `None` 表示用户拒绝、取消、超时或连接已断开。
async fn request_hostkey_confirm(
    ws_tx: &mut SplitSink<WebSocket, Message>,
    ws_rx: &mut SplitStream<WebSocket>,
    host_id: i64,
    label: &str,
    key_type: &str,
    fingerprint: &str,
    previous: Option<&str>,
) -> Option<bool> {
    let msg = serde_json::to_string(&HostKeyConfirmMsg {
        kind: "hostkey_confirm",
        host_id,
        label,
        key_type,
        fingerprint,
        previous,
    })
    .ok()?;
    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
        return None;
    }

    let deadline = tokio::time::sleep(AUTH_INPUT_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                send_fatal(ws_tx, "等待主机密钥确认超时", Some("input_timeout")).await;
                return None;
            }
            frame = ws_rx.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let s = text.as_str();
                        if !(s.starts_with('{') && s.ends_with('}')) {
                            continue;
                        }
                        match serde_json::from_str::<ClientMsg>(s) {
                            Ok(ClientMsg::HostKeyResponse { trust }) => return Some(trust),
                            Ok(ClientMsg::AuthCancel) => return None,
                            Ok(ClientMsg::Ping) => send_pong(ws_tx).await,
                            _ => {} // 建连阶段忽略其它输入帧
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return None,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("ws recv error during hostkey confirm: {e}");
                        return None;
                    }
                }
            }
        }
    }
}

const PROMPT_BUF_LIMIT: usize = 1024;
const INPUT_BUF_LIMIT: usize = 512;

/* ---------- SSH 会话 cwd 上报注入（OSC 9;9，ConEmu 风格） ----------
 *
 * SFTP 远程栏目录跟随需要终端上报工作目录：本地 PTY 由后端按 shell 注入，
 * SSH 会话默认没有任何上报（远端 shell 除非用户自配 osc7 集成）。前端按
 * 跟随开关在连接时声明 cwd_report，这里在 shell 就绪后向交互会话写入一行
 * 提示符钩子。与本地 PTY 的环境变量预置不同，注入发生在 rc 文件加载完之后
 * （首个提示符处执行），不会被用户 rc 覆盖，且只对声明了 cwd_report 的
 * 会话生效——未开启跟随的会话零变化。
 *
 * 已知代价：写入的一行命令会被回显进输出流，而且不止一次——注入在
 * exec 探测 $SHELL 后立即写入，启动慢的 shell（如 macOS zsh 加载 rc
 * 期间 ZLE 尚未接管 tty）会先被 pty 内核按规范模式回显一次（出现在
 * 首个提示符之前），ZLE/readline 就绪后又把缓冲的注入行重显在提示符
 * 处。后端在转发给前端前用 [`CwdEchoStripper`] 移除前
 * [`MAX_ECHO_OCCURRENCES`] 次出现（容忍折行处插入的 \r\n，超时放弃）。
 * 此外命令经提示符 accept 执行必然多出一轮「提示符→回车→新提示符」，
 * 剥掉回显后表现为会话开头多一条空提示符行：注入行开头的 printf 在
 * 执行时光标上移一行擦掉旧提示符行，让新提示符原地补位（见
 * SSH_*_CWD_LINE 注释），会话开头与未注入的连接观感一致。
 * 行首带空格，HISTCONTROL=ignorespace 的 bash 不会
 * 记入历史。fish/csh 等无法安全注入的 shell 通过 exec 探测 $SHELL 后
 * 跳过（降级为不跟随）；探测用的 exec 无 pty，不打印 MOTD/Last login，
 * 不影响「shell 优先打开」的欢迎信息顺序。
 */

/// 行首的 printf 是提示符行擦除：注入命令被 accept 执行时光标已到新行，
/// 每提示符遥测钩子函数体。首句必须先取 `$?`（钩子链前置函数会改写它）；
/// 上报 OSC 9;10;<exit>——上一条命令的退出码，供 `POST /api/ssh/send/{sid}`
/// 判定命令完成；cwd 变体额外上报 OSC 9;9;<PWD>（目录跟随）。末尾恢复 $?，
/// 对钩子链后续命令与读取 $? 的提示符主题透明。
///
/// 线体必须保持短小：注入行超过终端列宽会折行，行首的「上移一行擦除」
/// 只能清掉最后一行，前面的回显残影会留在用户终端（曾因 255 字符线体
/// 在 ~250 列终端折成两行而露馅）。cwd 上报不用运行时门控变量，直接按
/// 开关生成两个变体，就是为了把线体压回旧版 ~110 字符量级。
const ASHELL_RC_BASH_CWD: &str = concat!(
    "__arc(){ local __s=\"$?\"; ",
    "printf '\\e]9;9;%s\\a\\e]9;10;%s\\a' \"$PWD\" \"$__s\"; ",
    "return \"$__s\"; }",
);
const ASHELL_RC_BASH: &str = concat!(
    "__arc(){ local __s=\"$?\"; ",
    "printf '\\e]9;10;%s\\a' \"$__s\"; ",
    "return \"$__s\"; }",
);
/// zsh 用 print 转义（\e/\a）比 printf 八进制短得多；`$?` 在函数体首条
/// 命令的参数展开时取值，仍是用户命令的退出码
const ASHELL_RC_ZSH_CWD: &str = concat!(
    "__arc(){ local s=$?; ",
    "print -n \"\\e]9;9;$PWD\\e]9;10;$s\\a\"; ",
    "return $s; }",
);
const ASHELL_RC_ZSH: &str = concat!(
    "__arc(){ local s=$?; ",
    "print -n \"\\e]9;10;$s\\a\"; ",
    "return $s; }",
);

/// 先上移擦掉注入所在的旧提示符行，让下一个提示符原地补位——否则剥掉
/// 回显后该行只剩一条空提示符，会话开头多出一行。行首空格使注入行
/// 不进 history（HISTCONTROL=ignorespace）。
/// bash：钩子前插 PROMPT_COMMAND 链并保留 rc 里已有项（含后续用户改动）
fn telemetry_line_bash(cwd_report: bool) -> String {
    format!(
        " printf '\\033[1A\\033[2K\\r'; {rc}; PROMPT_COMMAND=\"__arc;$PROMPT_COMMAND\"\n",
        rc = if cwd_report { ASHELL_RC_BASH_CWD } else { ASHELL_RC_BASH },
    )
}

/// zsh：钩子必须前插 precmd_functions 队首——框架（oh-my-zsh 等）的钩子
/// 会改写 $?，只有队首能取到用户命令的真实退出码
fn telemetry_line_zsh(cwd_report: bool) -> String {
    format!(
        " printf '\\033[1A\\033[2K\\r'; {rc}; precmd_functions=(__arc $precmd_functions)\n",
        rc = if cwd_report { ASHELL_RC_ZSH_CWD } else { ASHELL_RC_ZSH },
    )
}

/// 远端登录 shell 探测命令：第二行取 /bin/sh 软链目标（bash-as-sh 识别；
/// 无 readlink 或非软链时该行为空/原样路径）
const SHELL_PROBE: &str = "echo \"$SHELL\"; readlink -f /bin/sh 2>/dev/null";

/// shell 探测硬超时：堡垒机等对 exec 不回应的服务器上，超时即放弃遥测钩子，
/// 保住后续中继循环的启动（见下方探测处的说明）
const SHELL_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// 重连恢复 sftp 提权的硬超时：提权链路依赖多条 exec（sftp-server 路径探测 /
/// NOPASSWD 探测），对 exec 不回应的设备会挂起。该恢复已在后台任务中执行，
/// 不阻塞终端，但仍需上限避免任务永久悬挂；取值与 SFTP_OPEN_TIMEOUT 同量级
const SFTP_RE_ELEVATE_TIMEOUT: Duration = Duration::from_secs(90);

/// sh 家族（dash/ash/ksh 等无 per-prompt 钩子机制的 POSIX shell）的完成
/// 上报后缀：并入 AI 命令最后一个非空行执行，`$?` 即该命令的退出码。
/// 不独立成行——独立行会多消耗一次提示符周期，用户终端每条 AI 命令后
/// 多出一个空提示符行。回显中的该后缀由中继层无上限剥离器剥掉，终端
/// 只显示命令本身。POSIX printf 保证 dash/ash/ksh 可用。
const MARKER_SUFFIX: &str = "; printf '\\033]9;10;%s\\007' \"$?\"";

/// 后缀无法并入时的退路（末行疑似 heredoc 终止符等）：独立成行注入，
/// 代价是多一次提示符周期（终端多一个空提示符行）。
const SH_MARKER_LINE: &str = " printf '\\033]9;10;%s\\007' \"$?\"\n";

/// 将上报后缀并入 body 最后一个非空命令行。返回 None 表示不可并入：
/// body 无可依附的命令行，或末行疑似 heredoc 终止符（body 含 `<<` 且
/// 末行为裸词——并入会破坏终止符匹配导致 shell 挂起）。其余边角
/// （末行以续行符/悬空操作符收尾等）会产生语法错误 → 无完成事件 →
/// deadline 兜底，可接受。
fn merge_marker(body: &str) -> Option<String> {
    let trimmed = body.trim_end_matches('\n');
    let mut last_idx = None;
    for (i, line) in trimmed.lines().enumerate() {
        if !line.trim().is_empty() {
            last_idx = Some(i);
        }
    }
    let idx = last_idx?;
    let last = trimmed.lines().nth(idx)?;
    let bare_word = !last.trim().is_empty()
        && last.trim().len() <= 32
        && last
            .trim()
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    if body.contains("<<") && bare_word {
        return None;
    }
    // 行尾分号是惰性的，剥掉避免出现 `;;` 语法错误
    let clean = last.trim_end();
    let clean = clean.strip_suffix(';').unwrap_or(clean);
    let mut out = String::with_capacity(trimmed.len() + MARKER_SUFFIX.len());
    for (i, line) in trimmed.lines().enumerate() {
        if i == idx {
            out.push_str(clean);
            out.push_str(MARKER_SUFFIX);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    Some(out)
}

/// marker 会话单次注入的回显剥离目标：后缀并入式取后缀本身（保留命令
/// 文本可见），独立上报行退路取整行；其余注入（悬空半行等）返回 None，
/// 保持现有剥离器不动。
fn marker_strip_target(data: &str) -> Option<&str> {
    let last = data.trim_end_matches('\n').lines().last()?;
    if last.ends_with(MARKER_SUFFIX) {
        Some(MARKER_SUFFIX)
    } else if last.trim() == SH_MARKER_LINE.trim() {
        Some(SH_MARKER_LINE.trim())
    } else {
        None
    }
}

/// 依据登录 shell 与 `/bin/sh` 软链目标决定完成信号方案：
/// zsh/bash（含 /bin/sh→bash 的 bash-as-sh，POSIX 模式下 PROMPT_COMMAND
/// 仍生效）装每提示符遥测钩子；dash/ash/ksh 家族走标记模式（无钩子
/// 机制，但 POSIX `$?` 与 printf 可靠）；csh/fish 等其余 shell 不装。
/// 返回 (钩子注入行, 完成模式)，无钩子时首项为 None。
fn resolve_completion(
    shell: &str,
    binsh: &str,
    cwd_report: bool,
) -> (Option<String>, ssh_svc::CompletionMode) {
    let base = shell.rsplit('/').next().unwrap_or(shell);
    let binsh_base = binsh.rsplit('/').next().unwrap_or(binsh);
    if base.contains("zsh") {
        (
            Some(telemetry_line_zsh(cwd_report)),
            ssh_svc::CompletionMode::Hooked,
        )
    } else if base.contains("bash") || (base == "sh" && binsh_base == "bash") {
        (
            Some(telemetry_line_bash(cwd_report)),
            ssh_svc::CompletionMode::Hooked,
        )
    } else if matches!(base, "sh" | "dash" | "ash" | "ksh" | "mksh" | "lksh" | "yash") {
        (None, ssh_svc::CompletionMode::Marker)
    } else {
        (None, ssh_svc::CompletionMode::None)
    }
}

/// 注入行回显剥离的放弃时限：正常情况下回显在建连后一两秒内出现，
/// 慢 rc 的 shell（macOS zsh）重显也只是再晚几秒。放弃后由
/// [`LooseEchoFilter`] 按标记 token 兜底
const CWD_ECHO_STRIP_TIMEOUT: Duration = Duration::from_secs(20);

/// 单个注入行最多剥离的回显次数：内核规范模式回显（ZLE/readline 接管
/// tty 前的那次）+ ZLE/readline 在提示符处重显，另留重绘余量。注入行仍在
/// ZLE 里 pending 的窗口内（macOS 慢启动 shell 可达数秒）发生的列宽变化
/// （批量连接的 tab 激活切换、窗口调整）都会触发 SIGWINCH 重绘，每一轮
/// 都是一次新的回显出现——预算放宽到 10 次；超预算后放行原始流，由
/// [`LooseEchoFilter`] 接管。
const MAX_ECHO_OCCURRENCES: usize = 10;

/// 在输出流中剥离 cwd 注入行的回显。注入行会被回显多次（见上方模块
/// 注释），前 [`MAX_ECHO_OCCURRENCES`] 次出现全部移除。匹配策略：目标
/// 行前缀必须连续命中（锁定），其后容忍终端按列宽折行插入的 \r\n（有
/// 连续长度上限），命中后顺带吃掉紧随的回车换行。目标行可能分块到达：
/// 转发时只扣留尾部像半截回显开头的字节跨块重试；超时或剥满次数后
/// 原样放行（退化为多显示一行）。
struct CwdEchoStripper {
    /// 注入行原文（不含行尾 \n）
    target: Vec<u8>,
    /// 跨块扣留的输出尾巴
    pending: Vec<u8>,
    deadline: Instant,
    /// 已剥离的回显次数
    stripped: usize,
    /// 最近一次转发给前端的字节：跨块判断命中是否起于行首用
    last_forwarded: Option<u8>,
    done: bool,
    /// 无时限、无次数上限（marker 行会话内每次注入都回显一次，剥离器
    /// 需持续生效；钩子行只在建连时注入一次，才有 20s/10 次预算可言）
    unbounded: bool,
    /// 日志标签（会话 sid）：放弃路径的告警按会话定位
    label: String,
}

enum FindEcho {
    /// 完整命中：[start, end) 为含回显及其回车的待剥区间
    Complete { start: usize, end: usize },
    /// 锁定前缀但目标未走完（分块边界）：从 start 起扣留待续
    Partial { start: usize },
    None,
}

impl CwdEchoStripper {
    fn new(target: &str) -> Self {
        Self::with_timeout(target, CWD_ECHO_STRIP_TIMEOUT)
    }

    /// 无时限、无次数上限的剥离器：供 marker 行回显使用
    fn unbounded(target: &str) -> Self {
        Self {
            target: target.as_bytes().to_vec(),
            pending: Vec::new(),
            deadline: Instant::now(),
            stripped: 0,
            last_forwarded: None,
            done: false,
            unbounded: true,
            label: String::new(),
        }
    }

    /// 取走跨块扣留的尾巴（重置剥离器时冲刷，避免扣留字节丢失）
    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }

    /// 附加日志标签（会话 sid）
    fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    fn with_timeout(target: &str, timeout: Duration) -> Self {
        Self {
            target: target.as_bytes().to_vec(),
            pending: Vec::new(),
            deadline: Instant::now() + timeout,
            stripped: 0,
            last_forwarded: None,
            done: false,
            unbounded: false,
            label: String::new(),
        }
    }

    /// 喂入一块输出，返回应转发给前端的字节
    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        let out = self.feed_inner(data);
        if let Some(&b) = out.last() {
            self.last_forwarded = Some(b);
        }
        out
    }

    fn feed_inner(&mut self, data: &[u8]) -> Vec<u8> {
        if self.done {
            return data.to_vec();
        }
        if !self.unbounded && Instant::now() >= self.deadline {
            self.done = true;
            if self.stripped == 0 {
                // 一次都没命中：回显迟到超时，或流被中继改写导致逐字节
                // 锁不上。扣留尾巴按转义字节记录，供与注入行对照定位失
                // 配形态；残影由宽松兜底过滤器接管（见 LooseEchoFilter）。
                log::warn!(
                    "[{}] telemetry hook echo never matched before deadline, passing raw; held tail: {}",
                    self.label,
                    self.pending.escape_ascii()
                );
            } else {
                log::debug!(
                    "[{}] telemetry hook echo strip deadline reached after {} occurrence(s)",
                    self.label,
                    self.stripped
                );
            }
            let mut out = std::mem::take(&mut self.pending);
            out.extend_from_slice(data);
            return out;
        }
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut out = Vec::with_capacity(buf.len());
        while !buf.is_empty() {
            match Self::find(&buf, &self.target, self.last_forwarded) {
                FindEcho::Complete { start, end } => {
                    self.stripped += 1;
                    if !self.unbounded && self.stripped >= MAX_ECHO_OCCURRENCES {
                        self.done = true;
                        // 预算耗尽：后续重绘回显原样放行，由宽松兜底过滤器接管
                        log::warn!(
                            "[{}] telemetry hook echo strip budget ({}) exhausted, further echoes pass raw",
                            self.label,
                            MAX_ECHO_OCCURRENCES
                        );
                    }
                    out.extend_from_slice(&buf[..start]);
                    buf.drain(..end);
                    if self.done {
                        out.extend_from_slice(&buf);
                        buf.clear();
                    }
                }
                FindEcho::Partial { start } => {
                    out.extend_from_slice(&buf[..start]);
                    self.pending = buf.split_off(start);
                    break;
                }
                FindEcho::None => {
                    // 只扣留尾部可能是半截回显开头的字节，其余立即转发，
                    // 避免多剥离窗口内拖住正常输出的尾巴
                    let hold = Self::holdback_len(&buf, &self.target);
                    let split = buf.len() - hold;
                    out.extend_from_slice(&buf[..split]);
                    self.pending = buf.split_off(split);
                    break;
                }
            }
        }
        out
    }

    fn find(buf: &[u8], target: &[u8], prev_byte: Option<u8>) -> FindEcho {
        const LOCK: usize = 8;
        /// 折行插入的 \r\n 连续字节数上限（列宽再窄一次折行也只有 2 字节）
        const MAX_WRAP_RUN: usize = 8;
        if buf.len() < LOCK || target.len() <= LOCK {
            return FindEcho::None;
        }
        let mut i = 0usize;
        'scan: while i + LOCK <= buf.len() {
            if buf[i..i + LOCK] != target[..LOCK] {
                i += 1;
                continue;
            }
            // 锁定后容忍 \r\n 插入，走完剩余目标
            let (mut k, mut j, mut run) = (i + LOCK, LOCK, 0usize);
            loop {
                if j >= target.len() {
                    // 只有起于行首的命中（内核规范模式回显，独占一行）才
                    // 顺带吃掉紧随的回车换行——那是回显自带的 Enter，留着
                    // 会多一个空行。提示符后开始的命中（ZLE/readline 重显）
                    // 必须保留换行：accept 换行把光标带到下一行，注入命令
                    // 开头的上移擦除才能准确定位旧提示符行。
                    let at_line_start = if i > 0 {
                        buf[i - 1] == b'\r' || buf[i - 1] == b'\n'
                    } else {
                        matches!(prev_byte, Some(b'\r') | Some(b'\n'))
                    };
                    let mut end = k;
                    if at_line_start {
                        let mut eaten = 0usize;
                        while end < buf.len()
                            && eaten < 4
                            && (buf[end] == b'\r' || buf[end] == b'\n')
                        {
                            end += 1;
                            eaten += 1;
                        }
                    }
                    return FindEcho::Complete { start: i, end };
                }
                if k >= buf.len() {
                    return FindEcho::Partial { start: i };
                }
                if buf[k] == target[j] {
                    k += 1;
                    j += 1;
                    run = 0;
                } else if (buf[k] == b'\r' || buf[k] == b'\n') && run < MAX_WRAP_RUN {
                    k += 1;
                    run += 1;
                } else {
                    i += 1;
                    continue 'scan;
                }
            }
        }
        FindEcho::None
    }

    /// buf 尾部与 target 前缀重叠的最长长度（上限 40，且不含完整目标）。
    /// 下限从 2 起：单个尾字节（典型是提示符结尾的空格）不值得扣留——
    /// 注入行是整块写入整块回显的，不会在 1 字节处被分包。
    fn holdback_len(buf: &[u8], target: &[u8]) -> usize {
        let max = buf.len().min(target.len() - 1).min(40);
        (2..=max)
            .rev()
            .find(|&l| &buf[buf.len() - l..] == &target[..l])
            .unwrap_or(0)
    }
}

/// 主剥离器放弃后的宽松兜底：整行逐字节匹配在流被中继改写（插入/删除
/// 字节）或回显迟到（超时放行）时会漏，回显一旦折行还叠加上移擦除只清
/// 最后一行的首行残影。本过滤器在会话开头窗口期内按标记 token
/// `__arc(){`（bash/zsh 变体行体共用）盲扫输出流，命中即吞掉 token 起
/// 到行尾的字节。吞没按内容盲吃：吃满「token 到行尾的最短字节数」后保
/// 留第一个行尾 \r/\n；其前的 \r/\n 视为按列宽折行的插入物一并吞掉（字
/// 节流里与真行尾无法区分，按长度阈值区分）。保留的换行让光标落到下一
/// 行，注入行首句的上移擦除才能定位并清掉提示符行——token 之前的行头
/// 残端随该行一并擦除，无需匹配，这正是逐字节匹配做不到的。上游删字节
/// 类改写会让真行尾提前于阈值：吞没总量超过兜帽即放弃本次原样放行，把
/// 误吞邻接输出的量压在几十字节内。窗口期后原样透传。
struct LooseEchoFilter {
    /// 锚定标记
    token: Vec<u8>,
    /// token 末尾到注入行行尾的最短字节数（不含行尾换行）
    min_tail: usize,
    deadline: Instant,
    /// 跨块扣留的尾部（可能是半截 token 开头）
    pending: Vec<u8>,
    /// 吞没态：Some(已吞字节数)；None 为扫描态
    swallowing: Option<usize>,
    /// 已兜底吞掉的回显次数（首次吞掉时记一条告警，说明主剥离器漏了）
    suppressed: usize,
    label: String,
}

/// 吞没总量兜帽：超过「最短行尾 + 该余量」仍未见行尾换行即放弃本次吞没
const SWALLOW_SLACK: usize = 64;

/// 兜底窗口：主剥离器 20s 超时之外再留迟到回显余量
const LOOSE_FILTER_WINDOW: Duration = Duration::from_secs(60);

impl LooseEchoFilter {
    /// 从注入行构造；行内找不到 token 时返回 None（两种变体行体都含该
    /// 标记，不该发生）
    fn new(target: &str, label: &str) -> Option<Self> {
        Self::with_window(target, label, LOOSE_FILTER_WINDOW)
    }

    fn with_window(target: &str, label: &str, window: Duration) -> Option<Self> {
        const TOKEN: &[u8] = b"__arc(){";
        let bytes = target.as_bytes();
        let off = bytes.windows(TOKEN.len()).position(|w| w == TOKEN)?;
        Some(Self {
            token: TOKEN.to_vec(),
            min_tail: bytes.len() - off - TOKEN.len(),
            deadline: Instant::now() + window,
            pending: Vec::new(),
            swallowing: None,
            suppressed: 0,
            label: label.to_string(),
        })
    }

    /// 喂入一块（已经过主剥离器的）输出，返回应转发的字节
    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        // 窗口关闭且无进行中的吞没：冲刷扣留尾巴，此后原样透传
        if self.swallowing.is_none() && Instant::now() >= self.deadline {
            let mut out = std::mem::take(&mut self.pending);
            out.extend_from_slice(data);
            return out;
        }
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut out = Vec::with_capacity(buf.len());
        let mut i = 0usize;
        while i < buf.len() {
            match self.swallowing {
                Some(eaten) => {
                    let b = buf[i];
                    i += 1;
                    if eaten >= self.min_tail && (b == b'\r' || b == b'\n') {
                        // 真行尾：保留换行，注入行首句的上移擦除依赖它定位
                        out.push(b);
                        self.swallowing = None;
                        self.suppressed += 1;
                        if self.suppressed == 1 {
                            log::warn!(
                                "[{}] loose echo filter engaged: strict stripper missed the hook echo",
                                self.label
                            );
                        }
                    } else if eaten >= self.min_tail + SWALLOW_SLACK {
                        // 兜帽：放弃本次吞没，原样放行（残影退化）
                        out.push(b);
                        self.swallowing = None;
                    } else {
                        self.swallowing = Some(eaten + 1);
                    }
                }
                None => {
                    if buf[i..].starts_with(&self.token) {
                        i += self.token.len();
                        self.swallowing = Some(0);
                    } else {
                        out.push(buf[i]);
                        i += 1;
                    }
                }
            }
        }
        // 扫描态下扣留尾部半截 token 开头（≥2 字节），跨块补全后整段吞
        // 掉；吞没态盲吃无前缀匹配问题，不扣留
        if self.swallowing.is_none() {
            let hold = (2..=buf.len().min(self.token.len() - 1))
                .rev()
                .find(|&l| buf[buf.len() - l..] == self.token[..l]);
            if let Some(l) = hold {
                let split = out.len().saturating_sub(l);
                self.pending = out.split_off(split);
            }
        }
        out
    }
}

/// 去掉终端输出里的 ANSI 控制序列，让 sudo 提示识别不被颜色/光标序列打断。
fn sanitize_prompt_text(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    let mut escaped = false;
                    for c in chars.by_ref() {
                        if c == '\x07' {
                            break;
                        }
                        if escaped && c == '\\' {
                            break;
                        }
                        escaped = c == '\x1b';
                    }
                }
                _ => {}
            },
            '\r' => out.push('\n'),
            '\x08' => {
                out.pop();
            }
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

fn push_limited(buf: &mut Vec<u8>, data: &[u8], limit: usize) {
    buf.extend_from_slice(data);
    if buf.len() > limit {
        let skip = buf.len() - limit;
        buf.drain(0..skip);
    }
}

fn reset_idle(timer: &mut std::pin::Pin<&mut tokio::time::Sleep>, dur: Duration) {
    timer.as_mut().reset(tokio::time::Instant::now() + dur);
}

fn tail_chars(text: &str, max: usize) -> String {
    text.chars()
        .rev()
        .take(max)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// 最近键盘输入里是否出现过提权命令。用于兼容 `Password:`、`doas`、
/// `sudo -p 'pwd: '` 这类不一定带 `[sudo] password for` 的提示。
fn recent_input_requests_elevation(input_buf: &[u8]) -> bool {
    let text = sanitize_prompt_text(input_buf);
    let tail = tail_chars(&text, 320).to_lowercase();
    tail.rsplit('\n').take(6).any(|line| {
        line.split_whitespace()
            .any(|token| token == "sudo" || token == "sudoedit" || token == "doas")
    })
}

/// 判断输出尾部是否像 sudo/doas 密码提示。
///
/// 兼容：
/// - 默认 `[sudo] password for user:`
/// - 带 ANSI/颜色序列的提示
/// - 本地化提示（中文“密码”、西/法/德/葡/俄等 password 词）
/// - `Password:` / `doas ... password:` / `sudo -p` 自定义短提示（要求最近输入有 sudo/doas）
fn detect_sudo_password_prompt(output_buf: &[u8], input_buf: &[u8]) -> bool {
    let text = sanitize_prompt_text(output_buf);
    let tail = tail_chars(&text, 320);
    let normalized = tail
        .trim_end_matches(|c| c == '\n' || c == '\r' || c == ' ')
        .to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    let last_line = normalized
        .rsplit('\n')
        .next()
        .unwrap_or(normalized.as_str())
        .trim_end();
    if last_line.is_empty() {
        return false;
    }

    let ends_prompt = last_line.ends_with(':') || last_line.ends_with('：');
    if !ends_prompt {
        return false;
    }

    let mentions_elevation = last_line.contains("sudo") || last_line.contains("doas");
    let mentions_password = [
        "password",
        "密码",
        "contraseña",
        "mot de passe",
        "passwort",
        "senha",
        "пароль",
    ]
    .iter()
    .any(|word| last_line.contains(word));

    if mentions_elevation && mentions_password {
        return true;
    }

    let elevated = recent_input_requests_elevation(input_buf);
    if elevated && mentions_password {
        return true;
    }

    // `sudo -p 'pwd: '` 这类极短自定义提示：只在刚执行过 sudo/doas 时接受，
    // 避免把普通冒号结尾输出误判成密码提示。
    elevated && last_line.chars().count() <= 48
}

async fn run_terminal(
    socket: WebSocket,
    state: AppState,
    host_id: i64,
    q: TerminalQuery,
    sid: &str,
) -> anyhow::Result<()> {
    // 0) 先拆分 socket：SSH 认证失败时需要在建连完成前与前端交互（重新输入密码）
    let (mut ws_tx, mut ws_rx) = socket.split();

    // 1) 取主机并解密凭证
    let mut host =
        match service::host::get_with_credentials(&state.db, &state.config.crypto_key, host_id)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                send_fatal(&mut ws_tx, &format!("加载主机配置失败: {e}"), Some("load_host")).await;
                anyhow::bail!("load host: {e}");
            }
        };

    // 跳板机凭证缓存：认证失败重试时原地更新，避免每轮重新解密加载
    let mut jump_host: Option<Host> = match host.jump_host_id {
        Some(jid) => {
            match service::host::get_with_credentials(&state.db, &state.config.crypto_key, jid)
                .await
            {
                Ok(j) => Some(j),
                Err(e) => {
                    send_fatal(
                        &mut ws_tx,
                        &format!("加载跳板机配置失败: {e}"),
                        Some("load_host"),
                    )
                    .await;
                    anyhow::bail!("load jump host: {e}");
                }
            }
        }
        None => None,
    };

    // 2) 建立 SSH session 并注册到全局（配置了跳板机时经 direct-tcpip 双跳）。
    // 主机密钥未确认时向前端弹窗核对指纹，信任写库后重试；认证失败（典型场景：
    // 服务器上改了密码，本地存的还是旧密码）时向前端弹窗请求新密码，可选记住。
    // attempts 只计认证重试（防暴力猜密码），主机密钥确认不消耗次数。
    let mut attempts: u32 = 0;
    // keyboard-interactive（2FA/TOTP）提示的按位置应答序列：弹窗收集后随
    // 重连重放。跳板机与目标机各自可能要求交互，两序列独立累积——单槽
    // 会互相覆盖，双端都要交互时永远无法收敛；密码重输后旧应答序列失效
    let mut kbd_jump: Vec<String> = Vec::new();
    let mut kbd_target: Vec<String> = Vec::new();
    // 建连阶段旁路通道：connect_prepared 内部同步上报阶段变化（TCP/握手/认证），
    // 这里在 select 中转成 ws progress 帧——对建连 future 只轮询不取消，
    // 保证 select 等待进度时不会中断正在进行的建连
    let (stage_tx, mut stage_rx) = watch::channel(ssh_svc::ConnectStage::Tcp);
    let stage_reporter_impl = |stage| {
        let _ = stage_tx.send(stage);
    };
    let stage_reporter: &ssh_svc::StageReporter<'_> = &stage_reporter_impl;
    let session_arc = loop {
        let (jump_kbd, target_kbd): (&[String], &[String]) =
            (kbd_jump.as_slice(), kbd_target.as_slice());
        // 块作用域内完成建连轮询：future 持有 jump_host/host/kbd_* 的借用，
        // 出块即释放，后续 match 分支才能原地更新这些凭证
        let connect_result = {
            let connect_fut = Session::connect_prepared(
                &state.db,
                jump_host.as_ref(),
                &host,
                jump_kbd,
                target_kbd,
                stage_reporter,
            );
            tokio::pin!(connect_fut);
            let mut stages_alive = true;
            // 建连总超时：TCP 阶段已有 15s 限时，这里兜底握手/认证悬挂。
            // deadline 固定，sleep_until 分支到点即触发；认证的用户交互发生在
            // connect_prepared 返回之后的弹窗循环里，不占此处预算
            let connect_deadline = tokio::time::Instant::now() + CONNECT_TOTAL_TIMEOUT;
            loop {
                tokio::select! {
                    res = &mut connect_fut => break res,
                    _ = tokio::time::sleep_until(connect_deadline) => {
                        break Err(AppError::Ssh(format!(
                            "connect: timed out after {}s overall",
                            CONNECT_TOTAL_TIMEOUT.as_secs()
                        )));
                    }
                    changed = stage_rx.changed(), if stages_alive => {
                        match changed {
                            Ok(()) => {
                                let stage = *stage_rx.borrow_and_update();
                                send_progress(&mut ws_tx, stage.as_str()).await;
                            }
                            // 发送端随闭包存活到建连结束，不会 drop；防御性停用分支避免忙轮询
                            Err(_) => stages_alive = false,
                        }
                    }
                }
            }
        };
        match connect_result
        {
            Ok(s) => break Arc::new(s),
            Err(AppError::HostKeyVerify {
                host_id: failed_id,
                addr,
                port,
                key_type,
                fingerprint,
                previous,
            }) => {
                let label = format!("{addr}:{port}");
                match request_hostkey_confirm(
                    &mut ws_tx,
                    &mut ws_rx,
                    failed_id,
                    &label,
                    &key_type,
                    &fingerprint,
                    previous.as_deref(),
                )
                .await
                {
                    Some(true) => {
                        if let Err(e) = service::known_host::trust(
                            &state.db,
                            &addr,
                            port,
                            &key_type,
                            &fingerprint,
                        )
                        .await
                        {
                            send_fatal(&mut ws_tx, &format!("保存主机指纹失败: {e}"), None).await;
                            anyhow::bail!("save trusted host key failed: {e}");
                        }
                        log::info!("host key trusted for {label} ({key_type})");
                        // 信任写库后继续循环重试建连
                    }
                    _ => {
                        send_fatal(&mut ws_tx, "主机密钥未被信任，连接已取消", Some("hostkey_rejected")).await;
                        anyhow::bail!("host key rejected for {label}");
                    }
                }
            }
            Err(AppError::AuthFailed { host_id: failed_id }) => {
                attempts += 1;
                if attempts >= MAX_AUTH_ATTEMPTS {
                    send_fatal(&mut ws_tx, "多次认证失败，已停止重试", Some("auth_exhausted")).await;
                    anyhow::bail!("ssh auth failed after {attempts} attempts");
                }
                let is_jump = jump_host.as_ref().is_some_and(|j| j.id == failed_id);
                let target: &mut Host = if is_jump {
                    jump_host.as_mut().unwrap()
                } else {
                    &mut host
                };
                let label = format!("{}@{}:{}", target.username, target.addr, target.port);
                match request_new_password(&mut ws_tx, &mut ws_rx, failed_id, &label).await {
                    Some((pwd, remember)) => {
                        if remember {
                            if let Err(e) = service::host::update_password_only(
                                &state.db,
                                &state.config.crypto_key,
                                failed_id,
                                &pwd,
                            )
                            .await
                            {
                                log::warn!("save updated password for host {failed_id}: {e}");
                            }
                        }
                        target.password = Some(pwd);
                        // 密码已变更：此前收集的 keyboard-interactive 应答序列失效
                        kbd_jump.clear();
                        kbd_target.clear();
                    }
                    None => {
                        send_fatal(&mut ws_tx, "认证已取消", Some("auth_cancelled")).await;
                        anyhow::bail!("auth cancelled by user");
                    }
                }
            }
            Err(AppError::KeyboardInteractiveRequired {
                host_id: failed_id,
                name,
                instructions,
                prompts,
                given,
            }) => {
                attempts += 1;
                if attempts >= MAX_AUTH_ATTEMPTS {
                    send_fatal(&mut ws_tx, "多次认证失败，已停止重试", Some("auth_exhausted")).await;
                    anyhow::bail!("ssh keyboard-interactive failed after {attempts} attempts");
                }
                let is_jump = jump_host.as_ref().is_some_and(|j| j.id == failed_id);
                let target: &mut Host = if is_jump {
                    jump_host.as_mut().unwrap()
                } else {
                    &mut host
                };
                let label = format!("{}@{}:{}", target.username, target.addr, target.port);
                match request_kbd_answers(
                    &mut ws_tx,
                    &mut ws_rx,
                    failed_id,
                    &label,
                    &name,
                    &instructions,
                    &prompts,
                )
                .await
                {
                    Some(answers) => {
                        // 完整重放序列 = 已自动应答项（密码式提示）+ 用户输入，
                        // 与本次连接的提示出现顺序一一对应，按归属机器独立累积
                        let mut full = given;
                        full.extend(answers);
                        if is_jump {
                            kbd_jump = full;
                        } else {
                            kbd_target = full;
                        }
                    }
                    None => {
                        send_fatal(&mut ws_tx, "认证已取消", Some("auth_cancelled")).await;
                        anyhow::bail!("keyboard-interactive auth cancelled by user");
                    }
                }
            }
            Err(AppError::KeyPassphraseRequired { host_id: failed_id }) => {
                attempts += 1;
                if attempts >= MAX_AUTH_ATTEMPTS {
                    send_fatal(&mut ws_tx, "多次认证失败，已停止重试", Some("auth_exhausted")).await;
                    anyhow::bail!("ssh key passphrase failed after {attempts} attempts");
                }
                let is_jump = jump_host.as_ref().is_some_and(|j| j.id == failed_id);
                let target: &mut Host = if is_jump {
                    jump_host.as_mut().unwrap()
                } else {
                    &mut host
                };
                let label = format!("{}@{}:{}", target.username, target.addr, target.port);
                let hint = if target.key_passphrase.is_some() {
                    "passphrase 不正确或已变更，请重新输入"
                } else {
                    "该私钥已加密，请输入 passphrase"
                };
                match request_new_passphrase(&mut ws_tx, &mut ws_rx, failed_id, &label, hint).await
                {
                    Some((passphrase, remember)) => {
                        if remember {
                            if let Err(e) = service::host::update_key_passphrase_only(
                                &state.db,
                                &state.config.crypto_key,
                                failed_id,
                                &passphrase,
                            )
                            .await
                            {
                                log::warn!(
                                    "save updated key passphrase for host {failed_id}: {e}"
                                );
                            }
                        }
                        target.key_passphrase = Some(passphrase);
                    }
                    None => {
                        send_fatal(&mut ws_tx, "认证已取消", Some("auth_cancelled")).await;
                        anyhow::bail!("key passphrase prompt cancelled by user");
                    }
                }
            }
            Err(e) => {
                // 未连上的建连失败（拒绝/超时/DNS 等）：分类后下发 fatal 帧，
                // 前端据此做 i18n 全局提示并保留标签页，而不是无声地关掉
                let code = classify_connect_failure(&e);
                // AppError::Ssh 的 Display 自带 "ssh error: " 前缀，剥掉避免与"连接失败"叠词
                let detail = e.to_string();
                let detail = detail.trim_start_matches("ssh error: ");
                send_fatal(&mut ws_tx, &format!("连接失败：{detail}"), code).await;
                return Err(anyhow::anyhow!("ssh connect: {e}"));
            }
        }
    };
    // 把 sid 写入 client handler，使远程转发回连能路由到本会话
    session_arc.attach_sid(sid).await;
    ssh_svc::set_client(sid.to_string(), session_arc.clone()).await;
    log::info!("[{sid}] ssh session established (host {host_id})");

    // 认证前服务器横幅（SSH_MSG_USERAUTH_BANNER）：转发给前端在终端展示，
    // 与 ssh CLI 在输密码前显示的行为对齐
    if let Some(banner) = session_arc.take_banner().await {
        let msg = serde_json::json!({ "kind": "banner", "text": banner }).to_string();
        let _ = ws_tx.send(Message::Text(msg.into())).await;
    }

    // 3) 打开终端 channel 并请求 shell。必须先于 SFTP 子通道：OpenSSH 只对同一
    //    连接上的第一个会话打印 Last login/MOTD，先开 sftp 子系统会让 shell 会话
    //    的欢迎信息（Last login / uname / Welcome...）整体消失，提示符前一片空白
    //    （阿里云 Debian 13 实测复现）。
    send_progress(&mut ws_tx, "shell").await;
    let mut channel = match session_arc.channel_open_session().await {
        Ok(c) => c,
        Err(e) => {
            send_fatal(
                &mut ws_tx,
                &format!("打开终端会话失败: {e}"),
                Some("open_session"),
            )
            .await;
            anyhow::bail!("open session channel: {e}");
        }
    };
    if let Err(e) = channel
        .request_pty(false, &q.term, q.cols, q.rows, 0, 0, &[])
        .await
    {
        send_fatal(
            &mut ws_tx,
            &format!("打开终端会话失败: {e}"),
            Some("open_session"),
        )
        .await;
        anyhow::bail!("request pty: {e}");
    }
    if let Err(e) = channel.request_shell(true).await {
        send_fatal(
            &mut ws_tx,
            &format!("打开终端会话失败: {e}"),
            Some("open_session"),
        )
        .await;
        anyhow::bail!("request shell: {e}");
    }

    // 4) sftp 子通道预热：放后台任务执行。open_sftp 在堡垒机上可能很慢
    //    （GateShell 的 sftp 是惰性仿真代理，后续会话初始化实测 >10s）甚至
    //    挂死（对 subsystem 请求不回应的设备），同步执行会拖住 ready 与
    //    中继循环；预热晚到期间 SFTP 面板列表报未开，比整条终端卡死好得多，
    //    挂死场景由 open_sftp 内部的 SFTP_OPEN_TIMEOUT 兜底。
    // 重连复用同一 sid：先记住此前的提权状态，open_sftp 会把它覆盖成普通会话，
    // 随后在后台任务里恢复提权。
    let sftp_was_elevated = ssh_svc::get_sftp_elevated(sid).await;

    // 创建终端命令注入 / 输出广播通道（供 POST /api/ssh/send/{sid} 使用）
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    let (output_tx, _) = broadcast::channel::<String>(64);
    ssh_svc::set_terminal_channels(sid, cmd_tx, output_tx.clone()).await;

    // 把 sid 通知客户端
    let ready = serde_json::to_string(&ReadyMsg { kind: "ready", sid })?;
    let _ = ws_tx.send(Message::Text(ready.into())).await;
    log::debug!("[{sid}] terminal ready sent, probing shell before relay loop");

    // sudo 密码自动填充：检测到密码提示时通知前端，
    // 前端拦截回车发送 sudo_fill，后端注入已保存的密码。
    let sudo_password = host.password.clone();

    // sftp 预热 + 重连提权恢复（后台任务，见上方第 4 步说明）：
    // 该 sid 此前是提权会话时，用主机密码重新提权（NOPASSWD 时
    // open_sftp_elevated 内部走免密分支）。失败仅降级为普通会话并记日志；
    // 提权链路依赖多条 exec，对 exec 不回应的设备由 SFTP_RE_ELEVATE_TIMEOUT 兜底。
    // 堡垒机同样预热：GateShell 类网关的 sftp 根目录是主机列表，进入主机
    // 目录即可代理目标机文件，面板按此手动导航（不随终端选中的主机联动）。
    {
        let warm_session = Arc::clone(&session_arc);
        let warm_sid = sid.to_string();
        let warm_password = sudo_password.clone();
        tokio::spawn(async move {
            // 先取提权目标用户：open_sftp 会清掉映射，恢复须在清除前捕获
            let elevate_user = ssh_svc::get_sftp_elevate_user(&warm_sid).await;
            if let Err(e) = warm_session.open_sftp(&warm_sid).await {
                log::warn!("open sftp for sid={warm_sid} failed: {e}");
            }
            if sftp_was_elevated {
                let re_elevate = tokio::time::timeout(
                    SFTP_RE_ELEVATE_TIMEOUT,
                    warm_session.open_sftp_elevated(
                        &warm_sid,
                        warm_password.as_deref(),
                        elevate_user.as_deref(),
                        None,
                    ),
                )
                .await;
                match re_elevate {
                    Ok(Ok(_)) => {
                        log::info!("sftp session {warm_sid} re-elevated after reconnect")
                    }
                    Ok(Err(e)) => {
                        log::warn!("re-elevate sid={warm_sid} failed: {e}; sftp stays non-elevated")
                    }
                    Err(_) => log::warn!(
                        "[{warm_sid}] sftp re-elevate timed out after {SFTP_RE_ELEVATE_TIMEOUT:?}; \
                         sftp stays non-elevated"
                    ),
                }
            }
        });
    }

    // 每提示符遥测钩子 / 完成标记：探测远端登录 shell（exec 无 pty，不影响
    // 欢迎信息），按 shell 家族决定完成信号方案（见 resolve_completion）。
    // 经 cmd_tx 写入：中继循环启动后它是写进交互 channel 的第一笔数据，
    // 此时用户尚未来得及输入，不会与用户命令行交错。
    //
    // 探测必须带硬超时：部分堡垒机/appliance（GateShell 实测）对 exec 请求
    // 既不回 channel_success 也不关通道，execute 会永久挂起——而探测位于
    // 中继循环启动之前，挂起等于整条转发链路（shell 输出 + 键盘输入）从未
    // 启动，表现为「认证成功但终端全空白」。超时即放弃遥测钩子（降级
    // CompletionMode::None）；探针跑在独立通道上，弃置它不影响交互通道。
    let mut cwd_echo_strip: Option<CwdEchoStripper> = None;
    let mut loose_echo_filter: Option<LooseEchoFilter> = None;
    let mut marker_mode = false;
    // 堡垒机会话跳过探测与遥测注入：exec 与注入行都落在堡垒机的菜单 shell 上
    // （目标机 shell 在嵌套中继之内，钩子本就够不着），注入反而会往菜单 TUI
    // 打垃圾输入。命令完成检测随之按无遥测降级（与 exec 不响应的设备同路径）。
    if !host.bastion {
        let probe =
            tokio::time::timeout(SHELL_PROBE_TIMEOUT, session_arc.execute(SHELL_PROBE)).await;
        match probe {
            Ok(Ok(res)) => {
                let mut lines = res.stdout.trim().lines();
                let shell = lines.next().unwrap_or("").trim();
                let binsh = lines.next().unwrap_or("").trim();
                let (line, mode) = resolve_completion(shell, binsh, q.cwd_report);
                marker_mode = mode == ssh_svc::CompletionMode::Marker;
                if mode != ssh_svc::CompletionMode::None {
                    ssh_svc::set_terminal_completion(sid, mode).await;
                }
                if let Some(line) = line {
                    match ssh_svc::get_terminal_sender(sid).await {
                        Ok(sender) => {
                            let _ = sender.send(line.clone());
                            // 注入行会被回显进输出流：武装剥离器，转发前移除其首次出现
                            let trimmed = line.trim_end_matches('\n');
                            cwd_echo_strip = Some(CwdEchoStripper::new(trimmed).with_label(sid));
                            // 宽松兜底：整行逐字节匹配在流被中继改写或回显迟到
                            // 时会漏，按标记 token 盲吞兜住（见 LooseEchoFilter）
                            loose_echo_filter = LooseEchoFilter::new(trimmed, sid);
                        }
                        Err(e) => log::debug!("terminal sender missing for telemetry hook: {e}"),
                    }
                }
                // marker 会话不在此处武装：剥离目标随每次注入形态不同（后缀
                // 并入式/独立上报行退路），由 cmd_rx 写入时按需重置
            }
            Ok(Err(e)) => log::debug!("detect remote shell for telemetry hook failed: {e}"),
            Err(_) => {
                log::warn!(
                    "[{sid}] shell probe timed out after {SHELL_PROBE_TIMEOUT:?}; \
                     telemetry hooks skipped (exec likely unsupported by server)"
                );
            }
        }
    }

    // 会话中断原因（session_end 帧下发给前端展示）：ExitStatus=shell 正常退出，
    // channel 收发错误按 russh 错误细分（心跳超时/服务器断开/网络错误），
    // 前端主动关 ws 的路径置 None 不发帧
    let mut session_end_code: Option<&'static str> = None;
    let mut sudo_buf: Vec<u8> = Vec::with_capacity(PROMPT_BUF_LIMIT);
    let mut terminal_input_buf: Vec<u8> = Vec::with_capacity(INPUT_BUF_LIMIT);
    // 广播（AI 收集副本）的 UTF-8 尾缓冲：多字节字符跨块时扣留待续，
    // 终端 WS 路径仍走原始字节，不受影响
    let mut utf8_tail: Vec<u8> = Vec::new();

    // idle 定时发送空字符保活：配置了 idle_send_interval > 0 时启用，
    // 每隔该秒数向 PTY 发送 \x00（对终端无害，可防止 NAT/防火墙超时断连）。
    // 注意：禁用时不能给超大 Duration 充当"永不触发"，tokio sleep / reset
    // 内部会算 Instant + duration，超出 Instant 表示范围会直接 panic
    // （overflow when adding duration to instant），表现为会话建立后瞬间
    // WebSocket 1006 断开。select 分支有 `if idle_secs > 0` 守卫，禁用时
    // 该 timer 永不触发，给任意安全占位时长即可。
    let idle_secs = host.idle_send_interval.unwrap_or(0);
    let idle_dur = Duration::from_secs(if idle_secs > 0 { idle_secs as u64 } else { 1 });
    let idle_timer = tokio::time::sleep(idle_dur);
    tokio::pin!(idle_timer);

    let closed = false;
    loop {
        tokio::select! {
            // 外部命令注入（POST /api/ssh/send/{sid}）
            Some(data) = cmd_rx.recv() => {
                if let Err(e) = channel.data(data.as_bytes()).await {
                    session_end_code = Some(classify_session_end_err(&e));
                    break;
                }
                // marker 会话：按本次注入形态重置回显剥离目标——后缀并入式
                // 只剥 `; printf …` 后缀（命令文本保留可见），独立上报行退路
                // 剥整行
                if marker_mode {
                    if let Some(target) = marker_strip_target(&data) {
                        if let Some(old) = cwd_echo_strip.as_mut() {
                            let pending = old.take_pending();
                            if !pending.is_empty() {
                                let _ = output_tx.send(String::from_utf8_lossy(&pending).to_string());
                                if ws_tx.send(Message::Binary(pending.into())).await.is_err() { break; }
                            }
                        }
                        cwd_echo_strip = Some(CwdEchoStripper::unbounded(target));
                    }
                }
                reset_idle(&mut idle_timer, idle_dur);
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let s = text.as_str();
                        if s.starts_with('{') && s.ends_with('}') {
                            if let Ok(cm) = serde_json::from_str::<ClientMsg>(s) {
                                match cm {
                                    ClientMsg::Cmd { data } => {
                                        push_limited(
                                            &mut terminal_input_buf,
                                            data.as_bytes(),
                                            INPUT_BUF_LIMIT,
                                        );
                                        if let Err(e) = channel.data(data.as_bytes()).await {
                                            session_end_code = Some(classify_session_end_err(&e));
                                            break;
                                        }
                                        reset_idle(&mut idle_timer, idle_dur);
                                    }
                                    ClientMsg::Resize { cols, rows } => {
                                        let _ = channel.window_change(cols, rows, 0, 0).await;
                                    }
                                    ClientMsg::Ping => {
                                        let _ = ws_tx.send(Message::Text("{\"kind\":\"pong\"}".to_string().into())).await;
                                    }
                                    ClientMsg::SudoFill => {
                                        if let Some(pwd) = sudo_password.as_deref() {
                                            if !pwd.is_empty() {
                                                let payload = format!("{pwd}\r");
                                                if let Err(e) = channel.data(payload.as_bytes()).await {
                                                    session_end_code = Some(classify_session_end_err(&e));
                                                    break;
                                                }
                                                reset_idle(&mut idle_timer, idle_dur);
                                            }
                                        }
                                    }
                                    // 会话已建立后迟到的认证/主机密钥帧：忽略
                                    ClientMsg::AuthResponse { .. } | ClientMsg::AuthCancel => {}
                                    ClientMsg::KbdAuthResponse { .. } => {}
                                    ClientMsg::HostKeyResponse { .. } => {}
                                }
                            } else if let Err(e) = channel.data(s.as_bytes()).await {
                                session_end_code = Some(classify_session_end_err(&e));
                                break;
                            } else {
                                reset_idle(&mut idle_timer, idle_dur);
                            }
                        } else if let Err(e) = channel.data(s.as_bytes()).await {
                            session_end_code = Some(classify_session_end_err(&e));
                            break;
                        } else {
                            reset_idle(&mut idle_timer, idle_dur);
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        if let Err(e) = channel.data(&b[..]).await {
                            session_end_code = Some(classify_session_end_err(&e));
                            break;
                        }
                        reset_idle(&mut idle_timer, idle_dur);
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::warn!("ws recv error sid={sid}: {e}");
                        break;
                    }
                }
            }
            ssh_msg = channel.wait() => {
                match ssh_msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        log::debug!("[{sid}] ssh→ws {}B", data.len());
                        // cwd 注入行回显剥离：未武装时原样透传
                        let bytes = match cwd_echo_strip.as_mut() {
                            Some(s) => s.feed(data),
                            None => data.to_vec(),
                        };
                        // 宽松兜底过滤器接管主剥离器漏掉的回显（迟到/预算
                        // 耗尽/流被改写）
                        let bytes = match loose_echo_filter.as_mut() {
                            Some(f) => f.feed(&bytes),
                            None => bytes,
                        };
                        let _ = output_tx.send(util::utf8_safe(&mut utf8_tail, &bytes));
                        if !bytes.is_empty()
                            && ws_tx
                                .send(Message::Binary(bytes.clone().into()))
                                .await
                                .is_err()
                        {
                            break;
                        }

                        push_limited(&mut sudo_buf, &bytes, PROMPT_BUF_LIMIT);
                        if detect_sudo_password_prompt(&sudo_buf, &terminal_input_buf)
                            && sudo_password.as_deref().is_some_and(|p| !p.is_empty())
                        {
                            sudo_buf.clear();
                            // 命中后必须同时清空输入缓冲：否则旧 sudo 命令一直留在
                            // 最近输入窗口里，本次提权完成后命令输出中普通的冒号
                            // 结尾行（apt 的 "The following ... installed:" 等）会被
                            // 反复误判成新的密码提示，前端跟着反复弹出。
                            terminal_input_buf.clear();
                            let _ = ws_tx.send(Message::Text(
                                "{\"kind\":\"sudo_prompt\"}".to_string().into(),
                            )).await;
                        }
                        reset_idle(&mut idle_timer, idle_dur);
                    }
                    Some(ChannelMsg::ExtendedData { ref data, ext: _ }) => {
                        let bytes = data.to_vec();
                        let _ = output_tx.send(util::utf8_safe(&mut utf8_tail, &bytes));
                        if ws_tx.send(Message::Binary(bytes.clone().into())).await.is_err() { break; }

                        // 部分服务端/程序会把密码提示写到 stderr；这里同样参与识别。
                        push_limited(&mut sudo_buf, &bytes, PROMPT_BUF_LIMIT);
                        if detect_sudo_password_prompt(&sudo_buf, &terminal_input_buf)
                            && sudo_password.as_deref().is_some_and(|p| !p.is_empty())
                        {
                            sudo_buf.clear();
                            // 命中后必须同时清空输入缓冲：否则旧 sudo 命令一直留在
                            // 最近输入窗口里，本次提权完成后命令输出中普通的冒号
                            // 结尾行（apt 的 "The following ... installed:" 等）会被
                            // 反复误判成新的密码提示，前端跟着反复弹出。
                            terminal_input_buf.clear();
                            let _ = ws_tx.send(Message::Text(
                                "{\"kind\":\"sudo_prompt\"}".to_string().into(),
                            )).await;
                        }
                        reset_idle(&mut idle_timer, idle_dur);
                    }
                    Some(ChannelMsg::ExitStatus { exit_status: _ }) => {
                        // shell 正常退出（用户 exit / 命令结束），非异常断开。
                        // 此分支会主动关闭 ws，session_end 帧必须先于 close 发出
                        session_end_code = Some("session_exited");
                        let _ = ws_tx
                            .send(Message::Text(
                                "{\"kind\":\"session_end\",\"code\":\"session_exited\"}".into(),
                            ))
                            .await;
                        if !closed {
                            let _ = channel.eof().await;
                            let _ = ws_tx.close().await;
                            let _ = channel.close().await;
                        }
                        break;
                    }
                    // 服务器关闭通道且无 exit status：shell 被杀或服务端主动断开
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        session_end_code = Some("channel_closed");
                        break;
                    }
                    _ => {}
                }
            }
            _ = &mut idle_timer, if idle_secs > 0 => {
                if let Err(e) = channel.data(&[0u8][..]).await {
                    session_end_code = Some(classify_session_end_err(&e));
                    break;
                }
                reset_idle(&mut idle_timer, idle_dur);
            }
        }
    }

    // 冲刷 UTF-8 尾缓冲：最后一块可能停在多字节字符中间
    let tail_text = util::utf8_flush(&mut utf8_tail);
    if !tail_text.is_empty() {
        let _ = output_tx.send(tail_text);
    }

    // 会话中断原因下发（session_exited 已在 ExitStatus 分支先行发送）：
    // 前端据此展示本地化断开原因（心跳超时/服务器断开/网络错误/通道关闭），
    // 替代过去只能看到 "连接已关闭（code=1006）" 的盲区
    if let Some(code) = session_end_code {
        if code != "session_exited" {
            let payload = serde_json::json!({ "kind": "session_end", "code": code }).to_string();
            let _ = ws_tx.send(Message::Text(payload.into())).await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_default_sudo_prompt() {
        let output = b"user@host:~$ sudo -l\r\n[sudo] password for user: ";
        assert!(detect_sudo_password_prompt(output, b"sudo -l\r"));
    }

    #[test]
    fn detects_prompt_with_ansi_sequences() {
        let output = b"\x1b[1;32muser@host\x1b[0m:~$ \x1b[36m[sudo] password for user:\x1b[0m ";
        assert!(detect_sudo_password_prompt(output, b""));
    }

    #[test]
    fn detects_localized_sudo_prompt() {
        let output = "[sudo] user 的密码：".as_bytes();
        assert!(detect_sudo_password_prompt(output, b""));
    }

    #[test]
    fn detects_generic_password_prompt_only_after_elevation_command() {
        let output = b"Password: ";
        assert!(detect_sudo_password_prompt(
            output,
            b"sudo systemctl restart sshd\r"
        ));
        assert!(!detect_sudo_password_prompt(output, b"mysql -u root -p\r"));
    }

    #[test]
    fn detects_short_custom_sudo_prompt() {
        let output = b"pwd: ";
        assert!(detect_sudo_password_prompt(output, b"sudo -p 'pwd: ' id\r"));
        assert!(!detect_sudo_password_prompt(output, b"read -p 'pwd: ' x\r"));
    }

    #[test]
    fn ignores_non_prompt_output() {
        let output = b"total 0\n-rw-r--r-- 1 root root 0 Jan  1 00:00 file\n";
        assert!(!detect_sudo_password_prompt(output, b"sudo ls\r"));
    }

    /// 回归：sudo 填充命中后必须清空输入缓冲，否则旧 sudo 命令残留在
    /// 最近输入窗口里，本次提权完成后命令输出中的普通冒号结尾行会被
    /// 反复误判成密码提示（前端反复弹出）。
    #[test]
    fn clears_input_elevation_after_hit_to_stop_false_positives() {
        // 填充前：短冒号行 + 最近输入有 sudo -> 命中（这正是误报来源）
        assert!(detect_sudo_password_prompt(
            b"The following NEW packages will be installed: ",
            b"sudo apt install foo\r"
        ));

        // 模拟 handler 命中后清空两个缓冲
        let empty_input: Vec<u8> = Vec::new();
        assert!(!detect_sudo_password_prompt(
            b"The following NEW packages will be installed: ",
            &empty_input
        ));

        // 自带 sudo + password 词的真实提示不依赖输入缓冲：
        // 密码输错重试仍能再次识别
        assert!(detect_sudo_password_prompt(
            b"[sudo] password for user: ",
            &empty_input
        ));
    }

    /* ---------- cwd 注入行回显剥离 ---------- */

    /// 构造一段「MOTD + 提示符 + 注入行回显 + 新提示符」的输出流
    fn echo_stream(target: &str, wrap_at: Option<usize>) -> Vec<u8> {
        let mut stream = b"Last login: ...".to_vec();
        stream.extend_from_slice(b"\r\nhost:~$ ");
        let t = target.as_bytes();
        match wrap_at {
            // 模拟终端按列宽折行：目标行中段插入 \r\n
            Some(at) => {
                stream.extend_from_slice(&t[..at]);
                stream.extend_from_slice(b"\r\n");
                stream.extend_from_slice(&t[at..]);
            }
            None => stream.extend_from_slice(t),
        }
        stream.extend_from_slice(b"\r\nhost:~$ ");
        stream
    }

    #[test]
    fn cwd_echo_stripped_in_single_chunk() {
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let out = s.feed(&echo_stream(target, None));
        // 提示符后开始的回显：剥掉文字但保留 accept 换行（上移擦除依赖它）
        assert_eq!(
            String::from_utf8_lossy(&out),
            "Last login: ...\r\nhost:~$ \r\nhost:~$ "
        );
    }

    #[test]
    fn cwd_echo_stripped_across_chunks() {
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let stream = echo_stream(target, None);
        let mid = 20;
        let out1 = s.feed(&stream[..mid]);
        // 尾部 "hos" 不像半截回显开头，立即转发不扣留
        assert_eq!(String::from_utf8_lossy(&out1), "Last login: ...\r\nhos");
        let out2 = s.feed(&stream[mid..]);
        let mut all = out1;
        all.extend_from_slice(&out2);
        assert_eq!(
            String::from_utf8_lossy(&all),
            "Last login: ...\r\nhost:~$ \r\nhost:~$ "
        );
    }

    #[test]
    fn cwd_echo_stripped_with_line_wrap() {
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let out = s.feed(&echo_stream(target, Some(30)));
        assert_eq!(
            String::from_utf8_lossy(&out),
            "Last login: ...\r\nhost:~$ \r\nhost:~$ "
        );
    }

    #[test]
    fn cwd_echo_strip_gives_up_after_timeout() {
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::with_timeout(target, Duration::ZERO);
        let data = b"plain output without echo";
        assert_eq!(s.feed(data), data.to_vec());
        // 超时后彻底放行，不再扣留
        assert_eq!(s.feed(b"more"), b"more".to_vec());
    }

    #[test]
    fn cwd_echo_strip_forwards_unrelated_output() {
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        // 无关输出立即转发：只有尾部像半截回显开头时才扣留
        assert_eq!(s.feed(b"abc"), b"abc".to_vec());
        assert_eq!(s.feed(&vec![b'x'; 100]), vec![b'x'; 100]);
    }

    #[test]
    fn cwd_echo_double_echo_stripped() {
        // macOS zsh 场景：注入行先被 pty 内核按规范模式回显（首个提示符
        // 之前，独占一行，自带换行一并吃掉），ZLE 就绪后又把缓冲行重显
        // 在提示符处（accept 换行保留），两次文字都要剥掉
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let t = target.as_bytes();
        let mut stream = b"Last login: ...\r\n".to_vec();
        stream.extend_from_slice(t);
        stream.extend_from_slice(b"\r\nhost:~% ");
        stream.extend_from_slice(t);
        stream.extend_from_slice(b"\r\nhost:~% ");
        let out = s.feed(&stream);
        assert_eq!(
            String::from_utf8_lossy(&out),
            "Last login: ...\r\nhost:~% \r\nhost:~% "
        );
    }

    #[test]
    fn cwd_echo_strips_newline_after_line_start_echo() {
        // 内核规范模式回显独占一行（前面是 MOTD 的换行）：回显自带的回车
        // 换行一并吃掉，不留空行
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let t = target.as_bytes();
        let mut stream = b"Last login: ...\r\n".to_vec();
        stream.extend_from_slice(t);
        stream.extend_from_slice(b"\r\nhost:~% ");
        let out = s.feed(&stream);
        assert_eq!(String::from_utf8_lossy(&out), "Last login: ...\r\nhost:~% ");
    }

    #[test]
    fn cwd_echo_strips_newline_when_echo_spans_chunk_start() {
        // 内核回显恰好从块边界开始：依据上一块最后转发的字节判断行首
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let t = target.as_bytes();
        assert_eq!(
            s.feed(b"Last login: ...\r\n"),
            b"Last login: ...\r\n".to_vec()
        );
        let mut rest = t.to_vec();
        rest.extend_from_slice(b"\r\nhost:~% ");
        assert_eq!(s.feed(&rest), b"host:~% ".to_vec());
    }

    #[test]
    fn cwd_echo_strip_stops_after_max_occurrences() {
        // 超出剥离上限的第 11 次出现原样放行（退化为多显示一行）
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let t = target.as_bytes();
        let mut stream = b"host:~% ".to_vec();
        for _ in 0..(MAX_ECHO_OCCURRENCES + 1) {
            stream.extend_from_slice(t);
            stream.extend_from_slice(b"\r\nhost:~% ");
        }
        let mut expected = b"host:~% ".to_vec();
        for _ in 0..MAX_ECHO_OCCURRENCES {
            expected.extend_from_slice(b"\r\nhost:~% ");
        }
        expected.extend_from_slice(t);
        expected.extend_from_slice(b"\r\nhost:~% ");
        assert_eq!(s.feed(&stream), expected);
    }

    #[test]
    fn cwd_echo_strip_holds_partial_prefix_across_chunks() {
        // 尾部 ≥2 字节像半截回显开头时扣留，跨块补齐后整行剥掉
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        assert_eq!(s.feed(b"hello prin"), b"hello".to_vec());
        assert!(s.feed(&target.as_bytes()[5..]).is_empty());
    }

    #[test]
    fn cwd_echo_strip_does_not_hold_lone_trailing_space() {
        // 提示符结尾的单个空格不算半截回显开头，立即转发不扣留
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        assert_eq!(s.feed(b"host:~% "), b"host:~% ".to_vec());
    }

    #[test]
    fn loose_filter_strips_full_leaked_echo() {
        // 主剥离器放弃后（整行逐字节匹配失败/超时），兜底过滤器按 token
        // 吞掉行尾；token 之前的行头残端留在提示符行上，由注入行首句的
        // 上移擦除随行清掉，因此字节流里保留是正确的
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let t = target.as_bytes();
        let off = target.find("__arc(){").unwrap();
        let mut f = LooseEchoFilter::new(target, "t").unwrap();
        let mut stream = b"host:~$ ".to_vec();
        stream.extend_from_slice(t);
        stream.extend_from_slice(b"\r\nhost:~$ ");
        let out = f.feed(&stream);
        let mut expected = b"host:~$ ".to_vec();
        expected.extend_from_slice(&t[..off]);
        expected.extend_from_slice(b"\r\nhost:~$ ");
        assert_eq!(out, expected);
    }

    #[test]
    fn loose_filter_swallows_wrap_crlf() {
        // 折行插入的 \r\n 出现在最短行尾长度之前：视为折行物一并吞掉，
        // 只保留真行尾换行
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let t = target.as_bytes();
        let off = target.find("__arc(){").unwrap();
        let mut f = LooseEchoFilter::new(target, "t").unwrap();
        let mut stream = b"host:~$ ".to_vec();
        let wrap_at = off + 40; // 吞没区内，距行尾阈值尚远
        stream.extend_from_slice(&t[..wrap_at]);
        stream.extend_from_slice(b"\r\n");
        stream.extend_from_slice(&t[wrap_at..]);
        stream.extend_from_slice(b"\r\nhost:~$ ");
        let out = f.feed(&stream);
        let mut expected = b"host:~$ ".to_vec();
        expected.extend_from_slice(&t[..off]);
        expected.extend_from_slice(b"\r\nhost:~$ ");
        assert_eq!(out, expected);
    }

    #[test]
    fn loose_filter_completes_token_across_chunks() {
        // 第一块断在 token 中间：尾部扣留，跨块补全后整段吞掉
        let line = telemetry_line_zsh(true);
        let target = line.trim_end_matches('\n');
        let t = target.as_bytes();
        let off = target.find("__arc(){").unwrap();
        let mut f = LooseEchoFilter::new(target, "t").unwrap();
        let mut first = b"host:~% ".to_vec();
        first.extend_from_slice(&t[..off + 3]);
        let out1 = f.feed(&first);
        let mut expected1 = b"host:~% ".to_vec();
        expected1.extend_from_slice(&t[..off]);
        assert_eq!(out1, expected1);
        let mut rest = t[off + 3..].to_vec();
        rest.extend_from_slice(b"\r\nhost:~% ");
        let out2 = f.feed(&rest);
        assert_eq!(out2, b"\r\nhost:~% ".to_vec());
    }

    #[test]
    fn loose_filter_passthrough_after_window() {
        // 窗口关闭后原样透传，不再吞任何内容
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let mut f = LooseEchoFilter::with_window(target, "t", Duration::ZERO).unwrap();
        let mut stream = b"host:~$ ".to_vec();
        stream.extend_from_slice(target.as_bytes());
        stream.extend_from_slice(b"\r\n");
        assert_eq!(f.feed(&stream), stream);
        assert_eq!(f.feed(b"more"), b"more".to_vec());
    }

    #[test]
    fn loose_filter_gives_up_over_cap() {
        // 上游删字节类改写会让真行尾提前于阈值：吞没越过兜帽后原样放行，
        // 误吞邻接输出的量压在 SWALLOW_SLACK 内
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let off = target.find("__arc(){").unwrap();
        let min_tail = target.len() - off - 8;
        let mut f = LooseEchoFilter::new(target, "t").unwrap();
        let mut stream = b"host:~$ ".to_vec();
        stream.extend_from_slice(target.as_bytes());
        // 改写形态：行尾换行丢失，token 后紧跟无换行的长内容
        stream.extend_from_slice(&vec![b'x'; min_tail + SWALLOW_SLACK + 50]);
        let out = f.feed(&stream);
        // 兜帽吃掉 SWALLOW_SLACK 个 x 后放弃，其余原样转发
        assert_eq!(out.len(), 8 + off + min_tail + 50);
        assert!(out.ends_with(&vec![b'x'; 50]));
    }

    #[test]
    fn loose_filter_noop_when_primary_stripped() {
        // 双层并存安全：主剥离器剥掉回显后，兜底过滤器扫不到 token，
        // 原样透传不误伤
        let line = telemetry_line_bash(true);
        let target = line.trim_end_matches('\n');
        let mut s = CwdEchoStripper::new(target);
        let mut f = LooseEchoFilter::new(target, "t").unwrap();
        let mut stream = b"host:~$ ".to_vec();
        stream.extend_from_slice(target.as_bytes());
        stream.extend_from_slice(b"\r\nhost:~$ ");
        let stripped = s.feed(&stream);
        assert_eq!(f.feed(&stripped), stripped);
    }
}

#[derive(Debug, Deserialize)]
pub struct SendQuery {
    /// 等待输出的硬上限毫秒数（默认 10s）。bash/zsh 会话在命令完成时
    /// 提前返回；无钩子会话在输出静默后提前返回；本值只兜底挂死场景。
    #[serde(default = "default_wait_ms")]
    pub wait_ms: u64,
}

fn default_wait_ms() -> u64 {
    10_000
}

/// wait_ms 硬上限：防止异常大值长时间占住请求与收集锁
const SEND_MAX_WAIT_MS: u64 = 120_000;
/// 无钩子会话的输出静默判定窗口：连续静默该时长视为命令结束（拿不到退出码）
const SEND_QUIESCE_MS: u64 = 400;
/// 返回给调用方的输出字节上限，超出丢弃并置 truncated
const SEND_OUTPUT_CAP: usize = 200_000;

/// `POST /api/ssh/send/{sid}?wait_ms=` 的结构化结果
#[derive(Debug, Serialize)]
pub struct SendResult {
    /// 终端输出（已剥命令回显与 ANSI 转义序列）
    pub output: String,
    /// 退出码：bash/zsh 会话由遥测钩子上报；其余会话为 null
    pub exit_code: Option<u32>,
    /// 是否确认完成（钩子上报的完成事件数达到预期）
    pub completed: bool,
    /// 是否因到达 wait_ms 上限而返回（输出可能不完整）
    pub timed_out: bool,
    /// 收集期间输出流是否关闭（会话已结束）
    pub session_closed: bool,
    /// 输出是否超上限被截断
    pub truncated: bool,
}

/// OSC 完成事件扫描器：流式统计遥测钩子上报的
/// `ESC]9;10;<exit>BEL`（兼容 ST 结尾）。事件可能跨块：解析失败时从最后一个
/// ESC 扣留尾部，与下一块拼接后重扫；扣留上限防病态输出无限增长。
struct CompletionScanner {
    tail: String,
    count: usize,
    last_exit: Option<u32>,
}

impl CompletionScanner {
    fn new() -> Self {
        Self {
            tail: String::new(),
            count: 0,
            last_exit: None,
        }
    }

    fn feed(&mut self, chunk: &str) {
        let mut buf = std::mem::take(&mut self.tail);
        buf.push_str(chunk);
        let b = buf.as_bytes();
        let mut i = 0;
        let mut last_esc: Option<usize> = None;
        while i < b.len() {
            if b[i] == 0x1b {
                if let Some((code, end)) = parse_completion_at(b, i) {
                    self.count += 1;
                    self.last_exit = Some(code);
                    i = end;
                    last_esc = None;
                    continue;
                }
                last_esc = Some(i);
            }
            i += 1;
        }
        self.tail = match last_esc {
            Some(p) => {
                let mut tail = buf.split_off(p);
                // 病态守护：尾部长期解析不出且持续增长时截断保留末尾
                if tail.len() > 8192 {
                    tail.split_off(tail.len() - 32)
                } else {
                    tail
                }
            }
            None => String::new(),
        };
    }
}

/// `b[pos]` 为 ESC 时尝试解析 `ESC]9;10;<1-3位数字>(BEL|ESC \)`
fn parse_completion_at(b: &[u8], pos: usize) -> Option<(u32, usize)> {
    const HEAD: &[u8] = b"\x1b]9;10;";
    let rest = b.get(pos..)?;
    if !rest.starts_with(HEAD) {
        return None;
    }
    let mut i = HEAD.len();
    let mut code: u32 = 0;
    let mut digits = 0;
    while i < rest.len() && rest[i].is_ascii_digit() {
        code = code.saturating_mul(10).saturating_add((rest[i] - b'0') as u32);
        digits += 1;
        i += 1;
        if digits > 3 {
            return None; // 退出码 0-255，超长按非事件处理
        }
    }
    if digits == 0 {
        return None;
    }
    match rest.get(i) {
        Some(0x07) => Some((code, pos + i + 1)),
        Some(0x1b) if rest.get(i + 1) == Some(&b'\\') => Some((code, pos + i + 2)),
        // 终止符未到（跨块）或非终止字节：按非事件处理，跨块场景靠尾部扣留重扫
        _ => None,
    }
}

/// `POST /api/ssh/send/{sid}?wait_ms=`
///
/// 向已有终端会话注入命令并收集输出。
/// body 即为要发送到 SSH stdin 的文本，以 `\n` 结尾执行命令；多行文本
/// 按行数等待相应次数的完成事件。
///
/// 收集策略：bash/zsh 会话（含 bash-as-sh）由每提示符遥测钩子上报完成
/// 事件与退出码；dash/ash/ksh 等 POSIX sh 家族在命令后追加独立上报行
/// （标记模式，该行在用户终端可见）；其余 shell 与无尾 `\n` 的悬空命令
/// 退化为输出静默判定。`wait_ms` 为所有场景的硬上限。同一会话的并发
/// 收集按 sid 串行化。
pub async fn send_handler(
    Path(sid): Path<String>,
    Query(query): Query<SendQuery>,
    body: String,
) -> Result<impl IntoResponse, crate::errors::AppError> {
    // 同一会话串行化：并发注入的输出在广播流中无法区分归属
    let collect_lock = ssh_svc::acquire_collect_lock(&sid).await;
    let _guard = collect_lock.lock().await;

    // 先订阅输出，再发送命令，避免竞态丢失
    let output_tx = ssh_svc::get_terminal_output(&sid).await?;
    let mut rx = output_tx.subscribe();
    let tx = ssh_svc::get_terminal_sender(&sid).await?;

    let mode = ssh_svc::get_terminal_completion(&sid).await;
    let hooked = mode == ssh_svc::CompletionMode::Hooked;
    // marker 模式：上报后缀并入最后一个非空命令行（独立成行会多消耗一次
    // 提示符周期，用户终端每条命令后多出空提示符行）。body 须以 \n 结尾
    // （完整命令行才可依附）；末行疑似 heredoc 终止符等不可依附时退回
    // 独立上报行；悬空半行 / 全空白 body 退化为静默兜底。
    let mut send_marker = false;
    let send_body: Vec<String> = if mode == ssh_svc::CompletionMode::Marker
        && body.ends_with('\n')
        && !body.trim().is_empty()
    {
        match merge_marker(&body) {
            Some(merged) => {
                send_marker = true;
                vec![merged]
            }
            None => {
                send_marker = true;
                vec![body.clone(), SH_MARKER_LINE.to_string()]
            }
        }
    } else {
        vec![body.clone()]
    };
    // 完成事件数预期：钩子模式 = 注入行数（每行执行完各出一次提示符/钩子）；
    // 标记模式 = 1（上报后缀/上报行只发一次事件）
    let expected = if hooked { body.matches('\n').count().max(1) } else { 1 };
    // 注入的命令会被终端回显：武装剥离器在收集副本中移除（终端显示不受
    // 影响；marker 后缀已由中继层剥离，此处目标为原始命令行）
    let mut echo_stripper = CwdEchoStripper::new(body.trim_end_matches('\n'));
    let mut scanner = CompletionScanner::new();
    for payload in &send_body {
        tx.send(payload.clone())
            .map_err(|_| crate::errors::AppError::Internal("terminal channel closed".into()))?;
    }
    let mut raw = String::new();
    let mut truncated = false;
    let mut session_closed = false;
    let mut got_output = false;
    let mut timed_out = false;

    let wait = query.wait_ms.clamp(1, SEND_MAX_WAIT_MS);
    let deadline = tokio::time::sleep(Duration::from_millis(wait));
    tokio::pin!(deadline);
    let quiesce_dur = Duration::from_millis(SEND_QUIESCE_MS);
    let quiesce = tokio::time::sleep(quiesce_dur);
    tokio::pin!(quiesce);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(chunk) => {
                        let cleaned = echo_stripper.feed(chunk.as_bytes());
                        if cleaned.is_empty() {
                            continue;
                        }
                        got_output = true;
                        quiesce.as_mut().reset(tokio::time::Instant::now() + quiesce_dur);
                        let text = String::from_utf8_lossy(&cleaned);
                        scanner.feed(&text);
                        if raw.len() < SEND_OUTPUT_CAP {
                            raw.push_str(&text);
                        } else {
                            truncated = true;
                        }
                        if hooked && scanner.count >= expected {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => {
                        session_closed = true;
                        break;
                    }
                }
            }
            _ = &mut deadline => {
                timed_out = true;
                break;
            }
            // 无钩子会话：输出静默后视为命令结束（退出码不可得）
            _ = &mut quiesce, if !hooked && got_output => {
                break;
            }
        }
    }

    let output = util::strip_ansi(&raw);
    let completed = match mode {
        ssh_svc::CompletionMode::Hooked => scanner.count >= expected,
        ssh_svc::CompletionMode::Marker => send_marker && scanner.count >= 1,
        ssh_svc::CompletionMode::None => false,
    };
    Ok(handlers::ApiResponse::ok(SendResult {
        output,
        exit_code: if mode == ssh_svc::CompletionMode::None {
            None
        } else {
            scanner.last_exit
        },
        completed,
        timed_out,
        session_closed,
        truncated,
    }))
}

#[cfg(test)]
mod send_tests {
    use super::*;

    #[test]
    fn scanner_parses_bel_terminated_events() {
        let mut s = CompletionScanner::new();
        s.feed("out\x1b]9;10;0\x07more");
        assert_eq!(s.count, 1);
        assert_eq!(s.last_exit, Some(0));
    }

    #[test]
    fn scanner_parses_st_terminated_events() {
        let mut s = CompletionScanner::new();
        s.feed("out\x1b]9;10;127\x1b\\tail");
        assert_eq!(s.count, 1);
        assert_eq!(s.last_exit, Some(127));
    }

    #[test]
    fn scanner_joins_event_across_chunks() {
        let mut s = CompletionScanner::new();
        s.feed("out\x1b]9;10;2");
        assert_eq!(s.count, 0);
        s.feed("\x07done");
        assert_eq!(s.count, 1);
        assert_eq!(s.last_exit, Some(2));
    }

    #[test]
    fn scanner_ignores_cwd_reports_and_colors() {
        let mut s = CompletionScanner::new();
        s.feed("\x1b]9;9;/home\x07\x1b[31mred\x1b[0m\x1b]9;10;1\x07");
        assert_eq!(s.count, 1);
        assert_eq!(s.last_exit, Some(1));
    }

    #[test]
    fn scanner_counts_multiple_events_and_keeps_last_code() {
        let mut s = CompletionScanner::new();
        s.feed("\x1b]9;10;0\x07\x1b]9;10;3\x07");
        assert_eq!(s.count, 2);
        assert_eq!(s.last_exit, Some(3));
    }

    #[test]
    fn scanner_rejects_overlong_and_empty_codes() {
        let mut s = CompletionScanner::new();
        s.feed("\x1b]9;10;1234\x07\x1b]9;10;\x07");
        assert_eq!(s.count, 0);
        assert_eq!(s.last_exit, None);
    }

    #[test]
    fn parse_completion_at_matches_exact_boundaries() {
        let b = b"\x1b]9;10;0\x07".as_slice();
        assert_eq!(parse_completion_at(b, 0), Some((0, b.len())));
        // 中间字节不是数字也不是终止符：非事件
        assert_eq!(parse_completion_at(b"\x1b]9;10x0\x07", 0), None);
        // 事件前有杂讯：只认精确头部（含杂讯下标与 BEL 后的结束下标）
        assert_eq!(parse_completion_at(b"x\x1b]9;10;9\x07", 1), Some((9, 10)));
    }

    #[test]
    fn resolve_completion_bash_as_sh_via_readlink() {
        let (line, mode) = resolve_completion("/bin/sh", "/bin/bash", false);
        assert_eq!(mode, ssh_svc::CompletionMode::Hooked);
        assert!(line.unwrap().contains("__arc"));
    }

    #[test]
    fn resolve_completion_zsh_hooked_with_cwd_gate() {
        let (on, mode) = resolve_completion("/bin/zsh", "", true);
        assert_eq!(mode, ssh_svc::CompletionMode::Hooked);
        let on = on.unwrap();
        assert!(on.contains("9;9;"), "cwd 变体应上报 9;9");
        assert!(on.contains("9;10;"), "cwd 变体仍应上报 9;10");
        let (off, _) = resolve_completion("/bin/zsh", "", false);
        let off = off.unwrap();
        assert!(!off.contains("9;9;"), "非 cwd 变体不应上报 9;9");
        assert!(off.contains("9;10;"));
    }

    #[test]
    fn telemetry_lines_stay_short_to_avoid_wrap_residue() {
        // 注入行超过终端列宽会折行，上移擦除只清一行，前面的回显残影
        // 会留在用户终端（255 字符线体在 ~250 列终端露馅的教训）
        for cwd in [true, false] {
            assert!(
                telemetry_line_bash(cwd).len() <= 160,
                "bash 线体过长: {}",
                telemetry_line_bash(cwd)
            );
            assert!(
                telemetry_line_zsh(cwd).len() <= 160,
                "zsh 线体过长: {}",
                telemetry_line_zsh(cwd)
            );
        }
    }

    #[test]
    fn resolve_completion_posix_sh_family_marker() {
        for sh in ["/bin/sh", "/usr/bin/dash", "/bin/ash", "/bin/ksh", "/bin/mksh"] {
            let (line, mode) = resolve_completion(sh, "", false);
            assert_eq!(mode, ssh_svc::CompletionMode::Marker, "shell={sh}");
            assert!(line.is_none());
        }
    }

    #[test]
    fn resolve_completion_plain_sh_symlink_stays_marker() {
        // macOS /bin/sh 是实体文件：readlink 原样返回，仍走标记模式
        let (_, mode) = resolve_completion("/bin/sh", "/bin/sh", false);
        assert_eq!(mode, ssh_svc::CompletionMode::Marker);
    }

    #[test]
    fn resolve_completion_other_shells_silent() {
        for sh in ["/bin/csh", "/usr/bin/fish", "/usr/bin/pwsh", ""] {
            let (line, mode) = resolve_completion(sh, "", false);
            assert_eq!(mode, ssh_svc::CompletionMode::None, "shell={sh}");
            assert!(line.is_none());
        }
    }

    #[test]
    fn marker_echo_strip_unbounded_survives_many_occurrences() {
        // marker 行每次 AI 注入都回显一次：剥离器无 20s/10 次上限，
        // 会话生命周期内持续生效
        let target = SH_MARKER_LINE.trim_end_matches('\n');
        let mut s = CwdEchoStripper::unbounded(target);
        let t = target.as_bytes();
        for _ in 0..(MAX_ECHO_OCCURRENCES + 5) {
            assert!(s.feed(t).is_empty(), "unbounded 剥离器不应停止剥离");
        }
    }

    #[test]
    fn marker_echo_strip_survives_past_deadline_window() {
        // 与 new() 的 20s 时限对照：unbounded 靠字段豁免时限判断，
        // 构造后立即喂入即可验证（时限路径无法在单测中等待真实时间，
        // 这里验证的是「不因 done 提前放行」这一行为面）
        let target = SH_MARKER_LINE.trim_end_matches('\n');
        let mut s = CwdEchoStripper::unbounded(target);
        let t = target.as_bytes();
        assert!(s.feed(&[b'x'; 64]).starts_with(b"x"));
        assert!(s.feed(t).is_empty());
        assert!(s.feed(t).is_empty());
    }

    #[test]
    fn merge_marker_appends_to_last_command_line() {
        let merged = merge_marker("echo hello\n").unwrap();
        assert_eq!(merged, format!("echo hello{MARKER_SUFFIX}\n"));
        // 多行 body：后缀只依附最后一个非空行
        assert_eq!(
            merge_marker("echo a\necho hello\n").unwrap(),
            format!("echo a\necho hello{MARKER_SUFFIX}\n")
        );
    }

    #[test]
    fn merge_marker_strips_trailing_semicolon_and_blank_lines() {
        assert_eq!(
            merge_marker("echo a;\n").unwrap(),
            format!("echo a{MARKER_SUFFIX}\n")
        );
        assert_eq!(
            merge_marker("echo a\n\n").unwrap(),
            format!("echo a{MARKER_SUFFIX}\n")
        );
    }

    #[test]
    fn merge_marker_rejects_blank_body_and_heredoc_terminator() {
        assert!(merge_marker("  \n\n").is_none());
        // body 含 << 且末行为裸词：疑似 heredoc 终止符，不并入
        assert!(merge_marker("cat <<EOF\nhi\nEOF\n").is_none());
        // 无 << 时裸词末行正常并入
        assert!(merge_marker("EOF\n").unwrap().contains(MARKER_SUFFIX));
    }

    #[test]
    fn marker_strip_target_selects_by_payload_shape() {
        // 后缀并入式：目标为后缀本身（命令文本保留可见）
        assert_eq!(
            marker_strip_target(&format!("echo hello{MARKER_SUFFIX}\n")),
            Some(MARKER_SUFFIX)
        );
        // 独立上报行退路：目标为整行
        assert_eq!(
            marker_strip_target(SH_MARKER_LINE),
            Some(SH_MARKER_LINE.trim())
        );
        // 非 marker 注入（悬空半行等）：不重置
        assert_eq!(marker_strip_target("ls /tmp"), None);
    }
}
