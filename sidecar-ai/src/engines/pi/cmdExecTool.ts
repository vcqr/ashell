import { defineTool } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { emitToolConfirm } from "../../protocol";
import { execRemoteCommand, type SshTarget } from "../../remote";
import { STOP_SENTINEL } from "../../stdin";

/**
 * 远程命令执行工具。addr/ssid/token 启动时已知，经闭包注入（不进模型上下文）。
 * 危险命令由 LLM 自主标记 needs_approval=true，工具先发 [TOOL_CONFIRM] 等 y/n。
 */
export function createCmdExecTool(
  target: SshTarget,
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
      if (params.needs_approval) {
        emitToolConfirm("Allow this action? (y/n): ", { command: params.cmd });

        const response = (await readLineFromStdin()).trim().toLowerCase();
        if (response === STOP_SENTINEL || response !== "y") {
          return {
            content: [{ type: "text" as const, text: "User denied this action" }],
            details: { denied: true, command: params.cmd },
          };
        }
      }

      const { status, text } = await execRemoteCommand(target, params.cmd, params.wait_ms);

      return {
        content: [{ type: "text" as const, text: `result: ${text}` }],
        details: {
          command: params.cmd,
          wait_ms: params.wait_ms,
          status,
        },
      };
    },
  });
}
