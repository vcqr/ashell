//! tmux 会话管理（REST 服务层）
//!
//! GET  /api/ssh/tmux/sessions?sid=&host_id=
//! POST /api/ssh/tmux/kill { sid, name, host_id }
//!
//! 复用已建立的 SSH 会话（get_client）执行 tmux 命令。堡垒机会话直接拒绝：
//! exec 落在网关的菜单 shell 上而非目标机，tmux 命令触达不了目标机（与
//! SFTP 提权的拒绝理由一致）。execute 本身无超时，对 exec 不回应的设备会
//! 永久挂起——每条命令必须包硬超时（同 SHELL_PROBE 模式），否则 REST 请求
//! 悬死并拖住面板轮询。

use std::time::Duration;

use serde::Serialize;

use crate::errors::{AppError, AppResult};
use crate::models::DbPool;
use crate::service::host;
use crate::service::sftp::sh_quote;
use crate::service::ssh as ssh_svc;

/// exec 硬超时：堡垒机等对 exec 不回应的服务器上，超时即放弃
const TMUX_EXEC_TIMEOUT: Duration = Duration::from_secs(5);

/// 会话列表命令：先探测 tmux 是否可用（`__NO_TMUX__` 标记，避免依赖
/// "command not found"/"no server running" 这类受远端 LANG 影响的本地化
/// 文案做判断），再取四列冒号分隔的会话信息。分隔符用冒号而非制表符：
/// tmux format 语法不解释 `\t`（会原样输出反斜杠+t），而会话名天然禁止
/// 含冒号（target 语法保留字符），冒号切分绝对安全。tmux ls 的 stderr
/// 丢弃：无运行中 server 的报错无需区分细节，空 stdout 即空列表。
const LIST_CMD: &str = "command -v tmux >/dev/null 2>&1 || echo __NO_TMUX__; tmux ls -F '#{session_name}:#{session_windows}:#{session_attached}:#{session_created}' 2>/dev/null";

#[derive(Debug, Serialize)]
pub struct TmuxSession {
    pub name: String,
    pub windows: u32,
    pub attached: u32,
    /// 会话创建时间（unix 秒）
    pub created: i64,
}

/// 堡垒机拒绝：host_id 查库读 bastion 标记（与 SFTP 提权拒绝同理由）
async fn ensure_not_bastion(pool: &DbPool, host_id: i64) -> AppResult<()> {
    let h = host::get_by_id(pool, host_id).await?;
    if h.bastion {
        return Err(AppError::BadRequest(
            "堡垒机会话不支持 tmux 会话管理：exec 落在网关菜单 shell 上，无法触达目标机".into(),
        ));
    }
    Ok(())
}

/// get_client + execute + 硬超时。sid 不存在（会话已断开）时 get_client
/// 返回 NotFound，前端按会话失效提示。
async fn exec_with_timeout(
    sid: &str,
    cmd: &str,
) -> AppResult<ssh_svc::CommandExecutedResult> {
    let session = ssh_svc::get_client(sid).await?;
    let res = tokio::time::timeout(TMUX_EXEC_TIMEOUT, session.execute(cmd))
        .await
        .map_err(|_| {
            AppError::BadRequest("远端执行 tmux 命令超时（该设备可能不支持 exec）".into())
        })??;
    Ok(res)
}

/// 列出远端 tmux 会话。tmux 未安装按 BadRequest 报错（前端内联提示）；
/// 无运行中的 tmux server 返回空列表。
pub async fn list(
    sid: &str,
    host_id: i64,
    pool: &DbPool,
) -> AppResult<Vec<TmuxSession>> {
    ensure_not_bastion(pool, host_id).await?;
    let res = exec_with_timeout(sid, LIST_CMD).await?;
    if res.stdout.contains("__NO_TMUX__") {
        return Err(AppError::BadRequest("远端未安装 tmux".into()));
    }
    if res.exit_status != 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for line in res.stdout.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, ':');
        let (Some(name), Some(w), Some(a), Some(c)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        // 解析失败的行跳过（会话名理论上可含任意字符，但四列结构必需完整）
        let (Ok(windows), Ok(attached), Ok(created)) = (
            w.trim().parse::<u32>(),
            a.trim().parse::<u32>(),
            c.trim().parse::<i64>(),
        ) else {
            continue;
        };
        out.push(TmuxSession {
            name: name.to_string(),
            windows,
            attached,
            created,
        });
    }
    Ok(out)
}

/// 终止远端 tmux 会话。会话名经 sh_quote 防注入；失败把 tmux 的 stderr
/// 回给前端（含截断），便于区分"会话不存在 / 权限不足"等情形。
pub async fn kill(
    sid: &str,
    name: &str,
    host_id: i64,
    pool: &DbPool,
) -> AppResult<()> {
    ensure_not_bastion(pool, host_id).await?;
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 256 {
        return Err(AppError::BadRequest("tmux 会话名无效".into()));
    }
    let cmd = format!("tmux kill-session -t {} 2>&1", sh_quote(trimmed));
    let res = exec_with_timeout(sid, &cmd).await?;
    if res.exit_status != 0 {
        let detail = res.stderr.trim();
        let detail = if detail.is_empty() {
            "tmux kill-session 失败"
        } else if detail.len() > 200 {
            &detail[..detail
                .char_indices()
                .nth(96)
                .map(|(i, _)| i)
                .unwrap_or(200)]
        } else {
            detail
        };
        return Err(AppError::BadRequest(format!("终止会话失败：{detail}")));
    }
    Ok(())
}
