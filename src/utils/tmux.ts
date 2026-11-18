/**
 * tmux 命令组装（一期自动接入 / 二期会话面板共用）。
 *
 * 转义规则：安全字符（字母数字下划线连字符）直接拼；其余整名用单引号
 * 包裹并把内部单引号写成 `'\''`（POSIX sh 语法，dash/ash/bash/zsh 通吃）。
 */

function quoteSessionName(name: string): string {
  if (/^[A-Za-z0-9_-]+$/.test(name)) return name;
  return `'${name.replace(/'/g, `'\\''`)}'`;
}

/** 一期：连接就绪后自动接入（存在即 attach、不存在则新建；留空用默认会话） */
export function buildTmuxCommand(session: string | null | undefined): string {
  const name = (session ?? "").trim();
  if (!name) return "tmux new -A";
  return `tmux new -A -s ${quoteSessionName(name)}`;
}

/**
 * 二期：会话面板「接入」——当前标签内切入目标会话，不新建 SSH 连接。
 * 已在 tmux 内用 switch-client（瞬时切换）；在裸 shell 里 switch-client
 * 会失败，落到 attach（stderr 丢弃避免刷屏）。
 */
export function buildTmuxSwitchCommand(name: string): string {
  const safe = quoteSessionName(name.trim());
  return `tmux switch-client -t ${safe} 2>/dev/null || tmux attach -t ${safe}`;
}
