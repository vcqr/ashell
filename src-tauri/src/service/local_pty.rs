//! 本地 PTY 终端：复用前端 TerminalView 的 JSON/二进制协议，实现"不连服务器、直接开本机 shell"。
//!
//! 协议（与 handlers::terminal 完全一致）：
//! - 上行 Text JSON：`{kind:"cmd", data}` / `{kind:"resize", cols, rows}` / `{kind:"ping"}`
//! - 下行 首帧 Text JSON：`{kind:"ready", sid}`，之后 Binary 帧为 PTY 输出。
//!
//! 关键平台细节：
//! - Windows ConPTY 启动时会发 `ESC[6n`（DSR-CPR）查询光标位置，xterm.js 会自动应答；
//!   但为了让裸 WebSocket 客户端也能跑通，这里统一在读线程拦截并合成 `ESC[1;1R`。
//! - `CommandBuilder` 默认不继承父进程环境，必须显式 `cmd.env(k, v)`，否则 PowerShell / bash
//!   常常因为缺少 PATH / SystemRoot 等关键变量直接退出。
//! - macOS/Linux 上必须以登录交互 shell（`-l -i`）启动：GUI app 从 Finder/Dock 拉起时
//!   父进程环境残缺（PATH 缺 homebrew / nvm / cargo bin），只有让 zsh/bash 跑一遍
//!   ~/.zprofile + ~/.zshrc 才能重建出和 iTerm2 / Terminal.app 一致的环境。同时不把
//!   残缺的 PATH 显式 setenv 给子进程，避免覆盖 shell 启动脚本重建的 PATH。
//! - 用户传 `\r\n` 或 `\n` 时统一规一化为 `\r`，匹配 ConPTY / 大多数 \*nix shell 的行尾期望。

use base64::Engine as _;
use std::io::{Read, Write};
use std::sync::OnceLock;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::service::ssh as ssh_svc;

/// 识别出的 shell 类别，决定 cwd 上报（OSC 9;9）的注入方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellKind {
    PowerShell,
    Cmd,
    Bash,
    Zsh,
    Other,
}

fn kind_of(program: &str) -> ShellKind {
    let name = std::path::Path::new(program)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match name.as_str() {
        "powershell" | "pwsh" => ShellKind::PowerShell,
        "cmd" => ShellKind::Cmd,
        "bash" => ShellKind::Bash,
        "zsh" => ShellKind::Zsh,
        _ => ShellKind::Other,
    }
}

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

/// `shell` 形如 `"powershell"` / `"pwsh"` / `"cmd"` / `"bash"` / `"zsh"` / 绝对路径。
/// 传 `None` 走平台默认。返回命令与识别出的 shell 类别（用于 cwd 上报注入）。
fn resolve_command(shell: Option<&str>) -> (CommandBuilder, ShellKind) {
    let pick = shell.map(str::trim).filter(|s| !s.is_empty());

    if cfg!(windows) {
        match pick.unwrap_or("auto") {
            "auto" => {
                // Windows 默认偏好 PowerShell；不存在再退回 cmd。
                if which_in_path("powershell.exe").is_some() {
                    let mut c = CommandBuilder::new("powershell.exe");
                    c.arg("-NoLogo");
                    (c, ShellKind::PowerShell)
                } else {
                    (CommandBuilder::new("cmd.exe"), ShellKind::Cmd)
                }
            }
            "powershell" => {
                let mut c = CommandBuilder::new("powershell.exe");
                c.arg("-NoLogo");
                (c, ShellKind::PowerShell)
            }
            "pwsh" => {
                let mut c = CommandBuilder::new("pwsh.exe");
                c.arg("-NoLogo");
                (c, ShellKind::PowerShell)
            }
            "cmd" => (CommandBuilder::new("cmd.exe"), ShellKind::Cmd),
            "git-bash" | "bash" => {
                // Git Bash 常见路径。注意：这里不加 -i，由 apply_cwd_reporting
                // 统一追加（--rcfile 必须排在 -i 之前，见其注释）
                for p in [
                    "C:\\Program Files\\Git\\bin\\bash.exe",
                    "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
                ] {
                    if std::path::Path::new(p).exists() {
                        return (CommandBuilder::new(p), ShellKind::Bash);
                    }
                }
                (CommandBuilder::new("bash.exe"), ShellKind::Bash)
            }
            other => {
                let kind = kind_of(other);
                (CommandBuilder::new(other), kind)
            }
        }
    } else {
        // macOS/Linux：必须以登录交互 shell 启动（`-l -i`），否则 ~/.zprofile / ~/.zshrc
        // / ~/.bash_profile 不会被执行，PATH 里用户自定义的部分（homebrew、nvm、pyenv、
        // cargo bin 等）全丢。GUI app（Tauri）从 Finder/Dock 拉起时父进程环境本身就残缺，
        // 只有让 shell 跑一遍启动脚本才能重建出和 iTerm2 / Terminal.app 一致的环境。
        let make_login_interactive = |program: &str| -> CommandBuilder {
            let mut c = CommandBuilder::new(program);
            c.arg("-l");
            c.arg("-i");
            c
        };
        // 依候选顺序返回第一个真实存在的 shell；全部缺失时退回 /bin/sh
        // （alpine 等 minimal 环境只有 busybox ash，避免硬编码路径导致 spawn 失败）
        let first_existing = |paths: &[&str]| -> String {
            paths
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|p| p.to_string())
                .unwrap_or_else(|| "/bin/sh".into())
        };

        match pick.unwrap_or("auto") {
            "auto" => {
                // SHELL 指向的 shell 不存在时（容器/无头环境常见）走候选链
                let shell_env = std::env::var("SHELL")
                    .ok()
                    .filter(|s| std::path::Path::new(s).exists());
                let s = shell_env.unwrap_or_else(|| {
                    first_existing(&["/bin/bash", "/bin/zsh", "/usr/bin/fish", "/bin/sh"])
                });
                let kind = kind_of(&s);
                (make_login_interactive(&s), kind)
            }
            "bash" => {
                let s = first_existing(&["/bin/bash", "/usr/bin/bash"]);
                (make_login_interactive(&s), ShellKind::Bash)
            }
            "zsh" => {
                let s = first_existing(&["/bin/zsh", "/usr/bin/zsh"]);
                (make_login_interactive(&s), ShellKind::Zsh)
            }
            "sh" => {
                // sh 没有 -l/-i 的稳定语义，直接起裸 sh
                (CommandBuilder::new("/bin/sh"), ShellKind::Other)
            }
            "fish" => {
                let s = first_existing(&["/usr/bin/fish", "/bin/fish"]);
                (make_login_interactive(&s), ShellKind::Other)
            }
            other => {
                if std::path::Path::new(other).exists() {
                    let kind = kind_of(other);
                    (make_login_interactive(other), kind)
                } else {
                    log::warn!("shell {other} 不存在，回退系统默认候选");
                    let s = first_existing(&["/bin/bash", "/bin/zsh", "/bin/sh"]);
                    let kind = kind_of(&s);
                    (make_login_interactive(&s), kind)
                }
            }
        }
    }
}

/// 极简 PATH 查找：用于 Windows auto 路径下判断 powershell.exe 是否存在。
fn which_in_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/* ---------- cwd 上报注入（OSC 9;9，ConEmu 风格） ----------
 *
 * 本地终端要驱动右侧"本地文件"抽屉做目录跟随，shell 需要在每次出提示符时
 * 把当前目录报给终端模拟器。选 OSC 9;9（纯转义序列，不改变可见提示符；
 * Windows Terminal / VS Code 同款机制，ConPTY 会原样透传给读取端）。
 * PowerShell 的 Set-Location 不同步进程 cwd，轮询进程行不通，只能走提示符注入。
 * 各 shell 的注入载体：
 * - PowerShell：-EncodedCommand 包一层 prompt 函数（profile 加载后执行，
 *   包住用户已有 prompt；-NoExit 保证注入后仍进入交互模式）
 * - cmd：PROMPT 环境变量（每次显示提示符都会重新展开，$P 即当前目录）
 * - bash：PROMPT_COMMAND 环境变量（rc 文件若重新赋值则失效，优雅降级）
 * - zsh：无环境变量钩子，用 ZDOTDIR 指向垫片目录（逐文件 source 原配置后
 *   挂 precmd 钩子；用户已自定义 ZDOTDIR 时不注入，避免破坏其配置）
 * 注入失败/被覆盖的后果只是"目录不跟随"，文件浏览器本身不受影响。
 */

const PS_CWD_SNIPPET: &str = r#"$global:__ashellOrigPrompt = $function:prompt
if ($null -eq $global:__ashellOrigPrompt) { $global:__ashellOrigPrompt = { "PS " } }
function global:prompt {
  $d = $executionContext.SessionState.Path.CurrentLocation.ProviderPath
  $o = & $global:__ashellOrigPrompt
  "$([char]27)]9;9;$d$([char]7)$o"
}
"#;

/// PowerShell 要求 -EncodedCommand 为 UTF-16LE 的 base64。
fn ps_encoded_command() -> String {
    let bytes: Vec<u8> = PS_CWD_SNIPPET
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// zsh 垫片目录：内容为静态脚本，进程生命周期内只需生成一次。
fn zsh_shim_dir() -> &'static Option<std::path::PathBuf> {
    static SHIM: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    SHIM.get_or_init(|| {
        let dir = std::env::temp_dir().join("ashell-zsh-shim");
        if std::fs::create_dir_all(&dir).is_err() {
            return None;
        }
        // 登录 shell 会依次读 ZDOTDIR 下的 .zshenv/.zprofile/.zshrc/.zlogin；
        // 垫片逐个 source 家目录的原文件，保证用户启动脚本链不缺
        let source_home = |name: &str| {
            format!(
                "[[ -f \"$HOME/{name}\" ]] && source \"$HOME/{name}\"\n"
            )
        };
        let files: [(&str, String); 4] = [
            (".zshenv", source_home(".zshenv")),
            (".zprofile", source_home(".zprofile")),
            (
                ".zshrc",
                format!(
                    "{}ashell_report_cwd() {{ printf '\\033]9;9;%s\\007' \"$PWD\" }}\nprecmd_functions+=(ashell_report_cwd)\n",
                    source_home(".zshrc")
                ),
            ),
            (".zlogin", source_home(".zlogin")),
        ];
        for (name, body) in files {
            if std::fs::write(dir.join(name), body).is_err() {
                return None;
            }
        }
        Some(dir)
    })
}

/// Windows git-bash 的 rcfile：等价复刻非登录交互 shell 的启动脚本链
/// （/etc/bash.bashrc + ~/.bashrc），再挂上报钩子。
/// 不走 PROMPT_COMMAND 环境变量：git-bash 的 /etc/profile.d/git-prompt.sh
/// 会覆盖它；且 $PWD 是 MSYS 风格（/c/...），须用 cygpath 转成 Windows 路径。
fn bash_rcfile() -> &'static Option<std::path::PathBuf> {
    static RC: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    RC.get_or_init(|| {
        let dir = std::env::temp_dir().join("ashell-bash-rc");
        if std::fs::create_dir_all(&dir).is_err() {
            return None;
        }
        let rc = dir.join("ashell-rcfile");
        let body = concat!(
            "[[ -f /etc/bash.bashrc ]] && . /etc/bash.bashrc\n",
            "[[ -f ~/.bashrc ]] && . ~/.bashrc\n",
            "__ashell_report_cwd() { printf '\\033]9;9;%s\\007' \"$(cygpath -w \"$PWD\" 2>/dev/null || printf '%s' \"$PWD\")\"; }\n",
            // 前插我们的钩子并保留 rc 链里已设置的 PROMPT_COMMAND（含数组形式）
            "PROMPT_COMMAND=\"__ashell_report_cwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}\"\n",
        );
        if std::fs::write(&rc, body).is_err() {
            return None;
        }
        Some(rc)
    })
}

fn apply_cwd_reporting(cmd: &mut CommandBuilder, kind: ShellKind) {
    match kind {
        ShellKind::PowerShell => {
            if cfg!(windows) {
                cmd.arg("-NoExit");
                cmd.arg("-EncodedCommand");
                cmd.arg(ps_encoded_command());
            }
        }
        ShellKind::Cmd => {
            if cfg!(windows) {
                // $E]9;9;$P$E\ = ESC ] 9;9;<当前目录> ESC \（ST 结尾）；
                // 尾部 $P$G 保持 cmd 默认可见提示符不变
                cmd.env("PROMPT", "$E]9;9;$P$E\\$P$G");
            }
        }
        ShellKind::Bash => {
            if cfg!(windows) {
                // --rcfile 只对非登录交互 shell 生效，git-bash（bash.exe -i）正是该形态；
                // MSYS bash 接受正斜杠的 Windows 路径。
                // 参数顺序必须是 --rcfile 在前、-i 在后：cygwin bash 5.3（Git for
                // Windows 当前版本）在 `-i --rcfile X` 的顺序下解析失败，直接
                // 打印用法退出（终端闪退）。
                if let Some(rc) = bash_rcfile() {
                    cmd.arg("--rcfile");
                    cmd.arg(rc.to_string_lossy().replace('\\', "/"));
                }
                cmd.arg("-i");
            } else {
                cmd.env("PROMPT_COMMAND", r#"printf '\033]9;9;%s\007' "$PWD""#);
            }
        }
        ShellKind::Zsh => {
            if std::env::var_os("ZDOTDIR").is_none() {
                if let Some(dir) = zsh_shim_dir() {
                    cmd.env("ZDOTDIR", dir);
                }
            }
        }
        ShellKind::Other => {}
    }
}

fn inherit_env(cmd: &mut CommandBuilder) {
    for (k, v) in std::env::vars() {
        // macOS/Linux 下不传 PATH：GUI app（从 Finder/Dock 拉起）的 PATH 通常只有
        // /usr/bin:/bin:/usr/sbin:/sbin，缺 homebrew / nvm / cargo bin。把残缺 PATH
        // 显式 setenv 给子进程，会覆盖登录 shell 从 ~/.zprofile / ~/.zshrc 里重建的
        // PATH（zsh 里 `export PATH=...` 实际是 append/重写，但 setenv 在 execve 时
        // 已经定值，shell 启动脚本能否覆盖取决于写法）。最稳是让登录 shell 自己重建。
        if cfg!(not(windows)) && k == "PATH" {
            continue;
        }
        cmd.env(k, v);
    }
    // 让 \*nix 端 ncurses/colors 行为合理
    if cfg!(not(windows)) {
        cmd.env("TERM", "xterm-256color");
    }
}

pub async fn handle(socket: WebSocket, sid: String, shell: Option<String>) {
    log::info!("local_pty: connected sid={sid}");

    let (mut ws_tx, mut ws_rx) = socket.split();

    // 1) 开 PTY
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            log::error!("local_pty openpty: {e}");
            let _ = ws_tx
                .send(Message::Text(
                    format!("\x1b[31m[ashell] openpty failed: {e}\x1b[0m").into(),
                ))
                .await;
            return;
        }
    };

    // 2) 拼命令
    let (mut cmd, shell_kind) = resolve_command(shell.as_deref());
    inherit_env(&mut cmd);
    apply_cwd_reporting(&mut cmd, shell_kind);
    if let Ok(home) = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        cmd.cwd(home);
    }

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            log::error!("local_pty spawn: {e}");
            let _ = ws_tx
                .send(Message::Text(
                    format!("\x1b[31m[ashell] spawn shell failed: {e}\x1b[0m").into(),
                ))
                .await;
            return;
        }
    };
    log::info!("local_pty spawned pid={:?}", child.process_id());

    // 释放 slave，让 child 独占（Unix 下 master 才能在 child 退出时收到 EOF）
    drop(pair.slave);

    let master = pair.master;

    // 3) 通告 sid
    let ready = serde_json::to_string(&ReadyMsg {
        kind: "ready",
        sid: &sid,
    })
    .unwrap_or_else(|_| "{\"kind\":\"ready\",\"sid\":\"\"}".to_string());
    let _ = ws_tx.send(Message::Text(ready.into())).await;

    // 4) reader / writer
    let mut reader = match master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            log::error!("local_pty try_clone_reader: {e}");
            let _ = child.kill();
            return;
        }
    };
    let mut writer = match master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            log::error!("local_pty take_writer: {e}");
            let _ = child.kill();
            return;
        }
    };

    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    let (dsr_tx, mut dsr_rx) = mpsc::channel::<Vec<u8>>(8);

    // 终端命令注入 / 输出广播通道（与 SSH 路径一致，供 POST /api/ssh/send/{sid} 使用，
    // 让 AI sidecar 也能对本地 PTY 会话执行命令并取回输出）。
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    let (output_tx, _) = broadcast::channel::<String>(64);
    ssh_svc::set_terminal_channels(&sid, cmd_tx, output_tx.clone()).await;

    // 阻塞读线程：扫描 ESC[xn DSR 查询并代为应答，避免 ConPTY 启动时无人回应

    // Windows ConPTY 下 shell 退出后 reader.read() 不会返回 EOF，
    // 需要单独监听子进程退出信号来驱动主循环 break。
    let (exit_tx, mut exit_rx) = mpsc::channel::<()>(1);
    #[cfg(windows)]
    {
        if let Some(pid) = child.process_id() {
            tokio::task::spawn_blocking(move || {
                use windows_sys::Win32::Foundation::CloseHandle;
                use windows_sys::Win32::System::Threading::{
                    OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
                };
                unsafe {
                    let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
                    if !handle.is_null() {
                        WaitForSingleObject(handle, 0xFFFFFFFF);
                        CloseHandle(handle);
                    }
                }
                let _ = exit_tx.blocking_send(());
            });
        } else {
            let _ = exit_tx;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = exit_tx;
    }

    let read_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    let mut i = 0;
                    while i + 3 < chunk.len() {
                        if chunk[i] == 0x1b && chunk[i + 1] == b'[' {
                            let mut j = i + 2;
                            while j < chunk.len() && !(0x40..=0x7e).contains(&chunk[j]) {
                                j += 1;
                            }
                            if j < chunk.len() && chunk[j] == b'n' {
                                let param = &chunk[i + 2..j];
                                let reply: &[u8] = if param == b"6" {
                                    b"\x1b[1;1R"
                                } else if param == b"5" {
                                    b"\x1b[0n"
                                } else {
                                    &[]
                                };
                                if !reply.is_empty() {
                                    let _ = dsr_tx.blocking_send(reply.to_vec());
                                }
                            }
                            i = j + 1;
                        } else {
                            i += 1;
                        }
                    }

                    if out_tx.blocking_send(chunk.to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 主事件循环
    loop {
        tokio::select! {
            // Windows: 子进程退出信号
            Some(()) = exit_rx.recv() => {
                break;
            }
            chunk = out_rx.recv() => {
                match chunk {
                    Some(chunk) => {
                        // 与远程 SSH 一致：输出走 Binary 帧（保留任意字节）
                        if ws_tx.send(Message::Binary(chunk.clone().into())).await.is_err() {
                            break;
                        }
                        // 同步广播给 AI sidecar 等外部订阅者（lossy：订阅者 lag 时丢老数据）
                        let _ = output_tx.send(String::from_utf8_lossy(&chunk).to_string());
                    }
                    None => break,
                }
            }
            Some(reply) = dsr_rx.recv() => {
                if writer.write_all(&reply).is_err() { break; }
                let _ = writer.flush();
            }
            // 外部命令注入（POST /api/ssh/send/{sid}）：与 ws 输入同样规一化行尾
            Some(data) = cmd_rx.recv() => {
                let normalized = data.replace("\r\n", "\r").replace('\n', "\r");
                if writer.write_all(normalized.as_bytes()).is_err() { break; }
                let _ = writer.flush();
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else { break; };
                match msg {
                    Ok(Message::Text(text)) => {
                        let s = text.as_str();
                        if s.starts_with('{') && s.ends_with('}') {
                            match serde_json::from_str::<ClientMsg>(s) {
                                Ok(ClientMsg::Cmd { data }) => {
                                    let normalized = data.replace("\r\n", "\r").replace('\n', "\r");
                                    if writer.write_all(normalized.as_bytes()).is_err() { break; }
                                    let _ = writer.flush();
                                }
                                Ok(ClientMsg::Resize { cols, rows }) => {
                                    log::info!(
                                        "local_pty: resize sid={sid} cols={cols} rows={rows}"
                                    );
                                    if let Err(e) = master.resize(PtySize {
                                        rows,
                                        cols,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    }) {
                                        log::warn!(
                                            "local_pty: resize failed sid={sid}: {e}"
                                        );
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
                                    let normalized = s.replace("\r\n", "\r").replace('\n', "\r");
                                    if writer.write_all(normalized.as_bytes()).is_err() { break; }
                                    let _ = writer.flush();
                                }
                            }
                        } else {
                            let normalized = s.replace("\r\n", "\r").replace('\n', "\r");
                            if writer.write_all(normalized.as_bytes()).is_err() { break; }
                            let _ = writer.flush();
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        if writer.write_all(&bytes).is_err() { break; }
                        let _ = writer.flush();
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }

    log::info!("local_pty cleanup sid={sid}");
    let _ = child.kill();
    let _ = child.wait();
    read_task.abort();
    ssh_svc::remove_terminal_channels(&sid).await;
    let _ = ws_tx.close().await;
}
