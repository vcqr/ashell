import {
  ModelRuntime,
  createAgentSession,
  ModelRegistry,
  SessionManager,
  SettingsManager,
  DefaultResourceLoader,
  type CreateAgentSessionResult,
  type AgentSessionEvent,
} from "@earendil-works/pi-coding-agent";
import type { Model, Api } from "@earendil-works/pi-ai";
import path from "node:path";
import os from "node:os";
import {
  emitAIMSG,
  emitCompact,
  emitSystemError,
  emitThinking,
  emitToolCall,
  emitToolRet,
  emitUsage,
} from "../../protocol";
import { displayTodoProgress, type TodoItem } from "../../progress";
import { createCmdExecTool } from "./cmdExecTool";
import { createAskUserQuestionTool } from "./askUserQuestionTool";
import { createTodoWriteTool } from "./todoWriteTool";
import type { EngineAdapter, EngineContext } from "../types";

const AGENT_TOOLS = [
  "read",
  "bash",
  "edit",
  "write",
  "grep",
  "find",
  "ls",
  "cmd_exec",
  "AskUserQuestion",
  "TodoWrite",
];

/**
 * Pi coding agent 引擎适配器（移植自 sidecar-pi）。
 *
 * - 模型解析：优先 .env 的 PI_* 字段构造自定义模型，回退 ~/.pi/agent 注册表
 * - 会话懒创建：首条消息时 createAgentSession，后续复用
 * - 事件流实时转译为前端协议（message_end / turn_end / auto_retry_start）
 */
export async function createPiEngine(ctx: EngineContext): Promise<EngineAdapter> {
  const piProvider = process.env.PI_PROVIDER || "";
  const piModelId = process.env.PI_MODEL || "";
  const piBaseUrl = process.env.PI_BASE_URL || "";
  const piApiKey = process.env.PI_API_KEY || "";
  const piApi = process.env.PI_API || "openai-completions";
  const piThinkingLevel = process.env.PI_THINKING_LEVEL || "off";
  // 上下文控制（宿主设置物化到 .env，两引擎通用）：窗口/输出上限覆盖模型默认，轮数限 0/空 = 不限
  const aiContextWindow = Number.parseInt(process.env.AI_CONTEXT_WINDOW || "", 10);
  const aiMaxTokens = Number.parseInt(process.env.AI_MAX_TOKENS || "", 10);
  const aiMaxRounds = Number.parseInt(process.env.AI_MAX_ROUNDS || "", 10);
  const ctxWindowOverride =
    Number.isFinite(aiContextWindow) && aiContextWindow > 0 ? aiContextWindow : undefined;
  const maxTokensOverride =
    Number.isFinite(aiMaxTokens) && aiMaxTokens > 0 ? aiMaxTokens : undefined;
  const maxRounds =
    Number.isFinite(aiMaxRounds) && aiMaxRounds > 0 ? aiMaxRounds : undefined;

  const agentDir = path.join(os.homedir(), ".pi", "agent");
  const modelRuntime = await ModelRuntime.create({
    authPath: path.join(agentDir, "auth.json"),
    modelsPath: path.join(agentDir, "models.json"),
  });
  const customModelRegistry = new ModelRegistry(modelRuntime);

  const cwd = process.cwd();

  let customModel: Model<Api> | null = null;

  if (piProvider && piModelId && piBaseUrl) {
    // 从 .env 构造自定义模型
    customModel = {
      id: piModelId,
      name: piModelId,
      api: piApi as Api,
      provider: piProvider,
      baseUrl: piBaseUrl,
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: ctxWindowOverride ?? 128000,
      maxTokens: maxTokensOverride ?? 16384,
    };

    // 注册自定义 provider，使 ModelRuntime.getAuth() 能解析该 provider 的 API key
    modelRuntime.registerProvider(piProvider, {
      name: piProvider,
      baseUrl: piBaseUrl,
      api: piApi as Api,
      authHeader: true,
    });

    // 设置运行时 API key（不持久化到 auth.json）
    if (piApiKey) {
      await modelRuntime.setRuntimeApiKey(piProvider, piApiKey);
    }

    console.log(
      `[PI] Using custom model from .env: ${piProvider}/${piModelId} (${piApi})`,
    );
    console.log(`[PI] Thinking level: ${piThinkingLevel}`);
  } else {
    // 回退到 ~/.pi/agent/ 注册表
    if (piProvider && piModelId) {
      customModel = customModelRegistry.find(piProvider, piModelId) ?? null;
    }

    if (!customModel) {
      const available = customModelRegistry.getAvailable();
      if (available && available.length > 0) {
        customModel = available[0]!;
      } else {
        const all = customModelRegistry.getAll();
        if (all && all.length > 0) {
          customModel = all[0]!;
        }
      }
    }

    if (!customModel) {
      // daemon 模式下进程被多会话共享，绝不允许 process.exit；
      // 抛错由 create 流程捕获，经 create 帧 err 应答上报宿主
      throw new Error(
        "No pi model configured. Please set PI_PROVIDER, PI_MODEL, PI_BASE_URL, PI_API_KEY, PI_API in ~/.ashell/ai/.env, or configure ~/.pi/agent/models.json.",
      );
    }

    // .env 显式配置的窗口/输出上限优先于模型注册表值（克隆覆盖，不动注册表对象）
    if (ctxWindowOverride || maxTokensOverride) {
      customModel = {
        ...customModel,
        ...(ctxWindowOverride && { contextWindow: ctxWindowOverride }),
        ...(maxTokensOverride && { maxTokens: maxTokensOverride }),
      } as Model<Api>;
    }

    console.log(`[PI] Using model from registry: ${customModel.provider}/${customModel.id}`);
    console.log(
      `[PI] Context: window=${customModel.contextWindow} maxTokens=${customModel.maxTokens} maxRounds=${maxRounds ?? "unlimited"}`,
    );
    console.log(`[PI] Thinking level: ${piThinkingLevel}`);
  }

  // ── 工具 ──
  let todos: TodoItem[] = [];

  const cmdExecTool = createCmdExecTool(ctx, () => ctx.io.readLineOrStop());
  const askUserQuestionTool = createAskUserQuestionTool(ctx.io, ctx.ssid);
  const todoWriteTool = createTodoWriteTool((newTodos) => {
    todos = newTodos;
    displayTodoProgress(ctx.ssid, todos);
  });

  // ── 资源加载器 ──
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir,
    extensionsOverride: (current) => ({
      extensions: [...current.extensions],
      errors: [...current.errors],
      runtime: current.runtime,
    }),
    promptsOverride: (current) => ({
      prompts: [...current.prompts],
      diagnostics: current.diagnostics,
    }),
    agentsFilesOverride: (current) => ({
      agentsFiles: [...current.agentsFiles],
    }),
    skillsOverride: (current) => ({
      skills: [...current.skills],
      diagnostics: current.diagnostics,
    }),
    systemPromptOverride: () =>
      `You are a helpful assistant. Available tools: read, bash, edit, write, grep, find, ls, cmd_exec, AskUserQuestion, TodoWrite. Use cmd_exec to execute commands on the remote SSH server. Use AskUserQuestion to ask the user a question with selectable options when you need input to proceed. Use TodoWrite to track progress on complex multi-step tasks. Be concise.`,
    // 避免 DefaultResourceLoader 追加 ~/.pi/agent 或 <cwd>/.pi 的 APPEND_SYSTEM.md。
    // 凭证经 cmd_exec 工具闭包注入，绝不写入提示词（提示词会进会话历史/落盘）；
    // 此处只约束工具的使用方式与破坏性操作审批
    appendSystemPromptOverride: () => [
      `The cmd_exec tool executes commands on the remote SSH server the user is connected to; its connection credentials are already configured internally and must not be requested or guessed. For destructive operations (rm, kill, shutdown, reboot, chmod, etc.), set needs_approval=true in the cmd_exec tool to request user confirmation before execution.`,
    ],
  });
  await resourceLoader.reload();

  // 自动压缩开启：上下文逼近窗口（预留 reserveTokens）时自动摘要压缩，
  // 溢出时压缩后重试；reserveTokens/keepRecentTokens 用 SDK 默认值
  const settingsManager = SettingsManager.inMemory({
    compaction: { enabled: true },
    retry: { enabled: true, maxRetries: 2 },
  });

  // ── Session 状态 ──
  let session: CreateAgentSessionResult | null = null;
  let stopped = false;
  // 本轮（一次 prompt 内所有 LLM 调用）的用量累加，prompt 结束时上报
  let turnUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0 };

  function handleSessionEvent(event: AgentSessionEvent) {
    switch (event.type) {
      case "message_end": {
        const msg = event.message as any;
        if (msg.role === "assistant") {
          for (const block of msg.content) {
            if (block.type === "text") {
              emitAIMSG(ctx.ssid, block.text);
            } else if (block.type === "thinking") {
              emitThinking(ctx.ssid, block.thinking);
            } else if (block.type === "toolCall") {
              // TodoWrite 的进度由 todoWriteTool 的 onUpdate 推送，跳过工具调用展示
              if (block.name === "TodoWrite") continue;
              const args = block.arguments ?? {};
              emitToolCall(ctx.ssid, {
                name: block.name,
                command: args.cmd || args.command || undefined,
                description: JSON.stringify(args, null, 2),
              });
            }
          }

          // 累加本轮每次 LLM 调用的用量（一次 prompt 可能多轮工具往返）
          const u = msg.usage;
          if (u) {
            turnUsage.input += u.input ?? 0;
            turnUsage.output += u.output ?? 0;
            turnUsage.cacheRead += u.cacheRead ?? 0;
            turnUsage.cacheWrite += u.cacheWrite ?? 0;
            turnUsage.cost += u.cost?.total ?? 0;
          }

          if (msg.stopReason === "error" && msg.errorMessage) {
            emitSystemError(ctx.ssid, msg.errorMessage);
          }
        }
        break;
      }

      case "compaction_end": {
        // 自动/手动压缩完成：上报压缩事件供前端提示
        if (event.aborted) break;
        if (event.errorMessage) {
          emitSystemError(ctx.ssid, `上下文压缩失败: ${event.errorMessage}`);
        } else if (event.result) {
          emitCompact(ctx.ssid, {
            trigger: event.reason === "manual" ? "manual" : "auto",
            tokensBefore: event.result.tokensBefore,
            tokensAfter: event.result.estimatedTokensAfter,
          });
        }
        break;
      }

      case "turn_end": {
        // turn_end 携带本轮工具执行结果，实时输出
        const toolResults = (event as any).toolResults as any[] | undefined;
        if (toolResults) {
          for (const tr of toolResults) {
            const payload = (tr.content || []).map((c: any) => ({
              type: c.type,
              content: c.type === "text" ? c.text : "",
            }));
            emitToolRet(ctx.ssid, payload);
          }
        }
        break;
      }

      case "auto_retry_start": {
        emitSystemError(
          ctx.ssid,
          `Retrying (${event.attempt}/${event.maxAttempts}): ${event.errorMessage}`,
        );
        break;
      }
    }
  }

  async function handle(userPrompt: string): Promise<void> {
    console.log(`[HANDLE] Handling user message: "${userPrompt}"`);
    stopped = false;
    turnUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0 };

    if (!session) {
      session = await createAgentSession({
        cwd,
        agentDir,
        model: customModel as Model<any>,
        thinkingLevel: piThinkingLevel as
          | "off"
          | "minimal"
          | "low"
          | "medium"
          | "high"
          | "xhigh",
        modelRuntime,
        resourceLoader,
        tools: AGENT_TOOLS,
        customTools: [cmdExecTool, askUserQuestionTool, todoWriteTool],
        sessionManager: SessionManager.inMemory(cwd),
        settingsManager,
      });
      console.log(`Session created: ${session.session.sessionId}`);

      // 轮数滑窗：每次 LLM 调用前只发送最近 maxRounds 轮 + 首条用户消息之前的
      // 系统基线；在用户消息边界裁剪，保证轮内 toolCall/toolResult 成对完整。
      // 完整历史仍保留在会话里，只是不进入模型上下文。
      if (maxRounds) {
        session.session.agent.transformContext = async (messages) => {
          const firstUser = messages.findIndex((m) => (m as { role?: string }).role === "user");
          if (firstUser < 0) return messages;
          const roundStarts: number[] = [];
          for (let i = firstUser; i < messages.length; i++) {
            if ((messages[i] as { role?: string }).role === "user") roundStarts.push(i);
          }
          if (roundStarts.length <= maxRounds) return messages;
          const cut = roundStarts[roundStarts.length - maxRounds]!;
          return [...messages.slice(0, firstUser), ...messages.slice(cut)];
        };
      }
    }

    // 订阅事件，prompt() 执行期间实时输出；结束后取消订阅避免重复
    const unsubscribe = session.session.subscribe((event: AgentSessionEvent) => {
      handleSessionEvent(event);
    });

    try {
      await session.session.prompt(userPrompt);
    } finally {
      unsubscribe?.();
    }

    // 轮末上报：用量为累加值；上下文占用取会话统计（估算，压缩后到下次响应前可能为 null）
    const cu = session.session.getSessionStats().contextUsage;
    emitUsage(ctx.ssid, {
      input: turnUsage.input,
      output: turnUsage.output,
      cacheRead: turnUsage.cacheRead,
      cacheWrite: turnUsage.cacheWrite,
      cost: turnUsage.cost > 0 ? turnUsage.cost : undefined,
      contextTokens: cu?.tokens ?? undefined,
      contextWindow: cu?.contextWindow,
      contextPercent: cu?.percent != null ? Math.round(cu.percent) : undefined,
    });
  }

  return {
    name: "pi",
    get stopped() {
      return stopped;
    },
    stop() {
      stopped = true;
      // 等待中的审批/提问由 StdinIO.signalStop 释放
      session?.session.abort().catch(() => {});
    },
    handle,
    dispose() {
      session?.session.dispose();
    },
  };
}
