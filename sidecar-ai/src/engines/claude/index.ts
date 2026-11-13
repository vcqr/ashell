import { query, type CanUseTool } from "@anthropic-ai/claude-agent-sdk";
import { join } from "path";
import { platform } from "os";
import { existsSync } from "fs";
import { askUserQuestion, type AskUserQuestionItem } from "../../askUser";
import { STOP_SENTINEL } from "../../stdin";
import {
  emitAIMSG,
  emitAIMSGDelta,
  emitCompact,
  emitSystemError,
  emitTextStart,
  emitThinking,
  emitThinkingDelta,
  emitThinkingStart,
  emitToolCall,
  emitToolConfirm,
  emitToolRet,
  emitUsage,
  isStreamingEnabled,
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

  // Claude Code 2.1.x：TodoWrite 已移除，换成默认关闭的 Task* 工具族（statsig
  // gate）。打开 gate 让 CLI 注入 TaskCreate/TaskUpdate/TaskGet/TaskList，恢复
  // todolist 进度；query 未传 env 选项，子进程继承本进程 env（对旧版 CLI 是
  // 未知 env，无害）。
  process.env.CLAUDE_CODE_ENABLE_TODO_TOOLS = "1";

  // 凭证（addr/ssid/token）经 cmd_exec MCP 工具的 ctx 闭包注入，绝不写入提示词
  // （appendPrompt 会进 CLI 会话历史并随 resume 落盘）；此处只约束工具使用方式
  // 与破坏性操作审批
  const appendPrompt = `Use the mcp__cmd_exec tool to execute commands on the remote SSH session the user is connected to; its connection credentials are already configured internally and must not be requested or guessed. For operations the user should confirm first (destructive commands, or risky actions static detection cannot see such as restarting production services), set needs_approval=true on the tool call; destructive commands are additionally detected automatically and always require user confirmation before execution. Use the todo/task tracking tools (TodoWrite, or TaskCreate/TaskUpdate on newer versions) to track progress on complex multi-step tasks so the user can follow along.`;

  let sessionId: string | undefined;
  let todos: TodoItem[] = [];
  // 2.1.x 的 Task* 工具族按 id 增量维护（TodoWrite 整表更新已移除）：
  // 调用侧追加/更新条目，工具结果侧回填 CLI 分配的 taskId
  interface TaskEntry {
    id?: string;
    item: TodoItem;
  }
  let taskEntries: TaskEntry[] = [];
  /** 本轮 tool_use_id → 工具名（结果侧路由用，prompt 开始时清空） */
  let toolUseNames = new Map<string, string>();
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
            // TodoWrite 是 <2.1 CLI 的整表 todo 工具；2.1+ 用 Task* 族（见上方 gate）
            "TodoWrite",
            "TaskCreate",
            "TaskUpdate",
            "TaskList",
            "TaskGet",
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
            "TaskCreate",
            "TaskUpdate",
            "TaskList",
            "TaskGet",
            "WebFetch",
          ],
          pathToClaudeCodeExecutable: cliPath,
          settingSources: ["project", "local"],
          // streaming 灰度开关：增量 stream_event 与完整消息并存，完整帧照发
          // 兼做前端校准；静默 /compact 轮不需要增量
          ...(isStreamingEnabled() && !silent && { includePartialMessages: true }),
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
      // 分段生命周期状态（每轮重置）：是否见过流式事件、按块索引累积的增量与块类型
      let streamedAny = false;
      const blockAcc = new Map<number, string>();
      const blockTypes = new Map<number, "text" | "thinking">();

      for await (const message of queryIterator) {
        // streaming 分段：content_block_start → START，delta 累积并转发，
        // content_block_stop → 立即 FINAL（全文），完整消息阶段跳过已发段。
        // parent_tool_use_id 非空的是子 agent 流，一期不上屏直接忽略
        if (
          !silent &&
          isStreamingEnabled() &&
          message.type === "stream_event" &&
          message.parent_tool_use_id === null
        ) {
          const ev = message.event;
          if (ev.type === "content_block_start") {
            const bt = ev.content_block?.type;
            if (bt === "text" || bt === "thinking") {
              streamedAny = true;
              blockTypes.set(ev.index, bt);
              blockAcc.set(ev.index, "");
              if (bt === "text") emitTextStart(ctx.ssid);
              else emitThinkingStart(ctx.ssid);
            }
          } else if (ev.type === "content_block_delta") {
            const delta = ev.delta;
            if (delta.type === "text_delta" && delta.text) {
              streamedAny = true;
              blockAcc.set(ev.index, (blockAcc.get(ev.index) ?? "") + delta.text);
              emitAIMSGDelta(ctx.ssid, delta.text);
            } else if (delta.type === "thinking_delta" && delta.thinking) {
              streamedAny = true;
              blockAcc.set(ev.index, (blockAcc.get(ev.index) ?? "") + delta.thinking);
              emitThinkingDelta(ctx.ssid, delta.thinking);
            }
          } else if (ev.type === "content_block_stop") {
            const bt = blockTypes.get(ev.index);
            const acc = blockAcc.get(ev.index);
            if (bt && acc !== undefined) {
              if (bt === "text") emitAIMSG(ctx.ssid, acc);
              else emitThinking(ctx.ssid, acc);
              blockTypes.delete(ev.index);
              blockAcc.delete(ev.index);
            }
          }
        }

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
          const blocks = message.message.content;
          const toolUseId = Array.isArray(blocks)
            ? (
                blocks.find(
                  (b: { type?: string; tool_use_id?: string }) =>
                    b?.type === "tool_result" && b.tool_use_id,
                ) as { tool_use_id?: string } | undefined
              )?.tool_use_id
            : undefined;
          const resultToolName =
            toolUseId != null ? toolUseNames.get(String(toolUseId)) : undefined;
          if (resultToolName === "TaskCreate") {
            // 回填 CLI 分配的 taskId：按 subject 匹配（并行创建时结果序可能与调用序错位）
            const out = message.tool_use_result as {
              task?: { id?: unknown; subject?: unknown };
            };
            const subject = out.task?.subject != null ? String(out.task.subject) : undefined;
            const entry =
              (subject != null &&
                taskEntries.find((t) => t.id == null && t.item.content === subject)) ||
              taskEntries.find((t) => t.id == null);
            if (entry && out.task?.id != null) entry.id = String(out.task.id);
          } else if (
            resultToolName === "TodoWrite" ||
            resultToolName === "TaskUpdate" ||
            resultToolName === "TaskList" ||
            resultToolName === "TaskGet"
          ) {
            // 任务簿记结果不进对话流：进度由调用侧 displayTodoProgress 呈现
          } else {
            // SDK 的 tool_result 块是嵌套形态 {type:"tool_result",content:[{type:"text",text}]}，
            // 拍平为协议形状 [{type,content:string}]（与 pi 引擎一致），否则前端会
            // 对内层数组走 JSON.stringify 兜底，结果行显示成原始 JSON
            const payload = (Array.isArray(blocks) ? blocks : []).flatMap(
              (b: {
                type?: string;
                text?: unknown;
                content?: unknown;
              }) => {
                if (b?.type === "tool_result" && Array.isArray(b.content)) {
                  return (b.content as Array<{ type?: string; text?: unknown }>).map(
                    (c) => ({
                      type: String(c?.type ?? "text"),
                      content: typeof c?.text === "string" ? c.text : "",
                    }),
                  );
                }
                return [
                  {
                    type: String(b?.type ?? "text"),
                    content:
                      typeof b?.text === "string"
                        ? b.text
                        : typeof b?.content === "string"
                          ? b.content
                          : "",
                  },
                ];
              },
            );
            // tool_result 的 is_error 透传（拒绝执行/命令失败等），前端渲染失败样式
            const isError = (Array.isArray(blocks) ? blocks : []).some(
              (b: { type?: string; is_error?: boolean }) =>
                b?.type === "tool_result" && b.is_error === true,
            );
            emitToolRet(ctx.ssid, payload, toolUseId, isError);
          }
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
          // streaming 模式下 text/thinking 段已在 content_block_stop 时 FINAL，
          // 完整消息只补工具调用；block 模式（或该消息未产出任何流式事件）走旧路径
          const streamingDone = isStreamingEnabled() && streamedAny;
          for (const block of message.message.content) {
            if ("text" in block) {
              if (!streamingDone) emitAIMSG(ctx.ssid, block.text);
            } else if ("thinking" in block) {
              // 本消息未产出流式事件（block 模式，或流式下该消息未走 stream_event）
              // 时补发 thinking 全帧，保证 block 模式下思考内容不丢
              if (!streamingDone && block.thinking) {
                emitThinking(ctx.ssid, block.thinking);
              }
            } else if ("name" in block) {
              const input = (block.input ?? {}) as Record<string, any>;
              const blockId = (block as { id?: string }).id;
              if (blockId) toolUseNames.set(String(blockId), block.name);
              if (block.name === "TodoWrite") {
                // <2.1 CLI：整表更新
                todos = Array.isArray(input.todos) ? input.todos : [];
                displayTodoProgress(ctx.ssid, todos);
              } else if (block.name === "TaskCreate") {
                // 2.1.x：TodoWrite 已移除，Task* 族按调用增量维护；taskId 由结果侧回填
                taskEntries.push({
                  item: {
                    content: String(input.subject ?? ""),
                    activeForm:
                      input.activeForm != null ? String(input.activeForm) : undefined,
                    status: "pending",
                  },
                });
                displayTodoProgress(ctx.ssid, taskEntries.map((t) => t.item));
              } else if (block.name === "TaskUpdate") {
                const entry = taskEntries.find(
                  (t) => t.id != null && t.id === String(input.taskId ?? ""),
                );
                if (entry) {
                  if (input.status === "deleted") {
                    taskEntries = taskEntries.filter((t) => t !== entry);
                  } else {
                    if (input.status != null) entry.item.status = input.status;
                    if (input.subject != null) entry.item.content = String(input.subject);
                    if (input.activeForm != null) {
                      entry.item.activeForm = String(input.activeForm);
                    }
                  }
                  displayTodoProgress(ctx.ssid, taskEntries.map((t) => t.item));
                }
              } else if (block.name === "TaskList" || block.name === "TaskGet") {
                // 任务簿记查询：进度已由上方增量推送呈现，不渲染调用/结果行
              } else {
                // cmd_exec MCP 工具的入参字段是 cmd（Bash 等内置工具是 command）；
                // 两者都没有时把完整参数透传为 description（前端渲染为 argsJson 折叠体）
                const rawCommand = input.cmd || input.command;
                const command =
                  typeof rawCommand === "string" && rawCommand ? rawCommand : undefined;
                emitToolCall(
                  ctx.ssid,
                  {
                    name: block.name,
                    command,
                    description: command
                      ? undefined
                      : input.description != null
                        ? String(input.description)
                        : JSON.stringify(input, null, 2),
                  },
                  blockId,
                );
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
    // tool_use_id 每轮 query 唯一，轮首清空防跨轮累积
    toolUseNames = new Map();
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
