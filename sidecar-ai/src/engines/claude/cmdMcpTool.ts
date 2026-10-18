import { tool, createSdkMcpServer } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";
import { execRemoteCommand } from "../../remote";
import type { EngineContext } from "../types";

/**
 * cmd_exec MCP 工具（in-process server）：向当前终端会话注入命令并回收输出。
 * addr/ssid/token 经 ctx 闭包注入（不进模型上下文/CLI transcript）。
 * 后端按完成事件判定命令结束并上报退出码；wait_ms 仅为挂死兜底上限。
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
    },
    async (args) => {
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
