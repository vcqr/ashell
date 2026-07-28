//! SSH 端口转发管理（-L 本地 / -R 远程 / -D 动态 SOCKS5）
//!
//! 设计：
//! - 每个 `sid` 对应一个 [`ForwardManager`]，挂在 [`FORWARD_MANAGERS`] 全局表里。
//! - 创建规则时分配 uuid，启动 tokio 任务（本地 listener / 远端 tcpip_forward 请求），
//!   `JoinHandle` 存入条目，`shutdown_for(sid)` / `remove_rule` 时统一 abort。
//! - 远程转发的回连 channel 由 [`crate::service::ssh::ClientHandler::server_channel_open_forwarded_tcpip`]
//!   接住后调 [`accept_remote_channel`] 路由到对应规则的本地目标。
//! - 字节计数仅在 wrapper 完成 bidirectional copy 后追加，断连时丢弃当前进行中的统计，简单足够。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use russh::Channel;
use russh::client::Msg;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::errors::{AppError, AppResult};
use crate::service::ssh as ssh_svc;

// 用 once_cell 而不是 std::sync::OnceLock 与 ssh.rs 保持一致
static FORWARD_MANAGERS: Lazy<RwLock<HashMap<String, Arc<ForwardManager>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 转发类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    /// -L: 本地端口 → 远端 host:port
    Local,
    /// -R: 远端端口 → 本地 host:port
    Remote,
    /// -D: 本地 SOCKS5 代理（动态选目标）
    Dynamic,
}

/// 转发规则（前端可见）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRule {
    pub id: String,
    pub sid: String,
    pub kind: ForwardKind,
    pub bind_addr: String,
    pub bind_port: u16,
    pub dest_host: Option<String>,
    pub dest_port: Option<u16>,
    pub status: String,
    pub err: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// 创建规则的输入
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardCreate {
    pub sid: String,
    pub kind: ForwardKind,
    pub bind_addr: String,
    pub bind_port: u16,
    pub dest_host: Option<String>,
    pub dest_port: Option<u16>,
}

struct ForwardEntry {
    rule: ForwardRule,
    rx: Arc<AtomicU64>,
    tx: Arc<AtomicU64>,
    /// 最近一次连接错误（"上次错误"在表格里展示，运行期由后台任务写入）
    last_err: Arc<AsyncMutex<Option<String>>>,
    /// 本地 listener 任务（仅 -L / -D）；-R 没有本地 listener，置 None
    join: Option<JoinHandle<()>>,
}

pub struct ForwardManager {
    sid: String,
    inner: RwLock<HashMap<String, ForwardEntry>>,
}

impl ForwardManager {
    fn new(sid: String) -> Self {
        Self {
            sid,
            inner: RwLock::new(HashMap::new()),
        }
    }

    async fn snapshot(&self) -> Vec<ForwardRule> {
        let map = self.inner.read().await;
        let mut out = Vec::with_capacity(map.len());
        for e in map.values() {
            let mut rule = e.rule.clone();
            rule.rx_bytes = e.rx.load(Ordering::Relaxed);
            rule.tx_bytes = e.tx.load(Ordering::Relaxed);
            rule.err = e.last_err.lock().await.clone();
            out.push(rule);
        }
        out
    }

    async fn shutdown_all(&self) {
        let mut map = self.inner.write().await;
        for (_, mut entry) in map.drain() {
            if let Some(j) = entry.join.take() {
                j.abort();
            }
            // 远程转发还需要显式取消 sshd 那边的监听
            if matches!(entry.rule.kind, ForwardKind::Remote) {
                if let Ok(sess) = ssh_svc::get_client(&self.sid).await {
                    let _ = sess
                        .cancel_tcpip_forward(&entry.rule.bind_addr, entry.rule.bind_port)
                        .await;
                }
            }
        }
    }
}

/// 获取或创建 sid 对应的 manager
pub async fn manager_for(sid: &str) -> Arc<ForwardManager> {
    {
        let r = FORWARD_MANAGERS.read().await;
        if let Some(m) = r.get(sid) {
            return Arc::clone(m);
        }
    }
    let mut w = FORWARD_MANAGERS.write().await;
    Arc::clone(
        w.entry(sid.to_string())
            .or_insert_with(|| Arc::new(ForwardManager::new(sid.to_string()))),
    )
}

/// 列出 sid 下全部规则
pub async fn list(sid: &str) -> Vec<ForwardRule> {
    let r = FORWARD_MANAGERS.read().await;
    match r.get(sid) {
        Some(m) => m.snapshot().await,
        None => Vec::new(),
    }
}

/// 关闭 sid 关联的全部转发，并从全局表移除
pub async fn shutdown_for(sid: &str) {
    let mgr = FORWARD_MANAGERS.write().await.remove(sid);
    if let Some(m) = mgr {
        m.shutdown_all().await;
    }
}

/// 创建并启动一条规则
pub async fn create(req: ForwardCreate) -> AppResult<ForwardRule> {
    let session = ssh_svc::get_client(&req.sid).await?;
    let mgr = manager_for(&req.sid).await;

    let id = Uuid::new_v4().to_string();
    let bind_addr = if req.bind_addr.trim().is_empty() {
        "127.0.0.1".to_string()
    } else {
        req.bind_addr.trim().to_string()
    };

    match req.kind {
        ForwardKind::Local => {
            let dest_host = req
                .dest_host
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| AppError::BadRequest("dest_host required for local forward".into()))?
                .to_string();
            let dest_port = req
                .dest_port
                .ok_or_else(|| AppError::BadRequest("dest_port required for local forward".into()))?;

            let listener = bind_listener(&bind_addr, req.bind_port).await?;
            let actual_port = listener.local_addr()?.port();

            let rx = Arc::new(AtomicU64::new(0));
            let tx = Arc::new(AtomicU64::new(0));
            let last_err: Arc<AsyncMutex<Option<String>>> = Arc::new(AsyncMutex::new(None));
            let join = tokio::spawn(run_local_listener(
                listener,
                Arc::clone(&session),
                dest_host.clone(),
                dest_port,
                Arc::clone(&rx),
                Arc::clone(&tx),
                Arc::clone(&last_err),
            ));

            let rule = ForwardRule {
                id: id.clone(),
                sid: req.sid.clone(),
                kind: ForwardKind::Local,
                bind_addr,
                bind_port: actual_port,
                dest_host: Some(dest_host),
                dest_port: Some(dest_port),
                status: "running".into(),
                err: None,
                rx_bytes: 0,
                tx_bytes: 0,
            };
            mgr.inner.write().await.insert(
                id,
                ForwardEntry {
                    rule: rule.clone(),
                    rx,
                    tx,
                    last_err,
                    join: Some(join),
                },
            );
            Ok(rule)
        }
        ForwardKind::Dynamic => {
            let listener = bind_listener(&bind_addr, req.bind_port).await?;
            let actual_port = listener.local_addr()?.port();

            let rx = Arc::new(AtomicU64::new(0));
            let tx = Arc::new(AtomicU64::new(0));
            let last_err: Arc<AsyncMutex<Option<String>>> = Arc::new(AsyncMutex::new(None));
            let join = tokio::spawn(run_dynamic_listener(
                listener,
                Arc::clone(&session),
                Arc::clone(&rx),
                Arc::clone(&tx),
                Arc::clone(&last_err),
            ));

            let rule = ForwardRule {
                id: id.clone(),
                sid: req.sid.clone(),
                kind: ForwardKind::Dynamic,
                bind_addr,
                bind_port: actual_port,
                dest_host: None,
                dest_port: None,
                status: "running".into(),
                err: None,
                rx_bytes: 0,
                tx_bytes: 0,
            };
            mgr.inner.write().await.insert(
                id,
                ForwardEntry {
                    rule: rule.clone(),
                    rx,
                    tx,
                    last_err,
                    join: Some(join),
                },
            );
            Ok(rule)
        }
        ForwardKind::Remote => {
            let dest_host = req
                .dest_host
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    AppError::BadRequest("dest_host required for remote forward".into())
                })?
                .to_string();
            let dest_port = req.dest_port.ok_or_else(|| {
                AppError::BadRequest("dest_port required for remote forward".into())
            })?;

            // sshd 端发起监听
            let actual_port = session
                .request_tcpip_forward(&bind_addr, req.bind_port)
                .await?;

            let rule = ForwardRule {
                id: id.clone(),
                sid: req.sid.clone(),
                kind: ForwardKind::Remote,
                bind_addr,
                bind_port: actual_port,
                dest_host: Some(dest_host),
                dest_port: Some(dest_port),
                status: "running".into(),
                err: None,
                rx_bytes: 0,
                tx_bytes: 0,
            };
            mgr.inner.write().await.insert(
                id,
                ForwardEntry {
                    rule: rule.clone(),
                    rx: Arc::new(AtomicU64::new(0)),
                    tx: Arc::new(AtomicU64::new(0)),
                    last_err: Arc::new(AsyncMutex::new(None)),
                    join: None,
                },
            );
            Ok(rule)
        }
    }
}

/// 删除一条规则并 abort 其后台任务
pub async fn remove_rule(sid: &str, rule_id: &str) -> AppResult<()> {
    let mgr = match FORWARD_MANAGERS.read().await.get(sid).cloned() {
        Some(m) => m,
        None => return Err(AppError::NotFound(format!("forward manager {sid}"))),
    };
    let entry = mgr.inner.write().await.remove(rule_id);
    let Some(mut entry) = entry else {
        return Err(AppError::NotFound(format!("forward rule {rule_id}")));
    };
    if let Some(j) = entry.join.take() {
        j.abort();
    }
    if matches!(entry.rule.kind, ForwardKind::Remote) {
        if let Ok(sess) = ssh_svc::get_client(sid).await {
            let _ = sess
                .cancel_tcpip_forward(&entry.rule.bind_addr, entry.rule.bind_port)
                .await;
        }
    }
    Ok(())
}

/// 远程转发回连入口：sshd 收到外部连接后 client handler 被回调，最终调到这里。
/// 我们用 `(connected_address, connected_port)` 反查 -R 规则 → 再开本地 TCP 桥接。
pub async fn accept_remote_channel(
    sid: &str,
    bind_addr: String,
    bind_port: u32,
    channel: Channel<Msg>,
) {
    let mgr = match FORWARD_MANAGERS.read().await.get(sid).cloned() {
        Some(m) => m,
        None => {
            log::warn!("accept_remote_channel: no manager for sid={sid}");
            return;
        }
    };
    let target = {
        let map = mgr.inner.read().await;
        map.values()
            .find(|e| {
                e.rule.kind == ForwardKind::Remote
                    && e.rule.bind_port as u32 == bind_port
                    && (e.rule.bind_addr == bind_addr
                        || e.rule.bind_addr == "0.0.0.0"
                        || e.rule.bind_addr == "*"
                        || bind_addr == "0.0.0.0")
            })
            .map(|e| {
                (
                    e.rule.dest_host.clone().unwrap_or_default(),
                    e.rule.dest_port.unwrap_or(0),
                    Arc::clone(&e.rx),
                    Arc::clone(&e.tx),
                    Arc::clone(&e.last_err),
                )
            })
    };
    let Some((dest_host, dest_port, rx, tx, last_err)) = target else {
        log::warn!(
            "accept_remote_channel: no rule matches {bind_addr}:{bind_port} on sid={sid}"
        );
        return;
    };

    tokio::spawn(async move {
        let local = match TcpStream::connect((dest_host.as_str(), dest_port)).await {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("dial {dest_host}:{dest_port}: {e}");
                log::warn!("remote-forward {msg}");
                *last_err.lock().await = Some(msg);
                let _ = channel.eof().await;
                return;
            }
        };
        *last_err.lock().await = None;
        let mut ch_stream = channel.into_stream();
        let mut local = local;
        let (rx_b, tx_b) =
            tokio::io::copy_bidirectional(&mut ch_stream, &mut local)
                .await
                .unwrap_or((0, 0));
        rx.fetch_add(rx_b, Ordering::Relaxed);
        tx.fetch_add(tx_b, Ordering::Relaxed);
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// 内部辅助
// ─────────────────────────────────────────────────────────────────────────────

async fn bind_listener(bind_addr: &str, port: u16) -> AppResult<TcpListener> {
    let addr: SocketAddr = format!("{bind_addr}:{port}")
        .parse()
        .map_err(|e| AppError::BadRequest(format!("invalid bind addr {bind_addr}:{port}: {e}")))?;
    TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Conflict(format!("bind {addr}: {e}")))
}

/// -L 本地转发循环
async fn run_local_listener(
    listener: TcpListener,
    session: Arc<ssh_svc::Session>,
    dest_host: String,
    dest_port: u16,
    rx: Arc<AtomicU64>,
    tx: Arc<AtomicU64>,
    last_err: Arc<AsyncMutex<Option<String>>>,
) {
    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("accept: {e}");
                log::warn!("local-forward {msg}");
                *last_err.lock().await = Some(msg);
                break;
            }
        };
        let session = Arc::clone(&session);
        let dest_host = dest_host.clone();
        let rx = Arc::clone(&rx);
        let tx = Arc::clone(&tx);
        let last_err = Arc::clone(&last_err);
        tokio::spawn(async move {
            let originator_ip = peer.ip().to_string();
            let originator_port = peer.port();
            let channel = match session
                .channel_open_direct_tcpip(&dest_host, dest_port, &originator_ip, originator_port)
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("dial {dest_host}:{dest_port}: {e}");
                    log::warn!("local-forward {msg}");
                    *last_err.lock().await = Some(msg);
                    return;
                }
            };
            // 成功建立通道后清掉旧错误，避免一直挂着
            *last_err.lock().await = None;
            let mut ch_stream = channel.into_stream();
            let (rx_b, tx_b) =
                tokio::io::copy_bidirectional(&mut stream, &mut ch_stream)
                    .await
                    .unwrap_or((0, 0));
            tx.fetch_add(rx_b, Ordering::Relaxed); // 客户端 → 远端 = upload
            rx.fetch_add(tx_b, Ordering::Relaxed); // 远端 → 客户端 = download
        });
    }
}

/// -D 动态转发循环（SOCKS5 子集：NOAUTH + CONNECT，IPv4 / IPv6 / DOMAIN）
async fn run_dynamic_listener(
    listener: TcpListener,
    session: Arc<ssh_svc::Session>,
    rx: Arc<AtomicU64>,
    tx: Arc<AtomicU64>,
    last_err: Arc<AsyncMutex<Option<String>>>,
) {
    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("accept: {e}");
                log::warn!("dynamic-forward {msg}");
                *last_err.lock().await = Some(msg);
                break;
            }
        };
        let session = Arc::clone(&session);
        let rx = Arc::clone(&rx);
        let tx = Arc::clone(&tx);
        let last_err = Arc::clone(&last_err);
        tokio::spawn(async move {
            let (host, port) = match socks5_handshake(&mut stream).await {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("socks5 handshake: {e}");
                    log::warn!("{msg}");
                    *last_err.lock().await = Some(msg);
                    return;
                }
            };
            let originator_ip = peer.ip().to_string();
            let originator_port = peer.port();
            let channel = match session
                .channel_open_direct_tcpip(&host, port, &originator_ip, originator_port)
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("dial {host}:{port}: {e}");
                    log::warn!("dynamic-forward {msg}");
                    *last_err.lock().await = Some(msg);
                    let _ = stream.shutdown().await;
                    return;
                }
            };
            *last_err.lock().await = None;
            let mut ch_stream = channel.into_stream();
            let (rx_b, tx_b) =
                tokio::io::copy_bidirectional(&mut stream, &mut ch_stream)
                    .await
                    .unwrap_or((0, 0));
            tx.fetch_add(rx_b, Ordering::Relaxed);
            rx.fetch_add(tx_b, Ordering::Relaxed);
        });
    }
}

/// 极简 SOCKS5 握手；仅支持 NOAUTH + CONNECT。返回目标 host:port。
async fn socks5_handshake(stream: &mut TcpStream) -> std::io::Result<(String, u16)> {
    use std::io;
    // greeting: VER NMETHODS METHODS...
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await?;
    if head[0] != 0x05 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not socks5"));
    }
    let nmethods = head[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0x00) {
        // no acceptable methods
        stream.write_all(&[0x05, 0xFF]).await?;
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "only NOAUTH supported",
        ));
    }
    stream.write_all(&[0x05, 0x00]).await?;

    // request: VER CMD RSV ATYP DST.ADDR DST.PORT
    let mut req = [0u8; 4];
    stream.read_exact(&mut req).await?;
    if req[0] != 0x05 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad ver"));
    }
    if req[1] != 0x01 {
        // command not supported
        let _ = reply_socks5(stream, 0x07).await;
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only CONNECT supported",
        ));
    }
    let host: String = match req[3] {
        0x01 => {
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await?;
            std::net::Ipv4Addr::from(buf).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut buf = vec![0u8; len[0] as usize];
            stream.read_exact(&mut buf).await?;
            String::from_utf8(buf)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid hostname"))?
        }
        0x04 => {
            let mut buf = [0u8; 16];
            stream.read_exact(&mut buf).await?;
            std::net::Ipv6Addr::from(buf).to_string()
        }
        _ => {
            let _ = reply_socks5(stream, 0x08).await;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported atyp",
            ));
        }
    };
    let mut port = [0u8; 2];
    stream.read_exact(&mut port).await?;
    let port = u16::from_be_bytes(port);

    // 回包 success，BND.ADDR/BND.PORT 用 0
    reply_socks5(stream, 0x00).await?;
    Ok((host, port))
}

async fn reply_socks5(stream: &mut TcpStream, code: u8) -> std::io::Result<()> {
    // VER REP RSV ATYP=ipv4 BND.ADDR(4) BND.PORT(2)
    stream
        .write_all(&[0x05, code, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
}
