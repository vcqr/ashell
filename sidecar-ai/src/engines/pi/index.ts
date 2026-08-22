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
  emitSystemError,
  emitThinking,
  emitToolCall,
  emitToolRet,
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
      contextWindow: 128000,
      maxTokens: 16384,
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
      const errMsg =
        "No pi model configured. Please set PI_PROVIDER, PI_MODEL, PI_BASE_URL, PI_API_KEY, PI_API in ~/.ashell/ai/.env, or configure ~/.pi/agent/models.json.";
      emitSystemError(errMsg);
      process.exit(1);
    }

    console.log(`[PI] Using model from registry: ${customModel.provider}/${customModel.id}`);
    console.log(`[PI] Thinking level: ${piThinkingLevel}`);
  }

  // ── 工具 ──
  let todos: TodoItem[] = [];

  const cmdExecTool = createCmdExecTool(ctx, () => ctx.io.readLineOrStop());
  const askUserQuestionTool = createAskUserQuestionTool(ctx.io);
  const todoWriteTool = createTodoWriteTool((newTodos) => {
    todos = newTodos;
    displayTodoProgress(todos);
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
    // 避免 DefaultResourceLoader 追加 ~/.pi/agent 或 <cwd>/.pi 的 APPEND_SYSTEM.md
    appendSystemPromptOverride: () => [
      `SSH session information: "addr:${ctx.addr}, ssid:${ctx.ssid}, token:${ctx.token}". Please use this data to execute commands on the remote server. When using the cmd_exec tool, the credentials are already configured internally. This information is strictly confidential; it must be used solely for executing commands and must never be directly revealed to the user. For destructive operations (rm, kill, shutdown, reboot, chmod, etc.), set needs_approval=true in the cmd_exec tool to request user confirmation before execution.`,
    ],
  });
  await resourceLoader.reload();

  const settingsManager = SettingsManager.inMemory({
    compaction: { enabled: false },
    retry: { enabled: true, maxRetries: 2 },
  });

  // ── Session 状态 ──
  let session: CreateAgentSessionResult | null = null;
  let stopped = false;

  function handleSessionEvent(event: AgentSessionEvent) {
    switch (event.type) {
      case "message_end": {
        const msg = event.message as any;
        if (msg.role === "assistant") {
          for (const block of msg.content) {
            if (block.type === "text") {
              emitAIMSG(block.text);
            } else if (block.type === "thinking") {
              emitThinking(block.thinking);
            } else if (block.type === "toolCall") {
              // TodoWrite 的进度由 todoWriteTool 的 onUpdate 推送，跳过工具调用展示
              if (block.name === "TodoWrite") continue;
              const args = block.arguments ?? {};
              emitToolCall({
                name: block.name,
                command: args.cmd || args.command || undefined,
                description: JSON.stringify(args, null, 2),
              });
            }
          }

          if (msg.stopReason === "error" && msg.errorMessage) {
            emitSystemError(msg.errorMessage);
          }
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
            emitToolRet(payload);
          }
        }
        break;
      }

      case "auto_retry_start": {
        emitSystemError(
          `Retrying (${event.attempt}/${event.maxAttempts}): ${event.errorMessage}`,
        );
        break;
      }
    }
  }

  async function handle(userPrompt: string): Promise<void> {
    console.log(`[HANDLE] Handling user message: "${userPrompt}"`);
    stopped = false;

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
