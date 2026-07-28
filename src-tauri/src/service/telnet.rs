//! Telnet 终端：复用前端 TerminalView 的 JSON/二进制协议，通过 TCP 连接远端 Telnet 服务。
//!
//! 协议（与 handlers::terminal / local_pty 完全一致）：
//! - 上行 Text JSON：`{kind:"cmd", data}` / `{kind:"resize", cols, rows}` / `{kind:"ping"}`
//! - 下行 首帧 Text JSON：`{kind:"ready", sid}`，之后 Binary 帧为 Telnet 输出。
//!
//! Telnet IAC 协商：
//! - 收到 IAC WILL ECHO → 回 IAC DO ECHO（服务端回显）
//! - 收到 IAC WILL SGA  → 回 IAC DO SGA（抑制 Go Ahead）
//! - 收到 IAC DO TTYPE  → 回 IAC WILL TTYPE，随后 IAC SB TTYPE IS xterm-256color IAC SE
//! - 收到 IAC DO NAWS   → 回 IAC WILL NAWS，随后发送窗口尺寸
//! - 其余 WILL/DO 一律 WONT/DONT 拒绝

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::service::ssh as ssh_svc;

// Telnet 协议常量
const IAC: u8 = 255;
const WILL: u8 = 251;
const WONT: u8 = 252;
const DO: u8 = 253;
const DONT: u8 = 254;
const SB: u8 = 250;
const SE: u8 = 240;

// Telnet 选项
const OPT_ECHO: u8 = 1;
const OPT_SGA: u8 = 3;
const OPT_TTYPE: u8 = 24;
const OPT_NAWS: u8 = 31;

// TTYPE 子协商
const TTYPE_IS: u8 = 0;
const TERM_TYPE: &[u8] = b"xterm-256color";

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ClientMsg {
    Cmd { data: String },
    Resize { cols: u16, rows: u16 },
    Ping,
    #[serde(rename = "sudo_fill")]
    SudoFill,
}

#[derive(Debug, Serialize)]
struct ReadyMsg<'a> {
    kind: &'a str,
    sid: &'a str,
}

/// 从 Telnet 字节流中剥离 IAC 序列，返回纯净数据 + 需要回复的 IAC 响应。
///
/// 返回 `(clean_data, iac_responses)`。
fn process_telnet_stream(
    buf: &[u8],
    cols: u16,
    rows: u16,
) -> (Vec<u8>, Vec<u8>) {
    let mut clean = Vec::with_capacity(buf.len());
    let mut resp = Vec::new();
    let mut i = 0;

    while i < buf.len() {
        if buf[i] != IAC {
            clean.push(buf[i]);
            i += 1;
            continue;
        }

        // IAC 序列
        if i + 1 >= buf.len() {
            // 不完整，丢弃尾部 IAC
            break;
        }

        let cmd = buf[i + 1];
        match cmd {
            WILL | WONT | DO | DONT => {
                if i + 2 >= buf.len() {
                    break;
                }
                let opt = buf[i + 2];
                i += 3;

                match (cmd, opt) {
                    (WILL, OPT_ECHO) => {
                        resp.extend_from_slice(&[IAC, DO, OPT_ECHO]);
                    }
                    (WILL, OPT_SGA) => {
                        resp.extend_from_slice(&[IAC, DO, OPT_SGA]);
                    }
                    (DO, OPT_TTYPE) => {
                        resp.extend_from_slice(&[IAC, WILL, OPT_TTYPE]);
                        // 紧接着发送 TTYPE 子协商
                        resp.extend_from_slice(&[IAC, SB, OPT_TTYPE, TTYPE_IS]);
                        resp.extend_from_slice(TERM_TYPE);
                        resp.extend_from_slice(&[IAC, SE]);
                    }
                    (DO, OPT_NAWS) => {
                        resp.extend_from_slice(&[IAC, WILL, OPT_NAWS]);
                        resp.extend_from_slice(&[
                            IAC, SB, OPT_NAWS,
                            (cols >> 8) as u8, (cols & 0xFF) as u8,
                            (rows >> 8) as u8, (rows & 0xFF) as u8,
                            IAC, SE,
                        ]);
                    }
                    (WILL, _) => {
                        resp.extend_from_slice(&[IAC, DONT, opt]);
                    }
                    (DO, _) => {
                        resp.extend_from_slice(&[IAC, WONT, opt]);
                    }
                    _ => {
                        // WONT/DONT 不需要回复
                    }
                }
            }
            SB => {
                // 跳过子协商直到 IAC SE
                i += 2;
                while i + 1 < buf.len() {
                    if buf[i] == IAC && buf[i + 1] == SE {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            IAC => {
                // IAC IAC → 字面 0xFF
                clean.push(0xFF);
                i += 2;
            }
            _ => {
                // 其他两字节命令（NOP、DM 等），跳过
                i += 2;
            }
        }
    }

    (clean, resp)
}

/// 构建 NAWS 子协商报文
fn build_naws(cols: u16, rows: u16) -> Vec<u8> {
    vec![
        IAC, SB, OPT_NAWS,
        (cols >> 8) as u8, (cols & 0xFF) as u8,
        (rows >> 8) as u8, (rows & 0xFF) as u8,
        IAC, SE,
    ]
}

pub async fn handle(
    socket: WebSocket,
    sid: String,
    addr: String,
    port: u16,
    _username: String,
    _password: Option<String>,
) {
    log::info!("telnet: connecting sid={sid} {addr}:{port}");

    let (mut ws_tx, mut ws_rx) = socket.split();

    // 1) TCP 连接
    let stream = match TcpStream::connect(format!("{addr}:{port}")).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("telnet connect failed sid={sid}: {e}");
            let _ = ws_tx
                .send(Message::Text(
                    format!("\x1b[31m[ashell] telnet connect failed: {e}\x1b[0m").into(),
                ))
                .await;
            return;
        }
    };
    log::info!("telnet: connected sid={sid}");

    // 2) 通告 sid
    let ready = serde_json::to_string(&ReadyMsg {
        kind: "ready",
        sid: &sid,
    })
    .unwrap_or_else(|_| "{\"kind\":\"ready\",\"sid\":\"\"}".to_string());
    let _ = ws_tx.send(Message::Text(ready.into())).await;

    let (mut reader, writer) = stream.into_split();

    // 3) 注册终端通道（AI sidecar 可通过 POST /api/ssh/send/{sid} 注入命令）
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    let (output_tx, _) = broadcast::channel::<String>(64);
    ssh_svc::set_terminal_channels(&sid, cmd_tx, output_tx.clone()).await;

    // 共享 writer（WS 输入 + 命令注入 + IAC 响应都要写）
    let writer = Arc::new(Mutex::new(writer));

    // 当前窗口尺寸（resize 时更新，NAWS 协商用）
    let cols = Arc::new(std::sync::atomic::AtomicU16::new(80));
    let rows = Arc::new(std::sync::atomic::AtomicU16::new(24));

    // 4) 读任务：从 TCP 读 → 剥离 IAC → 推给 WS + 广播
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    let (iac_tx, mut iac_rx) = mpsc::channel::<Vec<u8>>(16);
    let cols_r = cols.clone();
    let rows_r = rows.clone();

    let read_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let (clean, resp) =
                        process_telnet_stream(&buf[..n], cols_r.load(std::sync::atomic::Ordering::Relaxed), rows_r.load(std::sync::atomic::Ordering::Relaxed));
                    if !resp.is_empty() {
                        if iac_tx.send(resp).await.is_err() {
                            break;
                        }
                    }
                    if !clean.is_empty() {
                        if out_tx.send(clean).await.is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 5) 主事件循环
    let writer_cmd = writer.clone();
    let writer_iac = writer.clone();

    loop {
        tokio::select! {
            // TCP 输出 → WS Binary
            Some(data) = out_rx.recv() => {
                let _ = output_tx.send(String::from_utf8_lossy(&data).to_string());
                if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                    break;
                }
            }
            // IAC 响应 → TCP
            Some(resp) = iac_rx.recv() => {
                let mut w = writer_iac.lock().await;
                let _ = w.write_all(&resp).await;
                let _ = w.flush().await;
            }
            // 外部命令注入（AI sidecar）
            Some(data) = cmd_rx.recv() => {
                let mut w = writer_cmd.lock().await;
                let _ = w.write_all(data.as_bytes()).await;
                let _ = w.flush().await;
            }
            // WS 输入
            msg = ws_rx.next() => {
                let Some(msg) = msg else { break; };
                match msg {
                    Ok(Message::Text(text)) => {
                        let s = text.as_str();
                        if s.starts_with('{') && s.ends_with('}') {
                            match serde_json::from_str::<ClientMsg>(s) {
                                Ok(ClientMsg::Cmd { data }) => {
                                    let mut w = writer.lock().await;
                                    let _ = w.write_all(data.as_bytes()).await;
                                    let _ = w.flush().await;
                                }
                                Ok(ClientMsg::Resize { cols: c, rows: r }) => {
                                    cols.store(c, std::sync::atomic::Ordering::Relaxed);
                                    rows.store(r, std::sync::atomic::Ordering::Relaxed);
                                    let naws = build_naws(c, r);
                                    let mut w = writer.lock().await;
                                    let _ = w.write_all(&naws).await;
                                    let _ = w.flush().await;
                                }
                                Ok(ClientMsg::Ping) => {
                                    let _ = ws_tx.send(Message::Text(
                                        "{\"kind\":\"pong\"}".to_string().into(),
                                    )).await;
                                }
                                Ok(ClientMsg::SudoFill) => {
                                    // Telnet 无 sudo 密码机制，忽略
                                }
                                Err(_) => {
                                    let mut w = writer.lock().await;
                                    let _ = w.write_all(s.as_bytes()).await;
                                    let _ = w.flush().await;
                                }
                            }
                        } else {
                            let mut w = writer.lock().await;
                            let _ = w.write_all(s.as_bytes()).await;
                            let _ = w.flush().await;
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        let mut w = writer.lock().await;
                        let _ = w.write_all(&bytes).await;
                        let _ = w.flush().await;
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }

    log::info!("telnet cleanup sid={sid}");
    read_task.abort();
    ssh_svc::remove_terminal_channels(&sid).await;
    let _ = ws_tx.close().await;
}
