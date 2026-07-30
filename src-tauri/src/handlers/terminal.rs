use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::handlers;
use crate::service::ssh::{self as ssh_svc, Session};
use crate::service::{self, AppState};

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
}

/// 服务端首条消息：把生成的 sid 通知客户端
#[derive(Debug, Serialize)]
struct ReadyMsg<'a> {
    kind: &'a str,
    sid: &'a str,
}

const PROMPT_BUF_LIMIT: usize = 1024;
const INPUT_BUF_LIMIT: usize = 512;

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
    // 1) 取主机并解密凭证
    let host = service::host::get_with_credentials(&state.db, &state.config.crypto_key, host_id)
        .await
        .map_err(|e| anyhow::anyhow!("load host: {e}"))?;

    // 2) 建立 SSH session 并注册到全局
    let session = Session::connect(&host)
        .await
        .map_err(|e| anyhow::anyhow!("ssh connect: {e}"))?;
    let session_arc = Arc::new(session);
    // 把 sid 写入 client handler，使远程转发回连能路由到本会话
    session_arc.attach_sid(sid).await;
    ssh_svc::set_client(sid.to_string(), session_arc.clone()).await;

    // 3) 同步打开一个 sftp 子通道（即便客户端不立刻用，后续 REST 也能用）
    if let Err(e) = session_arc.open_sftp(sid).await {
        log::warn!("open sftp for sid={sid} failed: {e}");
    }

    // 4) 打开终端 channel
    let mut channel = session_arc.channel_open_session().await?;
    channel
        .request_pty(false, &q.term, q.cols, q.rows, 0, 0, &[])
        .await
        .map_err(|e| anyhow::anyhow!("request pty: {e}"))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| anyhow::anyhow!("request shell: {e}"))?;

    let (mut ws_tx, mut ws_rx) = socket.split();

    // 创建终端命令注入 / 输出广播通道（供 POST /api/ssh/send/{sid} 使用）
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    let (output_tx, _) = broadcast::channel::<String>(64);
    ssh_svc::set_terminal_channels(sid, cmd_tx, output_tx.clone()).await;

    // 把 sid 通知客户端
    let ready = serde_json::to_string(&ReadyMsg { kind: "ready", sid })?;
    let _ = ws_tx.send(Message::Text(ready.into())).await;

    // sudo 密码自动填充：检测到密码提示时通知前端，
    // 前端拦截回车发送 sudo_fill，后端注入已保存的密码。
    let sudo_password = host.password.clone();
    let mut sudo_buf: Vec<u8> = Vec::with_capacity(PROMPT_BUF_LIMIT);
    let mut terminal_input_buf: Vec<u8> = Vec::with_capacity(INPUT_BUF_LIMIT);

    // idle 定时发送空字符保活：配置了 idle_send_interval > 0 时启用，
    // 每隔该秒数向 PTY 发送 \x00（对终端无害，可防止 NAT/防火墙超时断连）
    let idle_secs = host.idle_send_interval.unwrap_or(0);
    let idle_dur = if idle_secs > 0 {
        Duration::from_secs(idle_secs as u64)
    } else {
        Duration::from_secs(u64::MAX / 2)
    };
    let idle_timer = tokio::time::sleep(idle_dur);
    tokio::pin!(idle_timer);

    let closed = false;
    loop {
        tokio::select! {
            // 外部命令注入（POST /api/ssh/send/{sid}）
            Some(data) = cmd_rx.recv() => {
                if channel.data(data.as_bytes()).await.is_err() { break; }
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
                                        if channel.data(data.as_bytes()).await.is_err() { break; }
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
                                                if channel.data(payload.as_bytes()).await.is_err() { break; }
                                            }
                                        }
                                    }
                                }
                            } else if channel.data(s.as_bytes()).await.is_err() {
                                break;
                            }
                        } else if channel.data(s.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        if channel.data(&b[..]).await.is_err() { break; }
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
                        let bytes = data.to_vec();
                        let _ = output_tx.send(String::from_utf8_lossy(&bytes).to_string());
                        if ws_tx.send(Message::Binary(bytes.clone().into())).await.is_err() { break; }

                        push_limited(&mut sudo_buf, &bytes, PROMPT_BUF_LIMIT);
                        if detect_sudo_password_prompt(&sudo_buf, &terminal_input_buf)
                            && sudo_password.as_deref().is_some_and(|p| !p.is_empty())
                        {
                            sudo_buf.clear();
                            let _ = ws_tx.send(Message::Text(
                                "{\"kind\":\"sudo_prompt\"}".to_string().into(),
                            )).await;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { ref data, ext: _ }) => {
                        let bytes = data.to_vec();
                        let _ = output_tx.send(String::from_utf8_lossy(&bytes).to_string());
                        if ws_tx.send(Message::Binary(bytes.clone().into())).await.is_err() { break; }

                        // 部分服务端/程序会把密码提示写到 stderr；这里同样参与识别。
                        push_limited(&mut sudo_buf, &bytes, PROMPT_BUF_LIMIT);
                        if detect_sudo_password_prompt(&sudo_buf, &terminal_input_buf)
                            && sudo_password.as_deref().is_some_and(|p| !p.is_empty())
                        {
                            sudo_buf.clear();
                            let _ = ws_tx.send(Message::Text(
                                "{\"kind\":\"sudo_prompt\"}".to_string().into(),
                            )).await;
                        }
                    }
                    Some(ChannelMsg::ExitStatus { exit_status: _ }) => {
                        if !closed {
                            let _ = channel.eof().await;
                            let _ = ws_tx.close().await;
                            let _ = channel.close().await;
                        }
                        break;
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    _ => {}
                }
            }
            _ = &mut idle_timer, if idle_secs > 0 => {
                if channel.data(&[0u8][..]).await.is_err() { break; }
                idle_timer.as_mut().reset(tokio::time::Instant::now() + idle_dur);
            }
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
}

#[derive(Debug, Deserialize)]
pub struct SendQuery {
    /// 等待输出收集的毫秒数，默认 500
    #[serde(default = "default_wait_ms")]
    pub wait_ms: u64,
}

fn default_wait_ms() -> u64 {
    500
}

/// `POST /api/ssh/send/{sid}?wait_ms=`
///
/// 向已有终端会话注入命令并收集输出。
/// body 即为要发送到 SSH stdin 的文本（通常以 `\n` 结尾）。
pub async fn send_handler(
    Path(sid): Path<String>,
    Query(query): Query<SendQuery>,
    body: String,
) -> Result<impl IntoResponse, crate::errors::AppError> {
    // 先订阅输出，再发送命令，避免竞态丢失
    let output_tx = ssh_svc::get_terminal_output(&sid).await?;
    let mut rx = output_tx.subscribe();
    let tx = ssh_svc::get_terminal_sender(&sid).await?;
    tx.send(body)
        .map_err(|_| crate::errors::AppError::Internal("terminal channel closed".into()))?;

    let mut result = String::new();
    let deadline = tokio::time::sleep(Duration::from_millis(query.wait_ms));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(data) => result.push_str(&data),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = &mut deadline => {
                break;
            }
        }
    }

    Ok(handlers::ApiResponse::ok(result))
}
