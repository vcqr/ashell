/**
 * 远程命令执行：POST 到 axum 的 /api/ssh/send/{ssid}，
 * 向已有终端会话注入命令并回收输出。claude（MCP 工具）与 pi（自定义工具）共用。
 *
 * 新版后端返回结构化结果（output/exit_code/completed/…，由遥测钩子判定
 * 命令完成）；旧版 data 为纯字符串。此处统一归一为 RemoteExecResult，
 * 调用方无需关心后端版本差异。
 */

export interface SshTarget {
  addr: string;
  ssid: string;
  token: string;
}

export interface RemoteExecResult {
  /** HTTP 状态码（非 200 时 output 为原始响应文本，其余字段为失败缺省） */
  status: number;
  /** 命令输出（后端已剥命令回显与 ANSI 转义序列） */
  output: string;
  /** 退出码：bash/zsh 会话由完成钩子上报；其余会话为 null */
  exitCode: number | null;
  /** 是否确认完成（完成事件数达到预期） */
  completed: boolean;
  /** 是否因到达 wait_ms 上限而返回（输出可能不完整） */
  timedOut: boolean;
  /** 收集期间会话输出流是否关闭 */
  sessionClosed: boolean;
  /** 输出是否超上限被截断 */
  truncated: boolean;
}

function failure(status: number, text: string): RemoteExecResult {
  return {
    status,
    output: text,
    exitCode: null,
    completed: false,
    timedOut: false,
    sessionClosed: false,
    truncated: false,
  };
}

export async function execRemoteCommand(
  target: SshTarget,
  cmd: string,
  waitMs?: number,
): Promise<RemoteExecResult> {
  let url = `http://${target.addr}/api/ssh/send/${target.ssid}`;
  if (waitMs) url += `?wait_ms=${waitMs}`;
  console.log("[cmd_exec] sending command to url:", url);

  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "text/plain",
      Authorization: `Bearer ${target.token}`,
    },
    body: cmd + "\n",
  });

  const status = response.status;
  const text = await response.text();
  if (status !== 200) {
    return failure(status, text);
  }

  // ApiResponse 信封：{code, message, data}；data 为对象（新版）或字符串（旧版）
  try {
    const json = JSON.parse(text);
    if (json && typeof json === "object" && "data" in json) {
      const d = json.data;
      if (typeof d === "string") {
        return { ...failure(status, d), completed: false };
      }
      return {
        status,
        output: typeof d?.output === "string" ? d.output : "",
        exitCode: typeof d?.exit_code === "number" ? d.exit_code : null,
        completed: d?.completed === true,
        timedOut: d?.timed_out === true,
        sessionClosed: d?.session_closed === true,
        truncated: d?.truncated === true,
      };
    }
  } catch {
    // 非 JSON 响应，按原始文本处理
  }
  return failure(status, text);
}
