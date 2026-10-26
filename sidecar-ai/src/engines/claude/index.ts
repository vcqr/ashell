import { query, type CanUseTool } from "@anthropic-ai/claude-agent-sdk";
import { join } from "path";
import { platform } from "os";
import { existsSync } from "fs";
import { askUserQuestion, type AskUserQuestionItem } from "../../askUser";
import { STOP_SENTINEL } from "../../stdin";
import {
  emitAIMSG,
  emitCompact,
  emitSystemError,
  emitToolCall,
  emitToolConfirm,
  emitToolRet,
  emitUsage,
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

  // 凭证（addr/ssid/token）经 cmd_exec MCP 工具的 ctx 闭包注入，绝不写入提示词
  // （appendPrompt 会进 CLI 会话历史并随 resume 落盘）；此处只约束工具使用方式
  // 与破坏性操作审批
  const appendPrompt = `Use the mcp__cmd_exec tool to execute commands on the remote SSH session the user is connected to; its connection credentials are already configured internally and must not be requested or guessed. For operations the user should confirm first (destructive commands, or risky actions static detection cannot see such as restarting production services), set needs_approval=true on the tool call; destructive commands are additionally detected automatically and always require user confirmation before execution.`;

  let sessionId: string | undefined;
  let todos: TodoItem[] = [];
  let currentQuery: ReturnType<typeof query> | null = null;
  let stopped = false;
  // total_cost_usd 是跨 resume 的累计值，本轮成本需差分（新会话从 0 起算）
  let lastCumulativeCost = 0;
  // 上下文控制（.env 的 AI_*，两引擎通用）：输出上限经 env 下发给 CLI 子进程，
  // 窗口配置覆盖用量上报的 contextWindow（进度条显示），轮数超限触发 /compact
  const aiMaxTokens = Number.parseInt(process.env.AI_MAX_TOKENS || "", 10);
  const aiContextWindow = Number.parseInt(process.env.AI_CONTEXT_WINDOW || "", 10);
  const aiMaxRounds = Number.parseInt(process.env.AI_MAX_ROUNDS || "", 10);
  const contextWindowOverride =
    Number.isFinite(aiContextWindow) && aiContextWindow > 0 ? aiContextWindow : undefined;
  const maxRounds =
    Number.isFinite(aiMaxRounds) && aiMaxRounds > 0 ? aiMaxRounds : undefined;
  if (Number.isFinite(aiMaxTokens) && aiMaxTokens > 0) {
    // CLI 子进程继承 process.env；daemon 多会话共享，后建会话覆盖（与 ANTHROPIC_* 同模式）
    process.env.CLAUDE_CODE_MAX_OUTPUT_TOKENS = String(aiMaxTokens);
  }
  let roundCount = 0;

  const canUseTool: CanUseTool = async (toolName, input) => {
    if (toolName === "AskUserQuestion") {
      const answers: Record<string, string> = {};
      let interrupted = false;
      for (const q of (input.questions ?? []) as AskUserQuestionItem[]) {
        const answer = await askUserQuestion(ctx.io, ctx.ssid, q);
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

    emitToolConfirm(ctx.ssid, "Allow this action? (y/n): ", input);
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

  /**
   * 跑一轮 query 并转译事件流。silent 模式用于轮数触发的 /compact：
   * 吞掉文本/工具/用量输出，只保留 init 与 compact_boundary（压缩提示照常上报）。
   */
  async function runQuery(prompt: string, silent = false): Promise<void> {
    try {
      const queryIterator = query({
        prompt,
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
            emitSystemError(ctx.ssid, String(message.error));
          }
          // CLI 内置自动压缩：透传压缩事件给前端展示
          if (message.subtype === "compact_boundary") {
            const meta = message.compact_metadata;
            emitCompact(ctx.ssid, {
              trigger: meta.trigger === "manual" ? "manual" : "auto",
              tokensBefore: meta.pre_tokens,
              tokensAfter: meta.post_tokens,
            });
          }
        }

        if (
          !silent &&
          message.type === "user" &&
          message.message?.content &&
          message.tool_use_result
        ) {
          emitToolRet(ctx.ssid, message.message.content);
        }

        // 每轮结束时 result 携带本轮 token 用量与累计成本
        if (message.type === "result" && !silent) {
          const usage = message.usage;
          const cumulativeCost = message.total_cost_usd ?? 0;
          const turnCost = Math.max(0, cumulativeCost - lastCumulativeCost);
          lastCumulativeCost = cumulativeCost;

          // 上下文窗口取本轮用量最大的模型条目（主循环模型）
          let contextWindow: number | undefined;
          let maxCost = -1;
          for (const mu of Object.values(message.modelUsage ?? {})) {
            if (mu.costUSD > maxCost) {
              maxCost = mu.costUSD;
              contextWindow = mu.contextWindow;
            }
          }
          // .env 显式配置的窗口优先（进度条按用户配置的比例显示）
          if (contextWindowOverride) contextWindow = contextWindowOverride;

          emitUsage(ctx.ssid, {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cacheRead: usage.cache_read_input_tokens ?? 0,
            cacheWrite: usage.cache_creation_input_tokens ?? 0,
            cost: turnCost > 0 ? turnCost : undefined,
            // 本轮 prompt 总量即请求时上下文占用
            contextTokens:
              usage.input_tokens +
              (usage.cache_read_input_tokens ?? 0) +
              (usage.cache_creation_input_tokens ?? 0),
            contextWindow,
            contextPercent:
              contextWindow && contextWindow > 0
                ? Math.round(
                    ((usage.input_tokens +
                      (usage.cache_read_input_tokens ?? 0) +
                      (usage.cache_creation_input_tokens ?? 0)) /
                      contextWindow) *
                      100,
                  )
                : undefined,
          });
        }

        if (!silent && message.type === "assistant" && message.message?.content) {
          for (const block of message.message.content) {
            if ("text" in block) {
              emitAIMSG(ctx.ssid, block.text);
            } else if ("name" in block) {
              const input = (block.input ?? {}) as Record<string, any>;
              if (block.name === "TodoWrite") {
                todos = Array.isArray(input.todos) ? input.todos : [];
                displayTodoProgress(ctx.ssid, todos);
              } else {
                emitToolCall(ctx.ssid, {
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

  async function handle(userPrompt: string): Promise<void> {
    stopped = false;
    // 轮数控制：达到上限后先静默跑一轮 /compact 把早期对话收敛为摘要，再处理真实提问
    if (maxRounds && sessionId && roundCount >= maxRounds) {
      console.log(`[CLAUDE] ${roundCount} rounds >= max ${maxRounds}, auto-compacting`);
      try {
        await runQuery("/compact", true);
        roundCount = 0;
      } catch (error) {
        // 压缩失败不阻塞本轮提问；中断要向上传播
        if (stopped) throw error;
        console.warn("[CLAUDE] auto-compact failed:", error);
      }
    }
    if (stopped) return; // 压缩期间被中断，不再继续本轮提问
    roundCount += 1;
    await runQuery(userPrompt);
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
