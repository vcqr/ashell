//! 无 ConPTY 老系统（Win10 1809 前 / Server 2016 / Wine）的本地终端 winpty 回退会话。
//!
//! 协议与 local_pty 完全一致（同一段前端 TerminalView 消费）；差异：
//! - PTY 来自 winpty-rs 的 WinPTY 后端（winpty 0.4.3 二进制随包分发，见
//!   resources/winpty/README.md），不走 portable-pty——其 load_conpty 对缺
//!   符号直接 panic，优雅降级分支走不到；
//! - winpty 的输出管道 EOF 是可靠的（shell 退出后 agent 关闭管道），没有
//!   ConPTY「控制台被子进程整体拆除但进程还活着」的 torndown 怪癖，因此
//!   不做重启代：EOF / 进程退出即会话结束，语义等同 local_pty 的 Unix 分支；
//! - win32-input-mode 键事件编码、OSC 9;9 cwd 上报、行尾规一化等输入路径
//!   直接复用 local_pty 的 helpers（winpty 同样把字节流交给 conhost 解析，
//!   这些序列的处理方式一致）。
//!
//! 仅 x86_64 Windows 编译：winpty 上游无 arm64 二进制，ARM64 Windows 也不
//! 存在无 ConPTY 的现实用户群（WoA 设备全系 1809+，见 resources/winpty/README.md）。
#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::CommandBuilder;
use tokio::sync::{broadcast, mpsc};
use winptyrs::{PTY, PTYArgs, PTYBackend};

use crate::service::local_pty::{
    apply_cwd_reporting, inherit_env, resolve_command, resolve_session_cwd, scan_cwd_osc,
    which_in_path, write_input, write_text_input, ClientMsg, ReadyMsg,
};
use crate::service::ssh as ssh_svc;
use crate::util;

/// reader 轮询间隔：read(false) 无数据时的空转步长（pywinpty 同款模式），
/// 交互手感与 CPU 占用之间的折衷
const READ_POLL: Duration = Duration::from_millis(30);
/// 退出监听轮询间隔：is_alive 的检查步长
const EXIT_POLL: Duration = Duration::from_millis(300);

pub(crate) async fn handle_winpty(
    socket: WebSocket,
    sid: String,
    shell: Option<String>,
    shell_command: Option<String>,
    initial_cwd: Option<String>,
) {
    log::info!(
        "local_pty_winpty: connected sid={sid} shell={shell:?} shell_command={shell_command:?}"
    );

    let (mut ws_tx, mut ws_rx) = socket.split();

    // 命令注入 / 输出广播通道（与 ConPTY / SSH 路径一致，供 AI sidecar
    // 通过 POST /api/ssh/send/{sid} 注入命令并收集输出）
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    let (output_tx, _) = broadcast::channel::<String>(64);
    ssh_svc::set_terminal_channels(&sid, cmd_tx, output_tx.clone()).await;

    // 跨会话状态：shell 经 OSC 9;9 上报的最新 cwd（本模块无重启代，仅用于
    // 与 ConPTY 路径共享 resolve_session_cwd 的初始目录优先级逻辑）
    let last_cwd: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // 1) 开 PTY。显式指定 WinPTY 后端：Auto 会先试 ConPTY，老系统上无意义。
    //    PTYArgs::default() 即 mouse_mode=NONE / timeout=10s / COLOR_ESCAPES，
    //    只覆盖初始尺寸（与 ConPTY 路径的默认 100x24 一致）。
    let mut args = PTYArgs::default();
    args.cols = 100;
    args.rows = 24;
    let mut pty = match PTY::new_with_backend(&args, PTYBackend::WinPTY) {
        Ok(p) => p,
        Err(e) => {
            log::error!("local_pty_winpty PTY new: {e:?}");
            let _ = ws_tx
                .send(Message::Text(
                    format!("\x1b[31m[ashell] winpty create failed: {e:?}\x1b[0m").into(),
                ))
                .await;
            ssh_svc::remove_terminal_channels(&sid).await;
            let _ = ws_tx.close().await;
            return;
        }
    };

    // 2) 拼命令：与 ConPTY 路径共用 resolve_command / inherit_env /
    //    apply_cwd_reporting（PS -EncodedCommand 注入、cmd PROMPT、git-bash
    //    --rcfile、cwd 优先级等逻辑一字不差），再转写为 winpty 的 spawn 四参数
    let (mut cmd, shell_kind) = resolve_command(shell.as_deref(), shell_command.as_deref());
    inherit_env(&mut cmd);
    apply_cwd_reporting(&mut cmd, shell_kind);
    if let Some(cwd) = resolve_session_cwd(&last_cwd, initial_cwd.as_deref()) {
        cmd.cwd(cwd);
    }
    let (appname, cmdline, cwd, env_block) = transcribe_command(&cmd);

    // spawn 参数进日志（连接是低频事件）：appname 为空串表示走 cmdline 首词的
    // CreateProcess 常规搜索；真机排查 "CreateProcess failed os error 2" 类
    // 失败时，这行能看到确切传给 winpty 的程序与命令行
    log::info!(
        "local_pty_winpty: spawn app={:?} cmdline={:?} cwd={:?} sid={sid}",
        appname.to_string_lossy(),
        cmdline.to_string_lossy(),
        cwd.as_ref().map(|c| c.to_string_lossy()),
    );

    let program_display = appname.to_string_lossy().into_owned();
    if let Err(e) = pty.spawn(appname, Some(cmdline), cwd, env_block) {
        // program_display 帮助终端里直接看出"找不到的到底是谁"：整条命令
        // 覆盖（自定义启动项）若含空格路径未加引号，首段会被 split_command_line
        // 切碎（如 C:\Program），此处一目了然
        log::error!("local_pty_winpty spawn: {e:?}");
        let _ = ws_tx
            .send(Message::Text(
                format!(
                    "\x1b[31m[ashell] spawn shell failed (app: {program_display}): {e:?}\x1b[0m"
                )
                .into(),
            ))
            .await;
        ssh_svc::remove_terminal_channels(&sid).await;
        let _ = ws_tx.close().await;
        return;
    }
    let pty = Arc::new(pty);
    log::info!("local_pty_winpty spawned pid={} sid={sid}", pty.get_pid());

    // 3) 通告 sid（协议首帧）
    let ready = serde_json::to_string(&ReadyMsg {
        kind: "ready",
        sid: &sid,
    })
    .unwrap_or_else(|_| "{\"kind\":\"ready\",\"sid\":\"\"}".to_string());
    let _ = ws_tx.send(Message::Text(ready.into())).await;

    // 4) reader / 退出监听 / writer
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    let (exit_tx, mut exit_rx) = mpsc::channel::<()>(1);

    // 停止标志：teardown 用它让两个阻塞线程确定性退出（不依赖 winpty-rs
    // 的 cancel_io 语义）；两个线程各自持有 Arc<PTY>，退出后引用计数归零，
    // PTY 的 Drop 会终止残留的 shell / agent / conhost 进程（1.0.8 修复）。
    let stop = Arc::new(AtomicBool::new(false));

    let cwd_sink = last_cwd.clone();
    let read_pty = pty.clone();
    let read_stop = stop.clone();
    let read_exit_tx = exit_tx.clone();
    let sid_read = sid.clone();
    let read_task = tokio::task::spawn_blocking(move || {
        let mut osc_carry: Vec<u8> = Vec::new();
        loop {
            if read_stop.load(Ordering::Relaxed) {
                break;
            }
            match read_pty.read(false) {
                Ok(s) => {
                    // winpty-rs 的读路径本身是有损的：管道字节 → CP_UTF8 解码 →
                    // UTF-16 → 剥离 NUL → OsString（见其 pty/base.rs 的 read 实现），
                    // 拿不到原始字节。to_string_lossy 保证转发给前端的恒为合法
                    // UTF-8（合法 UTF-8 输出原样往返；非法字节在 crate 内部已
                    // 变成 U+FFFD）。ConPTY 路径是字节级透传，此处为该后端的固有限制。
                    let bytes = s.to_string_lossy().into_owned().into_bytes();
                    if bytes.is_empty() {
                        std::thread::sleep(READ_POLL);
                        continue;
                    }
                    osc_carry.extend_from_slice(&bytes);
                    scan_cwd_osc(&mut osc_carry, &cwd_sink);
                    if out_tx.blocking_send(bytes).is_err() {
                        // 主循环已退出、通道被 drop，正常收尾
                        log::info!("local_pty_winpty: 输出通道已关闭，读线程退出 sid={sid_read}");
                        break;
                    }
                }
                Err(e) => {
                    // winpty-rs 1.0.8：流结束以 Err 报告（shell 退出 / agent 管道关闭）。
                    // 也发 exit 信号兜底：若进程退出监听因故卡住，主循环仍能结束。
                    log::info!("local_pty_winpty: 读流结束（shell 退出/管道关闭）: {e:?} sid={sid_read}");
                    let _ = read_exit_tx.blocking_send(());
                    break;
                }
            }
        }
    });

    let alive_pty = pty.clone();
    let alive_stop = stop.clone();
    let sid_alive = sid.clone();
    let exit_task = tokio::task::spawn_blocking(move || {
        loop {
            if alive_stop.load(Ordering::Relaxed) {
                return;
            }
            match alive_pty.is_alive() {
                Ok(true) => std::thread::sleep(EXIT_POLL),
                Ok(false) => {
                    match alive_pty.get_exitstatus() {
                        Ok(Some(code)) => {
                            log::info!("local_pty_winpty: shell 退出 code={code} sid={sid_alive}")
                        }
                        Ok(None) => {
                            log::info!("local_pty_winpty: shell 进程已退出 sid={sid_alive}")
                        }
                        Err(e) => log::warn!(
                            "local_pty_winpty: get_exitstatus 失败 sid={sid_alive}: {e:?}"
                        ),
                    }
                    break;
                }
                Err(e) => {
                    log::warn!("local_pty_winpty: is_alive 失败 sid={sid_alive}: {e:?}");
                    break;
                }
            }
        }
        let _ = exit_tx.blocking_send(());
    });

    // writer：桥接现有 write_input / write_text_input（&mut dyn Write）。
    // winpty 的 write 可能部分写入，循环补齐剩余字节。
    struct PtyWriter(Arc<PTY>);
    impl std::io::Write for PtyWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let total = buf.len();
            let mut sent = 0usize;
            // 输入字节 → OsString：合法 UTF-8 无损直达（写路径 encode_wide →
            // WideCharToMultiByte(CP_UTF8) 原样还原）；非法字节 lossy 兜底
            // （键盘/粘贴输入实际恒为合法 UTF-8，此分支仅防病态注入）。
            let chunk: OsString = match std::str::from_utf8(&buf[sent..]) {
                Ok(s) => OsString::from(s),
                Err(_) => OsString::from(String::from_utf8_lossy(&buf[sent..]).into_owned()),
            };
            while sent < total {
                let n = self
                    .0
                    .write(chunk.clone())
                    .map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::Other, format!("winpty write: {e:?}"))
                    })?;
                if n == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "winpty write 返回 0",
                    ));
                }
                sent += n as usize;
            }
            Ok(total)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = PtyWriter(pty.clone());

    // 主事件循环：结构与 local_pty::handle 一致，但无 torndown/重启代——
    // EOF 即 shell 退出，直接结束会话
    let mut utf8_tail: Vec<u8> = Vec::new();
    loop {
        tokio::select! {
            Some(()) = exit_rx.recv() => {
                log::info!("local_pty_winpty: [break] shell 子进程退出信号 sid={sid}");
                break;
            }
            chunk = out_rx.recv() => {
                match chunk {
                    Some(chunk) => {
                        // 有订阅者（AI 收集副本）才做 lossy 转换，零订阅者不白白分配
                        if output_tx.receiver_count() > 0 {
                            let _ = output_tx.send(util::utf8_safe(&mut utf8_tail, &chunk));
                        }
                        // 输出走 Binary 帧（保留任意字节），与 ConPTY/SSH 一致
                        if ws_tx.send(Message::Binary(chunk.into())).await.is_err() {
                            log::info!("local_pty_winpty: [break] ws 发送输出失败（前端已断开） sid={sid}");
                            break;
                        }
                    }
                    None => {
                        log::info!("local_pty_winpty: [break] PTY EOF（shell 退出） sid={sid}");
                        break;
                    }
                }
            }
            Some(data) = cmd_rx.recv() => {
                if write_text_input(&mut writer, &data, "external cmd") {
                    log::info!("local_pty_winpty: [break] 外部命令注入写入失败 sid={sid}");
                    break;
                }
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else {
                    log::info!("local_pty_winpty: [break] ws 流结束（前端断开，无 Close 帧） sid={sid}");
                    break;
                };
                match msg {
                    Ok(Message::Text(text)) => {
                        let s = text.as_str();
                        if s.starts_with('{') && s.ends_with('}') {
                            match serde_json::from_str::<ClientMsg>(s) {
                                Ok(ClientMsg::Cmd { data }) => {
                                    if write_text_input(&mut writer, &data, "ws cmd") {
                                        log::info!("local_pty_winpty: [break] ws cmd 写入失败 sid={sid}");
                                        break;
                                    }
                                }
                                Ok(ClientMsg::Resize { cols, rows }) => {
                                    log::info!(
                                        "local_pty_winpty: resize sid={sid} cols={cols} rows={rows}"
                                    );
                                    if let Err(e) = pty.set_size(cols as i32, rows as i32) {
                                        log::warn!("local_pty_winpty: resize failed sid={sid}: {e:?}");
                                    }
                                }
                                Ok(ClientMsg::Ping) => {
                                    let _ = ws_tx.send(Message::Text(
                                        "{\"kind\":\"pong\"}".to_string().into(),
                                    )).await;
                                }
                                Ok(ClientMsg::SudoFill) => {
                                    // 本地终端无 sudo 密码可用，忽略
                                }
                                Err(_) => {
                                    // 不是协议帧 → 当作原始输入透传
                                    if write_text_input(&mut writer, s, "ws raw text") {
                                        log::info!("local_pty_winpty: [break] ws raw text 写入失败 sid={sid}");
                                        break;
                                    }
                                }
                            }
                        } else {
                            if write_text_input(&mut writer, s, "ws text") {
                                log::info!("local_pty_winpty: [break] ws text 写入失败 sid={sid}");
                                break;
                            }
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        if write_input(&mut writer, &bytes, "ws binary") {
                            log::info!("local_pty_winpty: [break] ws binary 写入失败 sid={sid}");
                            break;
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        log::info!("local_pty_winpty: [break] 收到前端 Close 帧 sid={sid} frame={frame:?}");
                        break;
                    }
                    Err(e) => {
                        log::warn!("local_pty_winpty: [break] ws 接收错误 sid={sid}: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // 收尾：置停止标志让阻塞线程退出 → 引用计数归零 → PTY Drop 终止残留进程。
    // 与 ConPTY 路径不同，这里没有重启代，EOF/退出即会话结束。
    stop.store(true, Ordering::Relaxed);
    read_task.abort();
    exit_task.abort();
    let tail_text = util::utf8_flush(&mut utf8_tail);
    if !tail_text.is_empty() {
        let _ = output_tx.send(tail_text);
    }
    log::info!("local_pty_winpty cleanup sid={sid}");
    ssh_svc::remove_terminal_channels(&sid).await;
    let _ = ws_tx.close().await;
}

/// 裸程序名解析为绝对路径：CreateProcess 的 lpApplicationName 非 NULL 时不做
/// PATH 搜索（winpty 的 spawn_config 把 appname 原样传给它），裸名必然
/// "os error 2"。解析链：PATH → %SystemRoot%\System32 →
/// System32\WindowsPowerShell\v1.0（旧系统/精简环境的 PATH 常缺后两者）。
/// 带路径分隔符或本就存在的名字原样返回；全部失败返回 None。
fn resolve_program(program: &OsStr) -> Option<OsString> {
    let lossy = program.to_string_lossy();
    if lossy.contains('\\') || lossy.contains('/') || Path::new(program).is_file() {
        return Some(program.to_os_string());
    }
    if let Some(p) = which_in_path(&lossy) {
        return Some(p.into_os_string());
    }
    let system_root = std::env::var_os("SystemRoot")?;
    let system_root = std::path::PathBuf::from(system_root);
    [
        system_root.join("System32").join(program),
        system_root
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join(program),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .map(|p| p.into_os_string())
}

/// CommandBuilder → winpty spawn 四参数。
/// - appname：argv[0] 尽量解析为绝对路径（见 `resolve_program`）；解析彻底
///   失败时置为空串——winpty 对空 appname 按 NULL 处理，CreateProcess 退化为
///   按 cmdline 首词做常规搜索（应用目录/cwd/System32/PATH），是裸名唯一可能
///   成功的形态（非空 lpApplicationName 不搜索，必报 os error 2）。
/// - cmdline：argv 引号拼接（含空白/引号的段落加双引号），首段为程序本身。
/// - env：`KEY=VALUE\0` 逐对 UTF-16 块，按名排序（Windows 惯例）；
///   winpty-rs 会补最后一个 \0 形成双 \0 结尾的标准环境块。
fn transcribe_command(
    cmd: &CommandBuilder,
) -> (OsString, OsString, Option<OsString>, Option<OsString>) {
    let argv = cmd.get_argv();
    let raw_program = argv.first().cloned().unwrap_or_default();
    let program: OsString = resolve_program(&raw_program).unwrap_or_default();

    let mut parts: Vec<OsString> = Vec::with_capacity(argv.len());
    parts.push(program.clone());
    parts.extend(argv.iter().skip(1).cloned());
    let cmdline = quote_join(&parts);

    let cwd = cmd.get_cwd().cloned();

    let mut pairs: Vec<(String, String)> = cmd
        .iter_full_env_as_str()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pairs.sort_by(|a, b| a.0.to_uppercase().cmp(&b.0.to_uppercase()));
    let mut wide: Vec<u16> = Vec::new();
    for (k, v) in pairs {
        wide.extend(k.encode_utf16());
        wide.push('=' as u16);
        wide.extend(v.encode_utf16());
        wide.push(0);
    }
    let env_block = OsString::from_wide(&wide);

    (program, cmdline, cwd, Some(env_block))
}

/// Windows 命令行引号拼接：含空白或引号的段落加双引号（内嵌引号转义）；
/// 与 local_pty::split_command_line 的宽松约定互为逆操作。
fn quote_join(parts: &[OsString]) -> OsString {
    let mut out = OsString::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(" ");
        }
        let s = p.to_string_lossy();
        if s.contains(' ') || s.contains('\t') || s.contains('"') {
            out.push(format!("\"{}\"", s.replace('"', "\\\"")));
        } else {
            out.push(s.into_owned());
        }
    }
    out
}
