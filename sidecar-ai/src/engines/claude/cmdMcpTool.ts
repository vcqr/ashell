import { tool, createSdkMcpServer } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";
import { execRemoteCommand } from "../../remote";
import type { EngineContext } from "../types";

/**
 * cmd_exec MCP 工具（in-process server）：向当前 SSH 会话注入命令并回收输出。
 * addr/ssid/token 由系统提示词下发给模型，模型调用时回填（保持 sidecar-cc 原有契约）。
 */
export function createCmdExecMcpServer(ctx: EngineContext) {
  const cmdExec = tool(
    "exec",
    "Execute a command",
    {
      addr: z.string().describe("address of the server to send the command to"),
      ssid: z.string().describe("session id"),
      token: z.string().describe("token"),
      cmd: z.string().describe("The command to execute"),
      wait_ms: z
        .number()
        .describe("Time to wait before executing the command, in milliseconds")
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
