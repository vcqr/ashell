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
use crate::models::{DbPool, Host};
use crate::service::forward as forward_svc;
use crate::service::host as host_svc;
use crate::service::known_host;
use crate::service::sftp as sftp_svc;

static SSH_CLIENT_MAP: Lazy<RwLock<HashMap<String, Arc<Session>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static SFTP_SESSION_MAP: Lazy<RwLock<HashMap<String, Arc<SftpSession>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// sid -> 当前 SFTP 会话是否为 sudo 提权（root）。
/// 终端重连会用同一 sid 重建普通 sftp 会话，靠这个标记感知"提权被覆盖"并自动恢复。
static SFTP_ELEVATED_MAP: Lazy<RwLock<HashMap<String, bool>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 远端 sftp-server 二进制路径探测（POSIX sh，BusyBox 兼容）。
/// 优先 PATH 内 `command -v`，再测各发行版常见固定路径；找不到 exit 3。
const SFTP_SERVER_DETECT: &str = "p=$(command -v sftp-server 2>/dev/null); \
    [ -z \"$p\" ] && p=$(ls /usr/lib/openssh/sftp-server /usr/libexec/openssh/sftp-server \
/usr/lib/ssh/sftp-server /usr/lib/sftp-server /usr/libexec/sftp-server \
/usr/local/libexec/sftp-server /usr/local/lib/openssh/sftp-server \
/opt/homebrew/libexec/sftp-server /usr/local/sbin/sftp-server 2>/dev/null | head -n 1); \
    [ -n \"$p\" ] && [ -x \"$p\" ] && echo \"$p\" && exit 0; exit 3";

/// 把 sudo 拒绝时的 stderr 摘要整理成错误信息（空则退化为 exit code）
fn stderr_detail(stderr: &[u8], exit_status: u32) -> String {
    let msg = String::from_utf8_lossy(stderr).trim().to_string();
    if msg.is_empty() {
        format!("exit status {exit_status}")
    } else {
        msg
    }
}

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

/// check_server_key 拒绝密钥时留下的待确认信息：握手会以 UnknownKey 失败，
/// connect_* 据此还原成 [`AppError::HostKeyVerify`] 交给上层发起用户确认
#[derive(Debug, Clone)]
struct PendingHostKey {
    key_type: String,
    fingerprint: String,
    /// 库中已存指纹（变更场景）；首次连接为 None
    previous: Option<String>,
}

/// russh client handler：known_hosts 指纹校验（TOFU）。
///
/// 携带一个共享的 `sid`：会话建立时尚未分配，由调用方在 `Session::connect` 之后
/// 用 [`Session::attach_sid`] 回填。一旦写入，远程转发回来的 channel 就能据此
/// 路由到 [`crate::service::forward`] 中对应的规则。
pub struct ClientHandler {
    sid: Arc<AsyncMutex<Option<String>>>,
    /// 被连主机身份：known_hosts 按 addr + port + key_type 查询
    host_id: i64,
    addr: String,
    port: u16,
    pool: DbPool,
    pending_key: Arc<AsyncMutex<Option<PendingHostKey>>>,
}

impl ClientHandler {
    fn new(
        host_id: i64,
        addr: String,
        port: u16,
        pool: DbPool,
    ) -> (
        Self,
        Arc<AsyncMutex<Option<String>>>,
        Arc<AsyncMutex<Option<PendingHostKey>>>,
    ) {
        let sid = Arc::new(AsyncMutex::new(None));
        let pending_key = Arc::new(AsyncMutex::new(None));
        (
            Self {
                sid: Arc::clone(&sid),
                host_id,
                addr,
                port,
                pool,
                pending_key: Arc::clone(&pending_key),
            },
            sid,
            pending_key,
        )
    }
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    /// 服务器主动断开（DISCONNECT）时，默认实现会把 reason_code / message 吞掉，
    /// 只返回无详情的 `Error::Disconnect`。这里重写以打印服务器给的真实原因，
    /// 便于定位"connect: Disconnected"这类握手阶段断连。
    async fn disconnected(
        &mut self,
        reason: client::DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        match reason {
            client::DisconnectReason::ReceivedDisconnect(info) => {
                log::warn!(
                    "SSH server disconnected: reason={:?}, message=\"{}\"",
                    info.reason_code,
                    info.message
                );
                Ok(())
            }
            client::DisconnectReason::Error(e) => Err(e),
        }
    }

    /// 主机密钥校验（TOFU）：与 known_hosts 中已信任指纹比对。
    /// - 一致：静默放行
    /// - 首次连接 / 指纹变更：记录待确认信息后拒绝握手，由上层发起用户确认，
    ///   确认信任写入库后重试连接即走上一分支
    /// - 库查询失败：fail-closed 拒绝连接（不静默放行未知密钥）
    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let pk = server_public_key.public_key();
        let fingerprint = pk
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let key_type = pk.algorithm().to_string();
        match known_host::lookup(&self.pool, &self.addr, self.port, &key_type).await {
            Ok(Some(saved)) if saved == fingerprint => Ok(true),
            Ok(previous) => {
                *self.pending_key.lock().await = Some(PendingHostKey {
                    key_type,
                    fingerprint,
                    previous,
                });
                Ok(false)
            }
            Err(e) => {
                log::error!(
                    "known_hosts lookup failed for {}:{} (host {}): {e}",
                    self.addr,
                    self.port,
                    self.host_id
                );
                Ok(false)
            }
        }
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
    /// 跳板机会话：目标连接建立在其 direct-tcpip 通道之上，
    /// 必须与目标会话同生命周期（断开目标后一并断开）
    jump: Option<Box<Session>>,
}

impl Session {
    /// 通过主机配置（凭证已解密）建立 SSH 连接。
    ///
    /// 若配置了 `jump_host_id`：先连接跳板机，在其上开 direct-tcpip 通道
    /// 隧道到目标机，再在隧道上完成目标机的 SSH 握手（仅支持一级跳板）。
    pub async fn connect(pool: &DbPool, crypto_key: &[u8; 32], host: &Host) -> AppResult<Self> {
        let Some(jump_id) = host.jump_host_id else {
            return Self::connect_direct(pool, host).await;
        };

        let jump_host = host_svc::get_with_credentials(pool, crypto_key, jump_id)
            .await
            .map_err(|e| AppError::Ssh(format!("load jump host {jump_id}: {e}")))?;
        Self::connect_via(pool, &jump_host, host).await
    }

    /// 用调用方准备好的（已解密）跳板机配置连接目标主机。
    ///
    /// 终端 WS 在认证失败后的交互式重试中会缓存并原地修改主机凭证，
    /// 需要绕过 [`Session::connect`] 内部的重新加载，故单独暴露此入口。
    pub async fn connect_prepared(
        pool: &DbPool,
        jump_host: Option<&Host>,
        host: &Host,
    ) -> AppResult<Self> {
        match jump_host {
            Some(j) => Self::connect_via(pool, j, host).await,
            None => Self::connect_direct(pool, host).await,
        }
    }

    /// 经由已解密的跳板机配置连接目标主机（仅支持一级跳板）
    async fn connect_via(pool: &DbPool, jump_host: &Host, host: &Host) -> AppResult<Self> {
        if jump_host.protocol != "ssh" {
            return Err(AppError::Ssh("jump host must use SSH protocol".into()));
        }

        let port: u16 = host
            .port
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid port: {}", host.port)))?;
        let jump_sess = Self::connect_direct(pool, jump_host).await?;
        let channel = jump_sess
            .channel_open_direct_tcpip(&host.addr, port, "127.0.0.1", 0)
            .await
            .map_err(|e| AppError::Ssh(format!("jump host direct_tcpip: {e}")))?;

        let mut sess = Self::connect_stream(pool, host, channel.into_stream()).await?;
        sess.jump = Some(Box::new(jump_sess));
        Ok(sess)
    }

    /// 直接 TCP 连接目标主机
    async fn connect_direct(pool: &DbPool, host: &Host) -> AppResult<Self> {
        let port: u16 = host
            .port
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid port: {}", host.port)))?;
        let addr = (host.addr.as_str(), port);

        let (handler, sid_slot, pending_key) =
            ClientHandler::new(host.id, host.addr.clone(), port, pool.clone());
        let result = client::connect(Self::build_config(host), addr, handler).await;
        let mut handle = match result {
            Ok(h) => h,
            Err(e) => {
                log::error!("SSH connect failed for {}:{}: {e:?}", host.addr, port);
                return Err(Self::map_connect_err(e, &pending_key, host, port).await);
            }
        };

        Self::authenticate(&mut handle, host).await?;

        Ok(Self {
            handle: AsyncMutex::new(handle),
            sid_slot,
            jump: None,
        })
    }

    /// 在已有流上完成 SSH 握手（跳板机 direct-tcpip 通道）
    async fn connect_stream<R>(pool: &DbPool, host: &Host, stream: R) -> AppResult<Self>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let port: u16 = host
            .port
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid port: {}", host.port)))?;
        let (handler, sid_slot, pending_key) =
            ClientHandler::new(host.id, host.addr.clone(), port, pool.clone());
        let result = client::connect_stream(Self::build_config(host), stream, handler).await;
        let mut handle = match result {
            Ok(h) => h,
            Err(e) => return Err(Self::map_connect_err(e, &pending_key, host, port).await),
        };

        Self::authenticate(&mut handle, host).await?;

        Ok(Self {
            handle: AsyncMutex::new(handle),
            sid_slot,
            jump: None,
        })
    }

    /// 握手失败时优先还原"主机密钥待确认"：check_server_key 拒绝密钥后 russh
    /// 以 UnknownKey 终止握手，真正的待确认信息留在 handler 的 pending 槽里
    async fn map_connect_err(
        e: russh::Error,
        pending: &Arc<AsyncMutex<Option<PendingHostKey>>>,
        host: &Host,
        port: u16,
    ) -> AppError {
        if let Some(p) = pending.lock().await.take() {
            return AppError::HostKeyVerify {
                host_id: host.id,
                addr: host.addr.clone(),
                port,
                key_type: p.key_type,
                fingerprint: p.fingerprint,
                previous: p.previous,
            };
        }
        AppError::Ssh(format!("connect: {e}"))
    }

    fn build_config(host: &Host) -> Arc<client::Config> {
        Arc::new(client::Config {
            // 语义：null = 未配置（用默认）；0/负数 = 禁用；> 0 = 自定义
            keepalive_interval: match host.keepalive_interval {
                Some(v) if v > 0 => Some(Duration::from_secs(v as u64)),
                Some(_) => None,
                None => Some(Duration::from_secs(30)),
            },
            inactivity_timeout: match host.inactivity_timeout {
                Some(v) if v > 0 => Some(Duration::from_secs(v as u64)),
                Some(_) => None,
                None => Some(Duration::from_secs(120)),
            },
            ..Default::default()
        })
    }

    /// 密码 / 私钥认证（先密码后私钥），失败返回错误
    async fn authenticate(handle: &mut Handle<ClientHandler>, host: &Host) -> AppResult<()> {
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
            // 结构化错误：携带主机 id，终端 WS 据此发起交互式重新输入密码；
            // 其余调用方（如独立 SFTP 会话）经 IntoResponse 映射为 401。
            return Err(AppError::AuthFailed { host_id: host.id });
        }
        Ok(())
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
        SFTP_ELEVATED_MAP.write().await.insert(sid.to_string(), false);
        Ok(arc)
    }

    /// 以 sudo 提权打开 sftp 会话（临时提权）：不走 request_subsystem（sshd 会以
    /// 登录用户身份拉起 sftp-server，SFTP 协议本身无提权概念），改为 exec
    /// `sudo <sftp-server>` 让 sftp-server 以 root 运行，SFTP 协议流原样走
    /// channel stdin/stdout。
    ///
    /// 流程：探测远端 sftp-server 二进制路径 -> 探测 NOPASSWD（`sudo -n true`）
    /// -> 直接 exec，或 `sudo -S -p ''` 启动后从 stdin 喂一行密码。
    /// sudo 拒绝（密码错 / 不在 sudoers）会在 stderr 输出原因并快速退出，
    /// 据此报错；静默放行即进入 sftp 协议握手。
    pub async fn open_sftp_elevated(
        &self,
        sid: &str,
        password: Option<&str>,
    ) -> AppResult<Arc<SftpSession>> {
        let detect = self
            .execute(SFTP_SERVER_DETECT)
            .await
            .map_err(|e| AppError::Ssh(format!("elevate detect: {e}")))?;
        let path = detect.stdout.trim().to_string();
        if detect.exit_status != 0 || path.is_empty() {
            return Err(AppError::BadRequest(
                "sftp-server binary not found on remote host".into(),
            ));
        }

        let nopasswd = self
            .execute("sudo -n true 2>/dev/null")
            .await
            .map(|r| r.exit_status == 0)
            .unwrap_or(false);

        let quoted = format!("'{path}'");
        match (nopasswd, password.filter(|p| !p.is_empty())) {
            (true, _) => self
                .open_sftp_via(sid, &format!("sudo {quoted}"), None)
                .await,
            (false, Some(pwd)) => {
                let cmd = format!("sudo -S -p '' {quoted}");
                self.open_sftp_via(sid, &cmd, Some(pwd)).await
            }
            (false, None) => Err(AppError::BadRequest(
                "ELEVATE_PASSWORD_REQUIRED".into(),
            )),
        }
    }

    /// exec 一条"最终 exec sftp-server"的命令并完成 SFTP 握手。
    /// `password` 非空时先写一行到 stdin（`sudo -S` 从 stdin 读密码）。
    async fn open_sftp_via(
        &self,
        sid: &str,
        cmd: &str,
        password: Option<&str>,
    ) -> AppResult<Arc<SftpSession>> {
        let mut channel = self.channel_open_session().await?;
        channel
            .exec(true, cmd)
            .await
            .map_err(|e| AppError::Ssh(format!("elevate exec: {e}")))?;
        if let Some(pwd) = password {
            let mut line = pwd.to_string();
            line.push('\n');
            channel
                .data(line.as_bytes())
                .await
                .map_err(|e| AppError::Ssh(format!("send sudo password: {e}")))?;
        }
        // sudo 验证失败会往 stderr 写原因并退出（ExitStatus/Close 很快到达）；
        // 成功则静默放行进入 sftp-server，被动等 INIT、无输出 —— 超时即成功。
        let mut stderr: Vec<u8> = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_millis(1500), channel.wait()).await {
                Err(_) => break,
                Ok(Some(ChannelMsg::ExtendedData { ref data, ext: 1 })) => {
                    stderr.extend_from_slice(data);
                }
                Ok(Some(ChannelMsg::ExitStatus { exit_status })) => {
                    return Err(AppError::Ssh(format!(
                        "sudo rejected: {}",
                        stderr_detail(&stderr, exit_status)
                    )));
                }
                Ok(Some(ChannelMsg::Close)) | Ok(Some(ChannelMsg::Eof)) => {
                    return Err(AppError::Ssh(format!(
                        "sudo rejected: {}",
                        stderr_detail(&stderr, 0)
                    )));
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(AppError::Ssh(
                        "sftp channel closed during elevate".into(),
                    ));
                }
            }
        }
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| sftp_svc::sftp_err("elevated init", e))?;

        // root 自检：提权会话应能以 READ 打开 /root（各发行版均为 700，仅 root 可读）。
        // 打开报 permission denied 说明会话实际不是 root（提权未生效）；目录不存在
        // （macOS 为 /var/root 等非标准布局）则跳过判定不阻塞。
        match sftp
            .open_with_flags("/root", russh_sftp::protocol::OpenFlags::READ)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("permission") || msg.contains("denied") {
                    return Err(AppError::Ssh(
                        "elevation self-check failed: sftp session is not running as root".into(),
                    ));
                }
            }
        }

        let arc = Arc::new(sftp);
        SFTP_SESSION_MAP
            .write()
            .await
            .insert(sid.to_string(), arc.clone());
        SFTP_ELEVATED_MAP.write().await.insert(sid.to_string(), true);
        Ok(arc)
    }

    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .lock()
            .await
            .disconnect(Disconnect::ByApplication, "bye", "en")
            .await;
        if let Some(jump) = &self.jump {
            // Box::pin 打断递归 future（jump 实际只有一层）
            Box::pin(jump.disconnect()).await;
        }
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

/// 该 sid 的 SFTP 会话当前是否为提权（root）会话；未知 sid 视为否
pub async fn get_sftp_elevated(sid: &str) -> bool {
    SFTP_ELEVATED_MAP
        .read()
        .await
        .get(sid)
        .copied()
        .unwrap_or(false)
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
    SFTP_ELEVATED_MAP.write().await.remove(sid);
    if let Some(sess) = SSH_CLIENT_MAP.write().await.remove(sid) {
        sess.disconnect().await;
    }
}
