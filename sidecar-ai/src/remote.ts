/**
 * 远程命令执行：POST 到 axum 的 /api/ssh/send/{ssid}，
 * 向已有 SSH 会话注入命令并回收输出。claude（MCP 工具）与 pi（自定义工具）共用。
 */

export interface SshTarget {
  addr: string;
  ssid: string;
  token: string;
}

export async function execRemoteCommand(
  target: SshTarget,
  cmd: string,
  waitMs?: number,
): Promise<{ status: number; text: string }> {
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

  return { status: response.status, text: await response.text() };
}
