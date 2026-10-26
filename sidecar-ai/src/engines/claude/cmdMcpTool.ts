import { tool, createSdkMcpServer } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";
import { detectDanger } from "../../dangerCheck";
import { emitToolConfirm } from "../../protocol";
import { execRemoteCommand } from "../../remote";
import { STOP_SENTINEL } from "../../stdin";
import type { EngineContext } from "../types";

/**
 * cmd_exec MCP 工具（in-process server）：向当前终端会话注入命令并回收输出。
 * addr/ssid/token 经 ctx 闭包注入（不进模型上下文/CLI transcript）。
 * 后端按完成事件判定命令结束并上报退出码；wait_ms 仅为挂死兜底上限。
 *
 * 审批在工具执行体内部进行（canUseTool 因 allowedTools 预授权不会触发）：
 * 服务端静态检测命中危险模式强制确认（模型无法绕过），模型标记
 * needs_approval=true 作为第二道保险。默认拒绝：非 y / 中断一律不执行。
 */
export function createCmdExecMcpServer(ctx: EngineContext) {
  const cmdExec = tool(
    "exec",
    "Execute a command on the remote shell and wait for it to finish",
    {
      cmd: z.string().describe("The command to execute"),
      wait_ms: z
        .number()
        .describe(
          "Hard cap in milliseconds for waiting on the command (default 10000). " +
            "Most commands return earlier, when the shell reports completion. " +
            "Raise it for commands that legitimately run long (builds, large transfers).",
        )
        .optional(),
      needs_approval: z
        .boolean()
        .describe(
          "Set to true for operations the user should confirm first (destructive commands, " +
            "or risky actions static detection cannot see, e.g. restarting production services). Default: false.",
        )
        .optional(),
    },
    async (args) => {
      const verdict = detectDanger(args.cmd);
      if (args.needs_approval || verdict.dangerous) {
        emitToolConfirm(ctx.ssid, "Allow this action? (y/n): ", {
          command: args.cmd,
          ...(verdict.dangerous && { risk_reason: verdict.reason }),
        });
        const response = (await ctx.io.readLineOrStop()).trim().toLowerCase();
        if (response === STOP_SENTINEL || response !== "y") {
          return {
            content: [{ type: "text" as const, text: "User denied this action" }],
          };
        }
      }

      const r = await execRemoteCommand(ctx, args.cmd, args.wait_ms);

      let statusLine: string;
      if (r.status !== 200) {
        statusLine = `http_status: ${r.status}`;
      } else if (r.exitCode !== null) {
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

      return {
        content: [{ type: "text" as const, text: `${statusLine}\n\n${r.output}` }],
      };
    },
  );

  return createSdkMcpServer({
    name: "cmd",
    version: "1.0.0",
    tools: [cmdExec],
  });
}
