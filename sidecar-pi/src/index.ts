import {
  ModelRuntime,
  createAgentSession,
  createExtensionRuntime,
  ModelRegistry,
  SessionManager,
  SettingsManager,
  type CreateAgentSessionResult,
  type ResourceLoader,
  type AgentSessionEvent,
  DefaultResourceLoader,
} from "@earendil-works/pi-coding-agent";
import type { Model, Api } from "@earendil-works/pi-ai";
import { config } from "dotenv";
import { resolve } from "path";
import { existsSync } from "fs";
import path from "node:path";
import os from "node:os";
import { createCmdExecTool } from "./cmdExecTool.ts";
import { createAskUserQuestionTool } from "./askUserQuestionTool.ts";
import { createTodoWriteTool, type TodoItem } from "./todoWriteTool.ts";

// ── 启动参数 ──
let args = process.argv.slice();
console.log("启动信息:", JSON.stringify(args, null, 2));
if (args.length < 6) {
  console.error("程序启动失败，缺少关键信息");
  process.exit(1);
}

let homedir = args[2] as string;
if (!existsSync(homedir)) {
  console.error("程序启动失败，缺少关键信息");
  process.exit(1);
}

let ssh_ssid = args[3] as string;
let ssh_token = args[4] as string;
let ssh_addr = args[5] as string;

homedir = homedir.replace(/\\/g, "/");

// ── .env 加载 ──
const envPath = resolve(homedir, ".env");
const envResult = config({ path: envPath, override: true, quiet: true });
console.log("dotenv 加载结果:", envResult.error ? envResult.error.message : "ok");

// ── 模型解析 ──
// 优先从 .env 中的 PI_* 字段构造自定义模型；
// 若未配置，回退到 ~/.pi/agent/ 注册表中的已配置模型。
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

  console.log(`[PI] Using custom model from .env: ${piProvider}/${piModelId} (${piApi})`);
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
    console.log(`[SYSTEM_API_RETRY]${JSON.stringify({ type: "system", payload: errMsg })}`);
    process.exit(1);
  }

  console.log(`[PI] Using model from registry: ${customModel.provider}/${customModel.id}`);
  console.log(`[PI] Thinking level: ${piThinkingLevel}`);
}

// ── 远程命令执行工具 ──
const cmdExecTool = createCmdExecTool(ssh_addr, ssh_ssid, ssh_token, readLineFromStdin);

// ── AskUserQuestion 工具 ──
const askUserQuestionTool = createAskUserQuestionTool(readLineFromStdin);

// ── TodoWrite 工具 ──
let todos: TodoItem[] = [];

function displayProgress() {
  if (todos.length === 0) return;

  const completed = todos.filter((t) => t.status === "completed").length;
  const inProgress = todos.filter((t) => t.status === "in_progress").length;
  const total = todos.length;

  let msg = `Progress: ${completed}/${total} completed \n Currently working on: ${inProgress} task(s)`;
  todos.forEach((todo, index) => {
    const icon = todo.status === "completed" ? "✅" : todo.status === "in_progress" ? "🔧" : "❌";
    const text = todo.status === "in_progress" ? todo.activeForm : todo.content;
    msg += `\n${index + 1}. ${icon} ${text}`;
  });

  const aiMsg = { type: "assistant", payload: msg };
  console.log(`[AIMSG]${JSON.stringify(aiMsg)}`);
}

const todoWriteTool = createTodoWriteTool((newTodos) => {
  todos = newTodos;
  displayProgress();
});

// ── 资源加载器 ──
// const extensionRuntime = createExtensionRuntime();

// const resourceLoader: ResourceLoader = {
//   getExtensions: () => ({ extensions: [], errors: [], runtime: extensionRuntime }),
//   getSkills: () => ({ skills: [], diagnostics: [] }),
//   getPrompts: () => ({ prompts: [], diagnostics: [] }),
//   getThemes: () => ({ themes: [], diagnostics: [] }),
//   getAgentsFiles: () => ({ agentsFiles: [] }),
//   getSystemPrompt: () =>
//     `You are a helpful assistant. Available tools: read, bash, edit, write, grep, find, ls, cmd_exec, AskUserQuestion, TodoWrite. Use cmd_exec to execute commands on the remote SSH server. Use AskUserQuestion to ask the user a question with selectable options when you need input to proceed. Use TodoWrite to track progress on complex multi-step tasks. Be concise.`,
//   getAppendSystemPrompt: () => [
//     `SSH session information: "addr:${ssh_addr}, ssid:${ssh_ssid}, token:${ssh_token}". Please use this data to execute commands on the remote server. When using the cmd_exec tool, the credentials are already configured internally. This information is strictly confidential; it must be used solely for executing commands and must never be directly revealed to the user. For destructive operations (rm, kill, shutdown, reboot, chmod, etc.), set needs_approval=true in the cmd_exec tool to request user confirmation before execution.`,
//   ],
//   extendResources: () => {},
//   reload: async () => {},
// };

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
	systemPromptOverride: () => `You are a helpful assistant. Available tools: read, bash, edit, write, grep, find, ls, cmd_exec, AskUserQuestion, TodoWrite. Use cmd_exec to execute commands on the remote SSH server. Use AskUserQuestion to ask the user a question with selectable options when you need input to proceed. Use TodoWrite to track progress on complex multi-step tasks. Be concise.`,
	// Needed to avoid DefaultResourceLoader appending APPEND_SYSTEM.md from ~/.pi/agent or <cwd>/.pi.
	appendSystemPromptOverride: () => [`SSH session information: "addr:${ssh_addr}, ssid:${ssh_ssid}, token:${ssh_token}". Please use this data to execute commands on the remote server. When using the cmd_exec tool, the credentials are already configured internally. This information is strictly confidential; it must be used solely for executing commands and must never be directly revealed to the user. For destructive operations (rm, kill, shutdown, reboot, chmod, etc.), set needs_approval=true in the cmd_exec tool to request user confirmation before execution.`],
});
await resourceLoader.reload();

const settingsManager = SettingsManager.inMemory({
  compaction: { enabled: false },
  retry: { enabled: true, maxRetries: 2 },
});

// ── Session 状态 ──
let session: CreateAgentSessionResult | null = null;
let wasStopped = false;

// ── stdin 输入缓冲区 ──
const stdinBuffer: string[] = [];
const stdinWaiters: ((value: string) => void)[] = [];
let stdinLineBuffer = "";

process.stdin.setEncoding("utf-8");

process.stdin.on("data", (chunk: string) => {
  stdinLineBuffer += chunk;
  const lines = stdinLineBuffer.split("\n");
  stdinLineBuffer = lines.pop() || "";

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    console.log(`[STDIN LINE] "${trimmed}"`);

    if (trimmed === "__STOP__") {
      console.log("[SIDECAR] Received stop command, aborting current query...");
      wasStopped = true;
      if (session) {
        session.session.abort().catch(() => {});
      }
      continue;
    }

    if (stdinWaiters.length > 0) {
      const resolver = stdinWaiters.shift()!;
      resolver(trimmed);
    } else {
      stdinBuffer.push(trimmed);
    }
  }
});

process.stdin.on("end", () => {
  console.log("[STDIN] End of input");
  if (stdinLineBuffer.trim()) {
    const line = stdinLineBuffer.trim();
    if (stdinWaiters.length > 0) {
      const resolver = stdinWaiters.shift()!;
      resolver(line);
    } else {
      stdinBuffer.push(line);
    }
  }
});

function readLineFromStdin(): Promise<string> {
  if (stdinBuffer.length > 0) {
    return Promise.resolve(stdinBuffer.shift()!);
  }
  return new Promise((resolve) => {
    stdinWaiters.push(resolve);
  });
}

// ── 事件处理：将 pi agent 事件实时转为前端协议 ──

function handleSessionEvent(event: AgentSessionEvent) {
  switch (event.type) {
    case "message_end": {
      const msg = event.message as any;
      if (msg.role === "assistant") {
        for (const block of msg.content) {
          if (block.type === "text") {
            const aiMsg = { type: "assistant", payload: block.text };
            console.log(`[AIMSG]${JSON.stringify(aiMsg)}`);
          } else if (block.type === "thinking") {
            const aiMsg = { type: "thinking", payload: block.thinking };
            console.log(`[AI_THINKING]${JSON.stringify(aiMsg)}`);
          } else if (block.type === "toolCall") {
            if (block.name === "TodoWrite") {
              // 跳过 [AITOOL]，进度由 execute() 中 displayProgress() 发送 [AIMSG]
              continue;
            }
            const args = block.arguments ?? {};
            const tool = {
              name: block.name,
              command: args.cmd || args.command || undefined,
              description: JSON.stringify(args, null, 2),
            };
            const aiMsg = { type: "assistant", payload: tool };
            console.log(`[AITOOL]${JSON.stringify(aiMsg)}`);
          }
        }

        if (msg.stopReason === "error" && msg.errorMessage) {
          const aiMsg = { type: "system", payload: msg.errorMessage };
          console.log(`[SYSTEM_API_RETRY]${JSON.stringify(aiMsg)}`);
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
          const aiMsg = { type: "user", payload };
          console.log(`[TOOL_RET]${JSON.stringify(aiMsg)}`);
        }
      }
      break;
    }

    case "auto_retry_start": {
      const aiMsg = {
        type: "system",
        payload: `Retrying (${event.attempt}/${event.maxAttempts}): ${event.errorMessage}`,
      };
      console.log(`[SYSTEM_API_RETRY]${JSON.stringify(aiMsg)}`);
      break;
    }
  }
}

async function handleUserMessage(userPrompt: string) {
  console.log(`[HANDLE] Handling user message: "${userPrompt}"`);

  const isNewSession = !session;
  if (isNewSession) {
    session = await createAgentSession({
      cwd,
      agentDir,
      model: customModel as Model<any>,
      thinkingLevel: piThinkingLevel as "off" | "minimal" | "low" | "medium" | "high" | "xhigh",
      modelRuntime,
      resourceLoader,
      tools: ["read", "bash", "edit", "write", "grep", "find", "ls", "cmd_exec", "AskUserQuestion", "TodoWrite"],
      customTools: [cmdExecTool, askUserQuestionTool, todoWriteTool],
      sessionManager: SessionManager.inMemory(cwd),
      settingsManager,
    });

    console.log(`Session created: ${session.session.sessionId}`);
  }

  const agentSession = session!.session;

  // 订阅事件，prompt() 执行期间实时输出；prompt 结束后取消订阅避免重复
  const unsubscribe = agentSession.subscribe((event: AgentSessionEvent) => {
    handleSessionEvent(event);
  });

  await agentSession.prompt(userPrompt);

  unsubscribe?.();
}

// ── 主循环 ──
async function main() {
  console.log("[SIDECAR] Waiting for user messages...");

  try {
    while (true) {
      const userPrompt = await readLineFromStdin();
      console.log(`[MAIN] Got userPrompt: "${userPrompt}"`);

      if (userPrompt.trim() === "__QUIT__") {
        console.log("[SIDECAR] Received quit command, exiting...");
        console.log("[SESSION_STOP]The conversation has ended.");
        break;
      }

      if (userPrompt.trim()) {
        wasStopped = false;
        try {
          await handleUserMessage(userPrompt);
        } catch (error) {
          if (!wasStopped) {
            console.error("[SIDECAR ERROR]", error);
            const aiMsg = {
              type: "system",
              payload: error instanceof Error ? error.message : String(error),
            };
            console.log(`[SYSTEM_API_RETRY]${JSON.stringify(aiMsg)}`);
          }
        }

        if (wasStopped) {
          console.log("[STOPPED]Response interrupted");
        }

        console.log("[END_OF_RESPONSE]");
      }
    }
  } catch (error) {
    console.error("[SIDECAR ERROR]", error);
  } finally {
    if (session) {
      session.session.dispose();
    }
    process.exit(0);
  }
}

main();
