use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::io::FromRawFd;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

use tauri::Emitter;

/// 单个 sidecar 进程的运行时句柄
struct SidecarProcess {
    pid: u32,
    stdin: Box<dyn Write + Send>,
    #[allow(dead_code)]
    child: Child,
}

impl SidecarProcess {
    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.stdin.write_all(data).map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }
}

/// 全局 sidecar 进程表：ssid -> 进程
///
/// 一个 SSH 终端会话对应一个 sidecar；ssid 为空字符串视为「无终端」，不允许 spawn。
static SIDECAR_MAP: Mutex<Option<HashMap<String, SidecarProcess>>> = Mutex::new(None);

fn with_map<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut HashMap<String, SidecarProcess>) -> R,
{
    let mut guard = SIDECAR_MAP.lock().map_err(|e| e.to_string())?;
    let map = guard.get_or_insert_with(HashMap::new);
    Ok(f(map))
}

/// 启动 sidecar 进程（按 ssid 索引，每个 SSH 终端一个独立进程）
///
/// 位置参数顺序：`<workspace> <ssid> <token> <addr> <engine>`
/// 第 5 个参数把引擎类型下发给统一二进制 sidecar-ai；旧版二进制会忽略它。
///
/// - 同一 ssid 重复 spawn 会先 kill 旧进程（用户主动「新对话」场景）
/// - ssid 为空字符串时返回错误（无终端时不允许启动）
/// - sidecar_type 决定旧版二进制回退名（"claude" / "pi"），None 默认 "claude"
#[tauri::command]
pub fn spawn_sidecar(
    app: tauri::AppHandle,
    ssid: String,
    workspace: String,
    token: String,
    addr: String,
    sidecar_type: Option<String>,
) -> Result<u32, String> {
    if ssid.is_empty() {
        return Err("ssid is required to spawn sidecar".to_string());
    }

    let sidecar_type =
        sidecar_type.unwrap_or_else(|| crate::sidecar_factory::TYPE_CLAUDE.to_string());

    // Windows 路径反斜杠在 sidecar（Node）中易被当作转义字符，统一成正斜杠
    let workspace = crate::ai_env::normalize_path(&workspace);

    tracing::info!(
        "[SIDECAR] spawn_sidecar params: ssid={}, sidecar_type={}, workspace={}, token={}..., addr={}",
        ssid,
        sidecar_type,
        workspace,
        if token.len() > 8 { &token[..8] } else { &token },
        addr,
    );

    // 若已有同 ssid 的进程在运行，先终止
    with_map(|map| {
        if let Some(mut p) = map.remove(&ssid) {
            tracing::info!("[SIDECAR ssid={}] Killing existing PID {}", ssid, p.pid);
            let _ = p.child.kill();
            let _ = p.child.wait();
        }
    })?;

    let binary_path = crate::sidecar_factory::find_sidecar_binary(&sidecar_type)?;

    tracing::info!(
        "[SIDECAR ssid={}] Spawning: {:?} workspace={} addr={}",
        ssid,
        binary_path,
        workspace,
        addr,
    );

    // Unix: 使用 socketpair 作为 stdin（bun 编译的二进制对 pipe 不触发事件循环）
    // Windows: 使用 Stdio::piped()
    #[cfg(unix)]
    let (mut child, stdin) = {
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
            Command::new(&binary_path)
                .arg(&workspace)
                .arg(&ssid)
                .arg(&token)
                .arg(&addr)
                .arg(&sidecar_type)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .pre_exec(move || {
                    let _ = libc::dup2(stdin_read, 0);
                    let _ = libc::close(stdin_read);
                    Ok(())
                })
                .spawn()
                .map_err(|e| format!("Failed to spawn sidecar: {}", e))?
        };

        unsafe {
            libc::close(stdin_read);
        }
        let stdin: std::fs::File = unsafe { std::fs::File::from_raw_fd(stdin_write) };
        (child, Box::new(stdin) as Box<dyn Write + Send>)
    };

    #[cfg(not(unix))]
    let (mut child, stdin) = {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000
        // Windows 上父进程是 GUI 子系统、子进程是控制台程序时，系统会为子进程新建控制台，
        // 导致弹出 cmd 黑框。该 flag 阻止创建控制台窗口，stdin/stdout/stderr 管道照常工作。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let mut child = Command::new(&binary_path)
            .arg(&workspace)
            .arg(&ssid)
            .arg(&token)
            .arg(&addr)
            .arg(&sidecar_type)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

        let stdin = child.stdin.take().ok_or("Failed to get stdin handle")?;
        (child, Box::new(stdin) as Box<dyn Write + Send>)
    };

    let stdout_event = format!("sidecar-stdout-{}", ssid);
    let stderr_event = format!("sidecar-stderr-{}", ssid);

    // stdout -> 转发到前端 via Tauri event（事件名带 ssid）
    if let Some(stdout) = child.stdout.take() {
        let app_handle = app.clone();
        let ssid_for_log = ssid.clone();
        let event_name = stdout_event.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        tracing::info!("[SIDECAR ssid={} STDOUT] {}", ssid_for_log, text);
                        let _ = app_handle.emit(&event_name, &text);
                    }
                    Err(e) => {
                        tracing::error!("[SIDECAR ssid={} STDOUT ERROR] {}", ssid_for_log, e);
                        break;
                    }
                }
            }
            tracing::info!(
                "[SIDECAR ssid={}] stdout reader thread exited",
                ssid_for_log
            );
        });
    }

    // stderr -> 转发到前端 via Tauri event（事件名带 ssid）
    if let Some(stderr) = child.stderr.take() {
        let app_handle = app.clone();
        let ssid_for_log = ssid.clone();
        let event_name = stderr_event.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        tracing::warn!("[SIDECAR ssid={} STDERR] {}", ssid_for_log, text);
                        let _ = app_handle.emit(&event_name, &text);
                    }
                    Err(e) => {
                        tracing::error!("[SIDECAR ssid={} STDERR ERROR] {}", ssid_for_log, e);
                        break;
                    }
                }
            }
            tracing::info!(
                "[SIDECAR ssid={}] stderr reader thread exited",
                ssid_for_log
            );
        });
    }

    let pid = child.id();

    with_map(|map| {
        map.insert(ssid.clone(), SidecarProcess { pid, stdin, child });
    })?;

    tracing::info!("[SIDECAR ssid={}] Started with PID: {}", ssid, pid);

    Ok(pid)
}

/// 向指定 ssid 的 sidecar 进程写入数据
#[tauri::command]
pub fn write_to_sidecar(ssid: String, data: String) -> Result<(), String> {
    with_map(|map| match map.get_mut(&ssid) {
        Some(p) => {
            tracing::info!(
                "[SIDECAR ssid={}] Writing {} bytes to PID {}",
                ssid,
                data.len(),
                p.pid
            );
            p.write(data.as_bytes())
        }
        None => Err(format!("No sidecar process running for ssid={}", ssid)),
    })?
}

/// 终止指定 ssid 的 sidecar 进程
#[tauri::command]
pub fn kill_sidecar(ssid: String) -> Result<(), String> {
    with_map(|map| {
        if let Some(mut p) = map.remove(&ssid) {
            tracing::info!("[SIDECAR ssid={}] Killing PID {}", ssid, p.pid);
            let _ = p.stdin.flush();
            let _ = p.child.kill();
            let _ = p.child.wait();
            tracing::info!("[SIDECAR ssid={}] Process terminated", ssid);
        } else {
            tracing::info!("[SIDECAR ssid={}] kill_sidecar: no process found", ssid);
        }
    })
}

/// 获取指定 ssid 的 sidecar 进程 PID
#[tauri::command]
pub fn get_sidecar_pid(ssid: String) -> Option<u32> {
    SIDECAR_MAP
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref()?.get(&ssid).map(|p| p.pid))
}

/// 判断指定 ssid 是否有 sidecar 在运行
#[tauri::command]
pub fn has_sidecar(ssid: String) -> bool {
    SIDECAR_MAP
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|map| map.contains_key(&ssid)))
        .unwrap_or(false)
}

/// 同步终止所有 sidecar 进程（应用退出时调用）。
///
/// 不是 tauri command -- 由 lib.rs 在 RunEvent::Exit / ExitRequested 中调用，
/// 确保所有子进程随主进程一起结束，避免僵尸进程。
pub fn kill_all_sidecars() {
    let mut guard = match SIDECAR_MAP.lock() {
        Ok(g) => g,
        Err(e) => {
            tracing::error!("[SIDECAR] kill_all_sidecars: lock poisoned: {}", e);
            return;
        }
    };

    let Some(map) = guard.as_mut() else {
        return;
    };

    let count = map.len();
    if count == 0 {
        return;
    }

    tracing::info!("[SIDECAR] kill_all_sidecars: terminating {} process(es)", count);
    for (ssid, mut p) in map.drain() {
        tracing::info!("[SIDECAR ssid={}] kill_all: PID {}", ssid, p.pid);
        let _ = p.stdin.flush();
        let _ = p.child.kill();
        let _ = p.child.wait();
    }
}
