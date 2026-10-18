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
      "Execute a command on the remote shell and wait for it to finish. " +
      "Returns the command output and its exit code when available. " +
      "wait_ms is only a hard cap for hanging commands (default 10000); normal commands return as soon as they complete. " +
      "Do not run interactive full-screen programs (top, vim, etc.) or commands that never exit (tail -f, servers): they always run until the cap. " +
      "For destructive operations (rm, kill, shutdown, reboot, chmod, etc.), set needs_approval=true to request user confirmation before execution.",
    promptSnippet: "cmd_exec: execute a command on the remote shell and get its output and exit code",
    parameters: Type.Object({
      cmd: Type.String({ description: "The command to execute on the remote shell" }),
      wait_ms: Type.Optional(
        Type.Number({
          description:
            "Hard cap in milliseconds for waiting on the command (default 10000). Most commands return earlier, when the shell reports completion. Raise it for commands that legitimately run long (builds, large transfers).",
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
        emitToolConfirm(target.ssid, "Allow this action? (y/n): ", { command: params.cmd });

        const response = (await readLineFromStdin()).trim().toLowerCase();
        if (response === STOP_SENTINEL || response !== "y") {
          return {
            content: [{ type: "text" as const, text: "User denied this action" }],
            details: { denied: true, command: params.cmd },
          };
        }
      }

      const r = await execRemoteCommand(target, params.cmd, params.wait_ms);

      // 状态行 + 输出：退出码优先；缺失时给出可信度提示，避免模型把
      // 不完整输出当成命令失败
      let statusLine: string;
      if (r.exitCode !== null) {
        statusLine = `exit_code: ${r.exitCode}`;
      } else if (r.sessionClosed) {
        statusLine = "session_closed: the terminal session ended during execution";
      } else if (r.timedOut) {
        statusLine =
          "timed_out: the command did not finish within the cap; output below may be partial";
      } else {
        statusLine = "completed: unknown (exit code unavailable for this shell type)";
      }
      if (r.truncated) {
        statusLine += "\ntruncated: output exceeded the size cap and was cut";
      }
      if (r.status !== 200) {
        statusLine = `http_status: ${r.status}`;
      }

      return {
        content: [{ type: "text" as const, text: `${statusLine}\n\n${r.output}` }],
        details: {
          command: params.cmd,
          wait_ms: params.wait_ms,
          status: r.status,
          exit_code: r.exitCode,
          completed: r.completed,
          timed_out: r.timedOut,
          session_closed: r.sessionClosed,
          truncated: r.truncated,
        },
      };
    },
  });
}
