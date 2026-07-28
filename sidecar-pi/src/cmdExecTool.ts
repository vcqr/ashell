import { defineTool } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

/**
 * 创建远程命令执行工具。
 *
 * 参照 sidecar-cc/src/cmdMcpTool.ts 的逻辑：
 * - POST http://${addr}/api/ssh/send/${ssid}
 * - 可选 ?wait_ms=N query 参数
 * - Content-Type: text/plain, Authorization: Bearer ${token}
 * - body: cmd + "\n"
 *
 * 审批逻辑（LLM 自主决定）：
 * - LLM 对危险命令设置 needs_approval=true
 * - 工具发送 [TOOL_CONFIRM] 给前端，等待 stdin y/n
 * - 用户拒绝时返回 "User denied this action"
 *
 * addr / ssid / token 在 sidecar 启动时已知，通过闭包注入。
 * readLineFromStdin 由 index.ts 注入，用于等待用户审批。
 */
export function createCmdExecTool(
  ssh_addr: string,
  ssh_ssid: string,
  ssh_token: string,
  readLineFromStdin: () => Promise<string>,
) {
  return defineTool({
    name: "cmd_exec",
    label: "Remote Command Execution",
    description:
      "Execute a command on the remote SSH server. Use this to run commands on the connected server. " +
      "For destructive operations (rm, kill, shutdown, reboot, chmod, etc.), set needs_approval=true to request user confirmation before execution.",
    promptSnippet: "cmd_exec: execute a command on the remote SSH server",
    parameters: Type.Object({
      cmd: Type.String({ description: "The command to execute on the remote server" }),
      wait_ms: Type.Optional(
        Type.Number({
          description: "Time to wait before executing the command, in milliseconds",
        }),
      ),
      needs_approval: Type.Optional(
        Type.Boolean({
          description:
            "Set to true for destructive operations (rm, kill, shutdown, etc.) to require user approval before execution. Default: false.",
        }),
      ),
    }),
    async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
      // 如果 LLM 标记需要审批，发送 [TOOL_CONFIRM] 等待用户确认
      if (params.needs_approval) {
        const confirmMsg = {
          type: "tool_confirm",
          payload: {
            question: "Allow this action? (y/n): ",
            options: { command: params.cmd },
          },
        };
        console.log(`[TOOL_CONFIRM]${JSON.stringify(confirmMsg)}`);

        const response = (await readLineFromStdin()).trim().toLowerCase();
        if (response !== "y") {
          return {
            content: [{ type: "text" as const, text: "User denied this action" }],
            details: { denied: true, command: params.cmd },
          };
        }
      }

      let url = `http://${ssh_addr}/api/ssh/send/${ssh_ssid}`;
      if (params.wait_ms) {
        url += "?wait_ms=" + params.wait_ms;
      }
      console.log("[cmd_exec] sending command to url:", url);

      const fetchResponse = await fetch(url, {
        method: "POST",
        headers: {
          "Content-Type": "text/plain",
          Authorization: `Bearer ${ssh_token}`,
        },
        body: params.cmd + "\n",
      });

      const data = await fetchResponse.text();

      return {
        content: [{ type: "text" as const, text: `result: ${data}` }],
        details: {
          command: params.cmd,
          wait_ms: params.wait_ms,
          status: fetchResponse.status,
        },
      };
    },
  });
}
