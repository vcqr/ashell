import { tool, createSdkMcpServer } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";
import { execRemoteCommand } from "../../remote";
import type { EngineContext } from "../types";

/**
 * cmd_exec MCP 工具（in-process server）：向当前 SSH 会话注入命令并回收输出。
 * addr/ssid/token 经 ctx 闭包注入（不进模型上下文/CLI transcript），
 * 模型只提供命令与输出收集窗口。
 */
export function createCmdExecMcpServer(ctx: EngineContext) {
  const cmdExec = tool(
    "exec",
    "Execute a command on the remote SSH session",
    {
      cmd: z.string().describe("The command to execute"),
      wait_ms: z
        .number()
        .describe(
          "Output collection window after sending the command, in milliseconds (default 500)",
        )
        .optional(),
    },
    async (args) => {
      const { text } = await execRemoteCommand(ctx, args.cmd, args.wait_ms);
      return {
        content: [{ type: "text" as const, text: `result: ${text}` }],
      };
    },
  );

  return createSdkMcpServer({
    name: "cmd",
    version: "1.0.0",
    tools: [cmdExec],
  });
}
