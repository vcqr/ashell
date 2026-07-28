//! SSH/SFTP 会话管理
//!
//! 设计：
//! - 每个 WebSocket 终端连接建立时生成 sid，连接 SSH 后既：
//!   1) 注册到 `SSH_CLIENT_MAP[sid]`，保留 client.Handle，便于后续执行命令
//!   2) 同时打开一个 sftp 子通道并放入 `SFTP_SESSION_MAP[sid]`
//! - REST SFTP 接口（list / mkdir / upload / download ...）通过 sid 复用同一 SSH 连接
//! - WebSocket 关闭或显式 close 时统一从两个 map 中移除并断开

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::Lazy;
use russh::client::{self, Handle, Handler, Msg, Session as ClientSession};
use russh::keys::{decode_secret_key, PrivateKeyWithHashAlg};
use russh::{Channel, ChannelMsg, ChannelOpenHandleInner, Disconnect};
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex as AsyncMutex, RwLock, broadcast, mpsc};

use crate::errors::{AppError, AppResult};
use crate::models::Host;
use crate::service::forward as forward_svc;
use crate::service::sftp as sftp_svc;

static SSH_CLIENT_MAP: Lazy<RwLock<HashMap<String, Arc<Session>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static SFTP_SESSION_MAP: Lazy<RwLock<HashMap<String, Arc<SftpSession>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 终端会话的外部命令注入通道（POST /api/ssh/send/{sid} → SSH stdin）
static TERMINAL_SENDER_MAP: Lazy<RwLock<HashMap<String, mpsc::UnboundedSender<String>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 终端会话的 SSH 输出广播通道（SSH stdout/stderr → send_handler 收集）
static TERMINAL_OUTPUT_MAP: Lazy<RwLock<HashMap<String, broadcast::Sender<String>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 命令执行结果
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CommandExecutedResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: u32,
}

/// russh client handler，全部接受 server key（TOFU 简化版）。
///
/// 携带一个共享的 `sid`：会话建立时尚未分配，由调用方在 `Session::connect` 之后
/// 用 [`Session::attach_sid`] 回填。一旦写入，远程转发回来的 channel 就能据此
/// 路由到 [`crate::service::forward`] 中对应的规则。
///
/// TODO: 后续接入 known_hosts 校验
pub struct ClientHandler {
    sid: Arc<AsyncMutex<Option<String>>>,
}

impl ClientHandler {
    fn new() -> (Self, Arc<AsyncMutex<Option<String>>>) {
        let sid = Arc::new(AsyncMutex::new(None));
        (
            Self {
                sid: Arc::clone(&sid),
            },
            sid,
        )
    }
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    /// 远程端口转发（-R）回连：sshd 收到外部连接后会以这种 channel 通知客户端。
    /// 我们根据 `connected_address:connected_port` 反查 sid 对应的转发规则，
    /// 把 channel 桥接到本地目标地址。
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _handle: ChannelOpenHandleInner<Msg>,
        _session: &mut ClientSession,
    ) -> Result<(), Self::Error> {
        let sid = self.sid.lock().await.clone();
        let Some(sid) = sid else {
            log::warn!(
                "forwarded-tcpip ignored: handler has no sid (channel={:?})",
                channel.id()
            );
            return Ok(());
        };
        forward_svc::accept_remote_channel(
            &sid,
            connected_address.to_string(),
            connected_port,
            channel,
        )
        .await;
        Ok(())
    }
}

/// 已建立的 SSH 会话
///
/// `handle` 包在 `AsyncMutex` 里是为了让 `tcpip_forward` / `cancel_tcpip_forward`
/// 这种需要 `&mut self` 的 russh API 也能在 `Arc<Session>` 共享场景下调用。
/// 其它仅需 `&self` 的方法（开 channel / disconnect）也走同一把锁，开销可忽略。
pub struct Session {
    handle: AsyncMutex<Handle<ClientHandler>>,
    sid_slot: Arc<AsyncMutex<Option<String>>>,
}

impl Session {
    /// 通过主机配置（凭证已解密）建立 SSH 连接
    pub async fn connect(host: &Host) -> AppResult<Self> {
        let port: u16 = host
            .port
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid port: {}", host.port)))?;
        let addr = (host.addr.as_str(), port);

        let config = Arc::new(client::Config {
            keepalive_interval: Some(Duration::from_secs(30)),
            inactivity_timeout: Some(Duration::from_secs(120)),
            ..Default::default()
        });

        let (handler, sid_slot) = ClientHandler::new();
        let mut handle = client::connect(config, addr, handler)
            .await
            .map_err(|e| AppError::Ssh(format!("connect: {e}")))?;

        // 优先使用密码认证（行为与 demo 保持一致）
        let mut authed = false;
        if let Some(pwd) = host.password.as_deref() {
            if !pwd.is_empty() {
                let res = handle
                    .authenticate_password(&host.username, pwd)
                    .await
                    .map_err(|e| AppError::Ssh(format!("auth password: {e}")))?;
                authed = res.success();
            }
        }

        if !authed {
            // 私钥内容：优先使用粘贴的 private_key，否则从 private_key_path 读取文件
            let pk_str = if let Some(pk) = host.private_key.as_deref() {
                if !pk.is_empty() {
                    Some(pk.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            let pk_str = match pk_str {
                Some(s) => Some(s),
                None => {
                    if let Some(path) = host.private_key_path.as_deref() {
                        if !path.is_empty() {
                            Some(std::fs::read_to_string(path).map_err(|e| {
                                AppError::Ssh(format!("read private key file {}: {e}", path))
                            })?)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };

            if let Some(pk_str) = pk_str {
                let key = decode_secret_key(&pk_str, None)
                    .map_err(|e| AppError::Ssh(format!("decode private key: {e}")))?;
                let key = PrivateKeyWithHashAlg::new(Arc::new(key), None);
                let res = handle
                    .authenticate_publickey(&host.username, key)
                    .await
                    .map_err(|e| AppError::Ssh(format!("auth pubkey: {e}")))?;
                authed = res.success();
            }
        }

        if !authed {
            return Err(AppError::Ssh("authentication failed".into()));
        }

        Ok(Self {
            handle: AsyncMutex::new(handle),
            sid_slot,
        })
    }

    /// 把 sid 写入 client handler，使后续远程转发回连能路由到本会话
    pub async fn attach_sid(&self, sid: &str) {
        *self.sid_slot.lock().await = Some(sid.to_string());
    }

    /// 远程端口转发请求：让远端 sshd 监听 `bind_addr:bind_port`，
    /// 收到连接后通过 ssh 通道反向打回本地 handler。
    /// `port==0` 时由 sshd 分配，返回实际端口。
    pub async fn request_tcpip_forward(
        &self,
        bind_addr: &str,
        bind_port: u16,
    ) -> AppResult<u16> {
        let actual = self
            .handle
            .lock()
            .await
            .tcpip_forward(bind_addr.to_string(), bind_port as u32)
            .await
            .map_err(|e| AppError::Ssh(format!("tcpip_forward: {e}")))?;
        Ok(if bind_port == 0 {
            actual as u16
        } else {
            bind_port
        })
    }

    /// 取消远程端口转发
    pub async fn cancel_tcpip_forward(
        &self,
        bind_addr: &str,
        bind_port: u16,
    ) -> AppResult<()> {
        self.handle
            .lock()
            .await
            .cancel_tcpip_forward(bind_addr.to_string(), bind_port as u32)
            .await
            .map_err(|e| AppError::Ssh(format!("cancel_tcpip_forward: {e}")))?;
        Ok(())
    }

    /// 打开 direct-tcpip 通道（本地转发 -L / 动态转发 -D 用）
    pub async fn channel_open_direct_tcpip(
        &self,
        host: &str,
        port: u16,
        originator: &str,
        originator_port: u16,
    ) -> AppResult<Channel<Msg>> {
        self.handle
            .lock()
            .await
            .channel_open_direct_tcpip(
                host.to_string(),
                port as u32,
                originator.to_string(),
                originator_port as u32,
            )
            .await
            .map_err(|e| AppError::Ssh(format!("direct_tcpip: {e}")))
    }

    /// 打开普通 session channel（终端 / 命令执行 / SFTP 子系统）
    pub async fn channel_open_session(&self) -> AppResult<Channel<Msg>> {
        self.handle
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(|e| AppError::Ssh(format!("open channel: {e}")))
    }

    /// 执行远程命令，返回完整 stdout/stderr/exit_status
    pub async fn execute(&self, command: &str) -> AppResult<CommandExecutedResult> {
        let mut ch = self.channel_open_session().await?;

        ch.exec(true, command)
            .await
            .map_err(|e| AppError::Ssh(format!("exec: {e}")))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit: Option<u32> = None;

        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { ref data } => {
                    let _ = stdout.write_all(data).await;
                }
                ChannelMsg::ExtendedData { ref data, ext } => {
                    if ext == 1 {
                        let _ = stderr.write_all(data).await;
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => exit = Some(exit_status),
                ChannelMsg::Eof | ChannelMsg::Close => {}
                _ => {}
            }
        }

        Ok(CommandExecutedResult {
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            exit_status: exit.unwrap_or(0),
        })
    }

    /// 在该会话上打开 sftp 子通道并保存到 sid 映射
    pub async fn open_sftp(&self, sid: &str) -> AppResult<Arc<SftpSession>> {
        let channel = self.channel_open_session().await?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| sftp_svc::sftp_err("request subsystem", e))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| sftp_svc::sftp_err("init", e))?;
        let arc = Arc::new(sftp);
        SFTP_SESSION_MAP
            .write()
            .await
            .insert(sid.to_string(), arc.clone());
        Ok(arc)
    }

    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .lock()
            .await
            .disconnect(Disconnect::ByApplication, "bye", "en")
            .await;
    }
}

/// 注册 sid -> SSH session
pub async fn set_client(sid: String, sess: Arc<Session>) {
    SSH_CLIENT_MAP.write().await.insert(sid, sess);
}

/// 获取 sid 对应的 SSH session（用于执行命令）
pub async fn get_client(sid: &str) -> AppResult<Arc<Session>> {
    SSH_CLIENT_MAP
        .read()
        .await
        .get(sid)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("ssh session {sid}")))
}

/// 获取 sid 对应的 SFTP 子会话
pub async fn get_sftp(sid: &str) -> AppResult<Arc<SftpSession>> {
    SFTP_SESSION_MAP
        .read()
        .await
        .get(sid)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("sftp session {sid}")))
}

/// 注册终端会话的命令注入与输出广播通道
pub async fn set_terminal_channels(
    sid: &str,
    sender: mpsc::UnboundedSender<String>,
    output: broadcast::Sender<String>,
) {
    TERMINAL_SENDER_MAP
        .write()
        .await
        .insert(sid.to_string(), sender);
    TERMINAL_OUTPUT_MAP
        .write()
        .await
        .insert(sid.to_string(), output);
}

/// 获取终端会话的命令注入通道
pub async fn get_terminal_sender(sid: &str) -> AppResult<mpsc::UnboundedSender<String>> {
    TERMINAL_SENDER_MAP
        .read()
        .await
        .get(sid)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("terminal sender {sid}")))
}

/// 获取终端会话的输出广播通道
pub async fn get_terminal_output(sid: &str) -> AppResult<broadcast::Sender<String>> {
    TERMINAL_OUTPUT_MAP
        .read()
        .await
        .get(sid)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("terminal output {sid}")))
}

/// 移除终端会话的命令注入与输出广播通道
pub async fn remove_terminal_channels(sid: &str) {
    TERMINAL_SENDER_MAP.write().await.remove(sid);
    TERMINAL_OUTPUT_MAP.write().await.remove(sid);
}

/// 释放 sid 关联的会话（终端 WS 关闭或显式 close 时调用）
pub async fn remove(sid: &str) {
    // 先停掉所有端口转发任务（释放本地 listener / 取消远端转发请求）
    forward_svc::shutdown_for(sid).await;
    remove_terminal_channels(sid).await;
    let sftp = SFTP_SESSION_MAP.write().await.remove(sid);
    drop(sftp);
    if let Some(sess) = SSH_CLIENT_MAP.write().await.remove(sid) {
        sess.disconnect().await;
    }
}
