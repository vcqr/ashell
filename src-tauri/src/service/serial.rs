//! 串口终端：复用前端 TerminalView 的 JSON/二进制协议，通过 serialport 打开本地串口设备。
//!
//! 协议（与 handlers::terminal / local_pty 完全一致）：
//! - 上行 Text JSON：`{kind:"cmd", data}` / `{kind:"resize", cols, rows}` / `{kind:"ping"}`
//! - 下行 首帧 Text JSON：`{kind:"ready", sid}`，之后 Binary 帧为串口输出。
//!
//! 串口没有窗口尺寸概念，resize 消息仅记录日志、不做任何操作。

use std::io::{Read, Write};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::service::ssh as ssh_svc;

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

/// 串口配置（从 Host 模型映射）
pub struct SerialConfig {
    pub path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: String,
    pub flow_control: String,
}

fn map_data_bits(v: u8) -> serialport::DataBits {
    match v {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        _ => serialport::DataBits::Eight,
    }
}

fn map_stop_bits(v: u8) -> serialport::StopBits {
    match v {
        2 => serialport::StopBits::Two,
        _ => serialport::StopBits::One,
    }
}

fn map_parity(v: &str) -> serialport::Parity {
    match v {
        "odd" => serialport::Parity::Odd,
        "even" => serialport::Parity::Even,
        _ => serialport::Parity::None,
    }
}

fn map_flow_control(v: &str) -> serialport::FlowControl {
    match v {
        "software" => serialport::FlowControl::Software,
        "hardware" => serialport::FlowControl::Hardware,
        _ => serialport::FlowControl::None,
    }
}

pub async fn handle(socket: WebSocket, sid: String, config: SerialConfig) {
    log::info!(
        "serial: opening sid={sid} path={} baud={}",
        config.path,
        config.baud_rate
    );

    let (mut ws_tx, mut ws_rx) = socket.split();

    // 1) 打开串口
    let port = match serialport::new(&config.path, config.baud_rate)
        .data_bits(map_data_bits(config.data_bits))
        .stop_bits(map_stop_bits(config.stop_bits))
        .parity(map_parity(&config.parity))
        .flow_control(map_flow_control(&config.flow_control))
        .timeout(Duration::from_millis(50))
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            log::error!("serial open failed sid={sid}: {e}");
            let _ = ws_tx
                .send(Message::Text(
                    format!("\x1b[31m[ashell] serial open failed: {e}\x1b[0m").into(),
                ))
                .await;
            return;
        }
    };
    log::info!("serial: opened sid={sid}");

    // 2) 通告 sid
    let ready = serde_json::to_string(&ReadyMsg {
        kind: "ready",
        sid: &sid,
    })
    .unwrap_or_else(|_| "{\"kind\":\"ready\",\"sid\":\"\"}".to_string());
    let _ = ws_tx.send(Message::Text(ready.into())).await;

    // 3) 注册终端通道（AI sidecar 可通过 POST /api/ssh/send/{sid} 注入命令）
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    let (output_tx, _) = broadcast::channel::<String>(64);
    ssh_svc::set_terminal_channels(&sid, cmd_tx, output_tx.clone()).await;

    // 4) 串口读写：serialport 是同步 API，放到阻塞线程
    let mut port_reader = match port.try_clone() {
        Ok(r) => r,
        Err(e) => {
            log::error!("serial try_clone failed sid={sid}: {e}");
            let _ = ws_tx
                .send(Message::Text(
                    format!("\x1b[31m[ashell] serial clone failed: {e}\x1b[0m").into(),
                ))
                .await;
            return;
        }
    };
    let mut port_writer = port;

    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);

    let read_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match port_reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // 超时是正常的（timeout 50ms），继续读
                    continue;
                }
                Err(_) => break,
            }
        }
    });

    // 5) 主事件循环
    loop {
        tokio::select! {
            // 串口输出 → WS Binary
            Some(data) = out_rx.recv() => {
                let _ = output_tx.send(String::from_utf8_lossy(&data).to_string());
                if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                    break;
                }
            }
            // 外部命令注入（AI sidecar）
            Some(data) = cmd_rx.recv() => {
                if port_writer.write_all(data.as_bytes()).is_err() { break; }
                let _ = port_writer.flush();
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
                                    if port_writer.write_all(data.as_bytes()).is_err() { break; }
                                    let _ = port_writer.flush();
                                }
                                Ok(ClientMsg::Resize { cols, rows }) => {
                                    log::info!("serial: resize ignored sid={sid} cols={cols} rows={rows}");
                                }
                                Ok(ClientMsg::Ping) => {
                                    let _ = ws_tx.send(Message::Text(
                                        "{\"kind\":\"pong\"}".to_string().into(),
                                    )).await;
                                }
                                Ok(ClientMsg::SudoFill) => {
                                    // 串口无 sudo 机制，忽略
                                }
                                Err(_) => {
                                    if port_writer.write_all(s.as_bytes()).is_err() { break; }
                                    let _ = port_writer.flush();
                                }
                            }
                        } else {
                            if port_writer.write_all(s.as_bytes()).is_err() { break; }
                            let _ = port_writer.flush();
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        if port_writer.write_all(&bytes).is_err() { break; }
                        let _ = port_writer.flush();
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }

    log::info!("serial cleanup sid={sid}");
    read_task.abort();
    ssh_svc::remove_terminal_channels(&sid).await;
    let _ = ws_tx.close().await;
    drop(port_writer);
}
