//! AI sidecar daemon 宿主管理。
//!
//! app-ai 以 `--serve` 形态作为全应用唯一的常驻 daemon 运行，进程内按 ssid
//! 承载多个 AI 会话；宿主与其之间是 NDJSON 帧总线（见 sidecar-ai/src/daemon.ts）：
//! - 下行：create / user / stop / close 控制帧
//! - 上行：event 帧（按 ssid 解复用）+ create 的 ok/err 应答
//!
//! 对外接口保持与旧版一致（spawn/write/kill/has/pid + `sidecar-stdout-{ssid}`
//! 事件），前端与 Web 端点零改动。会话注册表取代旧版进程表：
//! - spawn  → 懒启动 daemon（如未运行）+ 下发 create 帧并等待应答
//! - write  → 数据行转 user 帧，`__STOP__` 转 stop 帧
//! - kill   → 下发 close 帧（daemon 内部回收引擎状态），进程本身不动
//! - kill_all → 终止 daemon 进程（unix 走进程组，连带回收 CLI 子进程）

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio::sync::oneshot;

#[cfg(feature = "desktop")]
use tauri::Emitter;

#[cfg(unix)]
use std::os::unix::io::FromRawFd;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// sidecar 单行输出事件。桌面端经 Tauri event 推送；Web 端经
/// `/api/ai/sidecar/{ssid}/stream` WebSocket 广播（见 [`broadcast`] 通道）。
#[derive(Debug, Clone, Serialize)]
pub struct SidecarEvent {
    /// "stdout" | "stderr"
    pub stream: &'static str,
    pub line: String,
}

/// create 帧等待 daemon 应答的超时。
/// 引擎创建本身很快（查路径 / 读注册表）；超时大概率意味着 app-ai 二进制过旧、
/// 不认识帧协议，报错信息里给出这个提示。
const CREATE_ACK_TIMEOUT: Duration = Duration::from_secs(15);

/// 会话条目：daemon 内部承载，宿主只保留事件广播通道
struct SessionEntry {
    events: broadcast::Sender<SidecarEvent>,
}

struct DaemonHandle {
    pid: u32,
    child: Child,
    stdin: Mutex<Box<dyn Write + Send>>,
    next_id: AtomicU64,
    /// create/close 等待 daemon 应答的挂起点（reader 线程解析 ok/err 后 resolve）
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<(), String>>>>,
    #[cfg(feature = "desktop")]
    app: tauri::AppHandle,
}

struct DaemonState {
    handle: DaemonHandle,
    sessions: HashMap<String, SessionEntry>,
}

/// 全局 daemon 状态：None 表示 daemon 未运行
static DAEMON: Mutex<Option<DaemonState>> = Mutex::new(None);

/// 桌面形态的 Tauri 事件桥类型；Web 形态下无 Tauri 运行时，用单元类型占位，
/// 使 spawn_sidecar_impl 在两种 target 下签名一致（bridge 恒为 None）。
#[cfg(feature = "desktop")]
pub type AppEventBridge = tauri::AppHandle;
#[cfg(not(feature = "desktop"))]
pub type AppEventBridge = ();

// ── daemon 进程管理 ──

/// 懒启动 daemon（已运行则直接返回 pid）。持锁完成 spawn，避免并发双启动。
fn ensure_daemon(bridge: Option<&AppEventBridge>) -> Result<u32, String> {
    // Web 形态无 Tauri 事件桥，参数仅用于与桌面形态统一签名
    #[cfg(not(feature = "desktop"))]
    let _ = bridge;

    let mut guard = DAEMON.lock().map_err(|e| e.to_string())?;
    if let Some(state) = guard.as_ref() {
        return Ok(state.handle.pid);
    }

    let binary_path = crate::sidecar_factory::find_sidecar_binary(
        crate::sidecar_factory::TYPE_CLAUDE,
    )?;

    tracing::info!("[DAEMON] spawning daemon: {:?}", binary_path);

    #[cfg(unix)]
    let (mut child, stdin) = spawn_daemon_unix(&binary_path)?;
    #[cfg(not(unix))]
    let (mut child, stdin) = spawn_daemon_windows(&binary_path)?;

    let pid = child.id();

    // stdout 承载帧协议；stderr 仅调试日志
    spawn_stdout_reader(child.stdout.take());
    spawn_stderr_reader(child.stderr.take());

    #[cfg(feature = "desktop")]
    let app = bridge.cloned().ok_or_else(|| {
        "sidecar daemon requires a Tauri AppHandle bridge".to_string()
    })?;

    *guard = Some(DaemonState {
        handle: DaemonHandle {
            pid,
            child,
            stdin: Mutex::new(stdin),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            #[cfg(feature = "desktop")]
            app,
        },
        sessions: HashMap::new(),
    });

    tracing::info!("[DAEMON] daemon started with PID: {}", pid);
    Ok(pid)
}

#[cfg(unix)]
fn spawn_daemon_unix(binary_path: &Path) -> Result<(Child, Box<dyn Write + Send>), String> {
    // Unix: 使用 socketpair 作为 stdin（bun 编译的二进制对 pipe 不触发事件循环）
    let mut stdin_sockets = [0i32; 2];
    unsafe {
        let rc = libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_STREAM,
            0,
            stdin_sockets.as_mut_ptr(),
        );
        if rc != 0 {
            return Err(format!(
                "Failed to create socketpair: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    let (stdin_read, stdin_write) = (stdin_sockets[0], stdin_sockets[1]);

    let child = unsafe {
        Command::new(binary_path)
            .arg("--serve")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // 独立进程组：kill_all 时 killpg 连带回收 claude CLI 等孙进程
            .process_group(0)
            .pre_exec(move || {
                let _ = libc::dup2(stdin_read, 0);
                let _ = libc::close(stdin_read);
                Ok(())
            })
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar daemon: {}", e))?
    };

    unsafe {
        libc::close(stdin_read);
    }
    let stdin: std::fs::File = unsafe { std::fs::File::from_raw_fd(stdin_write) };
    Ok((child, Box::new(stdin) as Box<dyn Write + Send>))
}

#[cfg(not(unix))]
fn spawn_daemon_windows(binary_path: &Path) -> Result<(Child, Box<dyn Write + Send>), String> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW：父进程是 GUI 子系统时阻止子进程弹出 cmd 黑框
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let child = Command::new(binary_path)
        .arg("--serve")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn sidecar daemon: {}", e))?;

    let stdin = child.stdin.take().ok_or("Failed to get stdin handle")?;
    Ok((child, Box::new(stdin) as Box<dyn Write + Send>))
}

/// daemon stdout → 帧解析/路由线程；EOF 视为 daemon 退出
fn spawn_stdout_reader(stdout: Option<std::process::ChildStdout>) {
    let Some(stdout) = stdout else {
        return;
    };
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(text) => handle_daemon_line(&text),
                Err(e) => {
                    tracing::error!("[DAEMON] stdout read error: {}", e);
                    break;
                }
            }
        }
        tracing::warn!("[DAEMON] stdout EOF, daemon exited");
        daemon_gone();
    });
}

fn spawn_stderr_reader(stderr: Option<std::process::ChildStderr>) {
    let Some(stderr) = stderr else {
        return;
    };
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(text) => tracing::warn!("[DAEMON STDERR] {}", text),
                Err(_) => break,
            }
        }
    });
}

/// 解析一行 daemon 输出：合法帧才路由，其余只记日志（宁丢勿串）
fn handle_daemon_line(text: &str) {
    let Ok(frame) = serde_json::from_str::<Value>(text) else {
        tracing::warn!("[DAEMON] non-frame stdout dropped: {}", truncate_log(text));
        return;
    };
    if frame.get("v").and_then(Value::as_i64) != Some(1) {
        tracing::warn!("[DAEMON] frame without v=1 dropped: {}", truncate_log(text));
        return;
    }

    match frame.get("type").and_then(Value::as_str) {
        Some("event") => {
            let (Some(sid), Some(tag)) = (
                frame.get("sid").and_then(Value::as_str),
                frame.get("tag").and_then(Value::as_str),
            ) else {
                tracing::warn!("[DAEMON] event frame missing sid/tag: {}", truncate_log(text));
                return;
            };
            // 还原为旧版行协议：`[TAG]{json}` 或 `[TAG]tail`
            let line = match frame.get("body") {
                Some(body) if !body.is_null() => format!("{}{}", tag, body),
                _ => format!(
                    "{}{}",
                    tag,
                    frame.get("tail").and_then(Value::as_str).unwrap_or("")
                ),
            };
            forward_event(sid, line);
        }
        Some("ok") | Some("err") => {
            let Some(id) = frame.get("id").and_then(Value::as_u64) else {
                return;
            };
            let result = match frame.get("type").and_then(Value::as_str) {
                Some("ok") => Ok(()),
                _ => {
                    let error = frame
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown daemon error")
                        .to_string();
                    Err(error)
                }
            };
            let pending = {
                let guard = match DAEMON.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::error!("[DAEMON] lock poisoned: {}", e);
                        return;
                    }
                };
                let Some(state) = guard.as_ref() else {
                    return;
                };
                state.handle.pending.lock().ok().and_then(|mut p| p.remove(&id))
            };
            if let Some(tx) = pending {
                let _ = tx.send(result);
            }
        }
        other => {
            tracing::warn!(
                "[DAEMON] unknown frame type {:?}: {}",
                other,
                truncate_log(text)
            );
        }
    }
}

/// 事件按 ssid 转发：桌面端 Tauri emit（事件名与旧版一致）+ Web 端广播通道
fn forward_event(sid: &str, line: String) {
    let guard = match DAEMON.lock() {
        Ok(g) => g,
        Err(e) => {
            tracing::error!("[DAEMON] lock poisoned: {}", e);
            return;
        }
    };
    let Some(state) = guard.as_ref() else {
        tracing::debug!("[DAEMON sid={}] event after daemon gone, dropped", sid);
        return;
    };
    let Some(entry) = state.sessions.get(sid) else {
        tracing::debug!("[DAEMON sid={}] event for unknown session, dropped", sid);
        return;
    };
    tracing::info!("[DAEMON sid={}] STDOUT {}", sid, line);
    #[cfg(feature = "desktop")]
    {
        let _ = state
            .handle
            .app
            .emit(&format!("sidecar-stdout-{}", sid), &line);
    }
    let _ = entry.events.send(SidecarEvent {
        stream: "stdout",
        line,
    });
}

/// daemon 退出：清空注册表、失败挂起应答，并向每个存活会话播报断线
/// （复用旧版 [SESSION_STOP] 行，前端据此收尾并在下次发送时自动重建）。
fn daemon_gone() {
    let mut guard = match DAEMON.lock() {
        Ok(g) => g,
        Err(e) => {
            tracing::error!("[DAEMON] kill: lock poisoned: {}", e);
            return;
        }
    };
    let Some(mut state) = guard.take() else {
        return;
    };

    for (_, tx) in state
        .handle
        .pending
        .lock()
        .ok()
        .map(|mut p| std::mem::take(&mut *p))
        .unwrap_or_default()
    {
        let _ = tx.send(Err("sidecar daemon exited".to_string()));
    }

    let msg = "[SESSION_STOP]AI 服务已断开，下次发送时自动恢复".to_string();
    for (ssid, entry) in state.sessions.drain() {
        tracing::info!("[DAEMON sid={}] broadcast disconnect notice", ssid);
        #[cfg(feature = "desktop")]
        {
            let _ = state
                .handle
                .app
                .emit(&format!("sidecar-stdout-{}", ssid), &msg);
        }
        let _ = entry.events.send(SidecarEvent {
            stream: "stdout",
            line: msg.clone(),
        });
    }

    kill_daemon_process(&mut state.handle);
}

fn kill_daemon_process(handle: &mut DaemonHandle) {
    // unix: daemon 在独立进程组，killpg 连带回收 CLI 孙进程；windows: 只 kill 自身
    #[cfg(unix)]
    unsafe {
        let _ = libc::killpg(handle.pid as libc::pid_t, libc::SIGKILL);
    }
    let _ = handle.child.kill();
    let _ = handle.child.wait();
}

fn truncate_log(text: &str) -> String {
    text.chars().take(160).collect()
}

// ── 帧发送 ──

fn daemon_send(handle: &DaemonHandle, frame: &mut Value) -> Result<(), String> {
    let mut line = frame.to_string();
    line.push('\n');
    let mut w = handle.stdin.lock().map_err(|e| e.to_string())?;
    w.write_all(line.as_bytes())
        .and_then(|_| w.flush())
        .map_err(|e| format!("write to daemon failed: {}", e))
}

/// 发送需要应答的控制帧，返回应答接收端（调用方负责超时）
fn send_with_ack(
    state: &mut DaemonState,
    frame: &mut Value,
) -> Result<oneshot::Receiver<Result<(), String>>, String> {
    let id = state.handle.next_id.fetch_add(1, Ordering::Relaxed);
    frame["v"] = json!(1);
    frame["id"] = json!(id);

    let (tx, rx) = oneshot::channel();
    state
        .handle
        .pending
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id, tx);

    if let Err(e) = daemon_send(&state.handle, frame) {
        if let Ok(mut p) = state.handle.pending.lock() {
            p.remove(&id);
        }
        return Err(e);
    }
    Ok(rx)
}

// ── 对外接口（与旧版签名一致） ──

/// 启动指定 ssid 的 AI 会话（daemon 不存在时懒启动）。
///
/// - 同一 ssid 重复 spawn：先下发 close 回收旧会话（用户「新对话」场景）
/// - ssid 为空字符串时返回错误（无终端时不允许启动）
/// - sidecar_type 决定会话引擎（"claude" / "pi"），None 默认 "claude"
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn spawn_sidecar(
    app: tauri::AppHandle,
    ssid: String,
    workspace: String,
    token: String,
    addr: String,
    sidecar_type: Option<String>,
) -> Result<u32, String> {
    spawn_sidecar_impl(&ssid, &workspace, &token, &addr, sidecar_type, Some(&app)).await
}

/// Web/HTTP 形态 spawn 入口：无 Tauri 事件桥，输出仅走 broadcast → WS 广播。
pub async fn spawn_sidecar_web(
    ssid: String,
    workspace: String,
    token: String,
    addr: String,
    sidecar_type: Option<String>,
) -> Result<u32, String> {
    spawn_sidecar_impl(&ssid, &workspace, &token, &addr, sidecar_type, None).await
}

async fn spawn_sidecar_impl(
    ssid: &str,
    workspace: &str,
    token: &str,
    addr: &str,
    sidecar_type: Option<String>,
    bridge: Option<&AppEventBridge>,
) -> Result<u32, String> {
    if ssid.is_empty() {
        return Err("ssid is required to spawn sidecar".to_string());
    }

    let engine = sidecar_type.unwrap_or_else(|| crate::sidecar_factory::TYPE_CLAUDE.to_string());

    // Windows 路径反斜杠在 sidecar（Node）中易被当作转义字符，统一成正斜杠
    let workspace = crate::ai_env::normalize_path(workspace);

    tracing::info!(
        "[DAEMON] spawn session params: ssid={}, engine={}, workspace={}, token={}..., addr={}",
        ssid,
        engine,
        workspace,
        if token.len() > 8 { &token[..8] } else { token },
        addr,
    );

    let daemon_pid = ensure_daemon(bridge)?;

    // 注册会话并下发 create 帧（锁内只做同步操作，应答等待在锁外）
    let rx = {
        let mut guard = DAEMON.lock().map_err(|e| e.to_string())?;
        let Some(state) = guard.as_mut() else {
            return Err("sidecar daemon not running".to_string());
        };

        if state.sessions.contains_key(ssid) {
            let mut f = json!({ "type": "close", "sid": ssid });
            let _ = daemon_send(&state.handle, &mut f);
        }
        state
            .sessions
            .insert(ssid.to_string(), SessionEntry {
                events: broadcast::channel(1024).0,
            });

        let mut frame = json!({
            "type": "create",
            "sid": ssid,
            "engine": engine,
            "workspace": workspace,
            "token": token,
            "addr": addr,
        });
        match send_with_ack(state, &mut frame) {
            Ok(rx) => rx,
            Err(e) => {
                state.sessions.remove(ssid);
                return Err(e);
            }
        }
    };

    let drop_session = |ssid: &str| {
        if let Ok(mut guard) = DAEMON.lock() {
            if let Some(state) = guard.as_mut() {
                state.sessions.remove(ssid);
            }
        }
    };

    match tokio::time::timeout(CREATE_ACK_TIMEOUT, rx).await {
        Ok(Ok(Ok(()))) => Ok(daemon_pid),
        Ok(Ok(Err(e))) => {
            drop_session(ssid);
            Err(e)
        }
        Ok(_) => {
            // 应答端被丢弃（daemon 退出清理等）
            drop_session(ssid);
            Err("sidecar daemon dropped create ack".to_string())
        }
        Err(_) => {
            drop_session(ssid);
            Err(
                "sidecar daemon create ack timeout（app-ai 二进制可能过旧，请更新到支持 --serve 的版本）"
                    .to_string(),
            )
        }
    }
}

/// 订阅指定 ssid 的输出事件（Web WS handler 用）。
/// 会话不存在时返回 None。
pub fn subscribe_events(ssid: &str) -> Option<broadcast::Receiver<SidecarEvent>> {
    let guard = DAEMON.lock().ok()?;
    guard
        .as_ref()?
        .sessions
        .get(ssid)
        .map(|p| p.events.subscribe())
}

/// 向指定 ssid 的会话写入数据。
/// 旧版协议语义映射：`__STOP__` → stop 帧，其余 → user 帧（数据行）。
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn write_to_sidecar(ssid: String, data: String) -> Result<(), String> {
    write_sidecar_data(&ssid, &data)
}

/// Web 版写入
#[cfg(not(feature = "desktop"))]
pub fn write_to_sidecar(ssid: String, data: String) -> Result<(), String> {
    write_sidecar_data(&ssid, &data)
}

fn write_sidecar_data(ssid: &str, data: &str) -> Result<(), String> {
    let mut guard = DAEMON.lock().map_err(|e| e.to_string())?;
    let Some(state) = guard.as_mut() else {
        return Err(format!("No sidecar process running for ssid={}", ssid));
    };
    if !state.sessions.contains_key(ssid) {
        return Err(format!("No sidecar process running for ssid={}", ssid));
    }

    let trimmed = data.trim_end();
    let mut frame = if trimmed.trim() == "__STOP__" {
        tracing::info!("[DAEMON ssid={}] stop current turn", ssid);
        json!({ "type": "stop", "sid": ssid })
    } else {
        tracing::info!("[DAEMON ssid={}] writing {} bytes", ssid, data.len());
        json!({ "type": "user", "sid": ssid, "data": trimmed })
    };
    daemon_send(&state.handle, &mut frame)
}

/// 结束指定 ssid 的会话：下发 close 帧，daemon 内部打断并回收引擎状态
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn kill_sidecar(ssid: String) -> Result<(), String> {
    kill_sidecar_process(&ssid)
}

/// Web 版终止
#[cfg(not(feature = "desktop"))]
pub fn kill_sidecar(ssid: String) -> Result<(), String> {
    kill_sidecar_process(&ssid)
}

fn kill_sidecar_process(ssid: &str) -> Result<(), String> {
    let mut guard = DAEMON.lock().map_err(|e| e.to_string())?;
    let Some(state) = guard.as_mut() else {
        tracing::info!("[DAEMON ssid={}] kill_sidecar: daemon not running", ssid);
        return Ok(());
    };
    if state.sessions.remove(ssid).is_some() {
        tracing::info!("[DAEMON ssid={}] session closed", ssid);
    } else {
        tracing::info!("[DAEMON ssid={}] kill_sidecar: no session found", ssid);
    }
    let mut f = json!({ "type": "close", "sid": ssid });
    let _ = daemon_send(&state.handle, &mut f);
    Ok(())
}

/// 获取指定 ssid 会话的 daemon 进程 PID
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn get_sidecar_pid(ssid: String) -> Option<u32> {
    get_sidecar_pid_impl(&ssid)
}

#[cfg(not(feature = "desktop"))]
pub fn get_sidecar_pid(ssid: String) -> Option<u32> {
    get_sidecar_pid_impl(&ssid)
}

fn get_sidecar_pid_impl(ssid: &str) -> Option<u32> {
    let guard = DAEMON.lock().ok()?;
    let state = guard.as_ref()?;
    state
        .sessions
        .contains_key(ssid)
        .then_some(state.handle.pid)
}

/// 判断指定 ssid 是否有 AI 会话在运行
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn has_sidecar(ssid: String) -> bool {
    has_sidecar_impl(&ssid)
}

#[cfg(not(feature = "desktop"))]
pub fn has_sidecar(ssid: String) -> bool {
    has_sidecar_impl(&ssid)
}

fn has_sidecar_impl(ssid: &str) -> bool {
    DAEMON
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|s| s.sessions.contains_key(ssid)))
        .unwrap_or(false)
}

/// 同步终止 daemon 进程（应用退出时调用）。
///
/// 不是 tauri command -- 由 lib.rs 在 RunEvent::Exit / ExitRequested 中调用，
/// 确保 daemon 及其 CLI 孙进程随主进程一起结束，避免僵尸进程。
pub fn kill_all_sidecars() {
    let mut guard = match DAEMON.lock() {
        Ok(g) => g,
        Err(e) => {
            tracing::error!("[DAEMON] kill_all_sidecars: lock poisoned: {}", e);
            return;
        }
    };

    let Some(mut state) = guard.take() else {
        return;
    };

    let session_count = state.sessions.len();
    tracing::info!(
        "[DAEMON] kill_all_sidecars: terminating daemon (pid {}), {} session(s)",
        state.handle.pid,
        session_count
    );
    state.sessions.clear();
    kill_daemon_process(&mut state.handle);
}
