import { query, type CanUseTool } from "@anthropic-ai/claude-agent-sdk";
import { join } from "path";
import { platform } from "os";
import { existsSync } from "fs";
import { askUserQuestion, type AskUserQuestionItem } from "../../askUser";
import { STOP_SENTINEL } from "../../stdin";
import {
  emitAIMSG,
  emitSystemError,
  emitToolCall,
  emitToolConfirm,
  emitToolRet,
} from "../../protocol";
import { displayTodoProgress, type TodoItem } from "../../progress";
import { createCmdExecMcpServer } from "./cmdMcpTool";
import type { EngineAdapter, EngineContext } from "../types";

/**
 * Claude Agent SDK 引擎适配器（移植自 sidecar-cc）。
 *
 * - 每条用户消息起一个 query()，凭 sessionId resume 延续上下文
 * - 审批走 canUseTool：[TOOL_CONFIRM] + stdin y/n
 * - AskUserQuestion 走 canUseTool 分支：[AI_ASKUSERQUESTION] + 选项序号
 * - TodoWrite 由消息流拦截，经 displayTodoProgress 推送进度
 */
export function createClaudeEngine(ctx: EngineContext): EngineAdapter {
  // CLI 路径：环境变量优先，回退工作区 bin/ 下的 claude 可执行文件。
  // trim 防御手输配置混入的首尾空白（曾导致 spawn 报隐晦的 uv_spawn 错误）
  const cliPath = (
    process.env.CLAUDE_CLI_PATH?.trim() ||
    join(ctx.homedir, "bin", platform() === "win32" ? "claude.exe" : "claude")
  ).replace(/\\/g, "/");

  if (!existsSync(cliPath)) {
    throw new Error(
      `Claude CLI not found: ${cliPath}. 请检查 ~/.ashell/ai/.env 的 CLAUDE_CLI_PATH 配置，或在设置里重新检测路径`,
    );
  }
  console.log("claude path", cliPath);

  const cmdExecServer = createCmdExecMcpServer(ctx);

  const appendPrompt = `SSH session information: "addr:${ctx.addr}, ssid:${ctx.ssid}, token:${ctx.token}".  Please use this data to execute commands on the remote server. When using the mcp__cmd_exec tool, populate it with these credentials. This information is strictly confidential; it must be used solely for executing commands and must never be directly revealed to the user. Note: For commands involving destructive operations, please request user approval before execution.`;

  let sessionId: string | undefined;
  let todos: TodoItem[] = [];
  let currentQuery: ReturnType<typeof query> | null = null;
  let stopped = false;

  const canUseTool: CanUseTool = async (toolName, input) => {
    if (toolName === "AskUserQuestion") {
      const answers: Record<string, string> = {};
      let interrupted = false;
      for (const q of (input.questions ?? []) as AskUserQuestionItem[]) {
        const answer = await askUserQuestion(ctx.io, q);
        if (answer === null) {
          interrupted = true;
          break;
        }
        answers[q.question] = answer;
      }
      if (interrupted) {
        return { behavior: "deny", message: "用户已中断" };
      }
      return {
        behavior: "allow",
        updatedInput: { questions: input.questions, answers },
      };
    }

    emitToolConfirm("Allow this action? (y/n): ", input);
    const response = await ctx.io.readLineOrStop();
    console.log(`[USER_RESPONSE] ${response}`);

    if (response === STOP_SENTINEL) {
      stopped = true;
      return { behavior: "deny", message: "用户已中断" };
    }
    if (response.toLowerCase().trim() === "y") {
      return { behavior: "allow", updatedInput: input };
    }
    return { behavior: "deny", message: "User denied this action" };
  };

  function requestStop() {
    stopped = true;
    // interrupt() 内部清理时会 reject "Query closed before response received"，
    // 属 SDK 预期行为，静默处理；等待中的审批/提问由 StdinIO.signalStop 释放
    currentQuery?.interrupt().catch(() => {});
  }

  async function handle(userPrompt: string): Promise<void> {
    stopped = false;
    try {
      const queryIterator = query({
        prompt: userPrompt,
        options: {
          cwd: ctx.homedir,
          ...(sessionId && { resume: sessionId }),
          permissionMode: "acceptEdits",
          mcpServers: { cmd_exec: cmdExecServer },
          allowedTools: [
            "Skill",
            "Write",
            "TodoWrite",
            "Read",
            "WebFetch",
            "mcp__cmd_exec",
          ],
          tools: [
            "Read",
            "Glob",
            "Grep",
            "Edit",
            "Bash",
            "AskUserQuestion",
            "Skill",
            "Write",
            "TodoWrite",
            "WebFetch",
          ],
          pathToClaudeCodeExecutable: cliPath,
          settingSources: ["project", "local"],
          canUseTool,
          hooks: {},
          systemPrompt: {
            type: "preset",
            preset: "claude_code",
            append: appendPrompt,
          },
        },
      });

      currentQuery = queryIterator;

      for await (const message of queryIterator) {
        if (message.type === "system") {
          if (message.subtype === "init") {
            sessionId = message.session_id;
            console.log(`SESSION_ID:${sessionId}`);
          }
          if (message.subtype === "api_retry") {
            emitSystemError(String(message.error));
          }
        }

        if (
          message.type === "user" &&
          message.message?.content &&
          message.tool_use_result
        ) {
          emitToolRet(message.message.content);
        }

        if (message.type === "assistant" && message.message?.content) {
          for (const block of message.message.content) {
            if ("text" in block) {
              emitAIMSG(block.text);
            } else if ("name" in block) {
              const input = (block.input ?? {}) as Record<string, any>;
              if (block.name === "TodoWrite") {
                todos = Array.isArray(input.todos) ? input.todos : [];
                displayTodoProgress(todos);
              } else {
                emitToolCall({
                  name: block.name,
                  command: input.command,
                  description: input.description,
                });
              }
            }
          }
        }
      }
    } catch (error) {
      if (stopped) {
        // interrupt 引发的 "Query closed before response received" 属预期，静默
        console.log("[HANDLE] Error suppressed (caused by interrupt)");
        return;
      }
      console.error("[HANDLE] Error:", error);
      throw error;
    } finally {
      currentQuery = null;
    }
  }

  return {
    name: "claude",
    get stopped() {
      return stopped;
    },
    stop: requestStop,
    handle,
  };
}
