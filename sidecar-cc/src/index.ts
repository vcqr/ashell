import { config } from "dotenv";
import { resolve, join } from "path";
import { platform } from "os"
import { query, type CanUseTool } from "@anthropic-ai/claude-agent-sdk";
import { existsSync } from "fs";
import * as readline from "readline";
import type { AIMsg, AITool, AskUserQuestion, AIConfirm } from "@/msgTypes"
import cmdExecServer from "./cmdMcpTool";

let args = process.argv.slice();
console.log("启动信息:", JSON.stringify(args, null, 2));
if (args.length < 3) {
  console.error("程序启动失败，缺少关键信息");
  process.exit(1);
}

let homedir = args[2] as string;
if (!existsSync(homedir)) {
  console.log('程序启动失败，缺少关键信息');
  process.exit(1);
}

let ssh_ssid = args[3] as string;
let ssh_token = args[4] as string;
let ssh_addr = args[5] as string;

// 转换为通用路径：Windows 反斜杠在 .env / claude SDK 传入时易被当作转义字符，统一成正斜杠
homedir = homedir.replace(/\\/g, '/');

const envPath = resolve(homedir, ".env");
console.log("=== 测试 handleUserMessage ===");
console.log("当前目录:", process.cwd());
console.log(".env 路径:", envPath);

// 先删除已有的环境变量，确保 dotenv 可以正确加载
delete process.env.ANTHROPIC_API_KEY;
delete process.env.ANTHROPIC_BASE_URL;
delete process.env.ANTHROPIC_MODEL;
delete process.env.ANTHROPIC_AUTH_TOKEN;


// 加载 .env 文件（使用当前工作目录，override 强制覆盖）
const result = config({ path: envPath, override: true, quiet: true, debug: false });
console.log("dotenv 加载结果:", result);
console.log("dotenv parsed:", result.parsed);

// console.log("=== 环境变量配置 ===");
// console.log("ANTHROPIC_API_KEY:", process.env.ANTHROPIC_API_KEY);
// console.log("ANTHROPIC_MODEL:", process.env.ANTHROPIC_MODEL);
// console.log("ANTHROPIC_BASE_URL:", process.env.ANTHROPIC_BASE_URL);
// console.log("ANTHROPIC_AUTH_TOKEN:", process.env.ANTHROPIC_AUTH_TOKEN);

let sessionId: string | undefined;
let todos: any[] = [];


let cliPath: string = process.env.CLAUDE_CLI_PATH || join(homedir, "bin", platform() === "win32" ? "claude.exe" : "claude");
// 统一成正斜杠，避免反斜杠在 spawn claude CLI 时被误解析
cliPath = cliPath.replace(/\\/g, '/');
console.log("claude path", cliPath);


// stdin 输入缓冲区
const stdinBuffer: string[] = [];
const stdinWaiters: ((value: string) => void)[] = [];
let stdinLineBuffer = "";

// ── 停止信号机制 ──
// currentQuery 保存当前正在执行的 Query 对象，requestStop() 调用其 interrupt() 方法中断执行
// stopPromise 用于解锁 canUseTool 中 readLineOrStop 的等待
let currentQuery: ReturnType<typeof query> | null = null;
let stopResolve: (() => void) | null = null;
let stopPromise: Promise<void> = new Promise(() => {});

function initStopSignal() {
  stopPromise = new Promise<void>((resolve) => {
    stopResolve = resolve;
  });
}

function requestStop() {
  if (currentQuery) {
    // interrupt() 内部清理时会 reject "Query closed before response received"
    // 这是 SDK 的预期行为，静默处理
    currentQuery.interrupt().catch(() => {});
  }
  if (stopResolve) {
    stopResolve();
    stopResolve = null;
  }
}

// 包装 readLineFromStdin，停止信号到达时返回 "__STOPPED__"
// stop 胜出时主动从 stdinWaiters 移除 resolver，避免残留 resolver 吞掉下一条用户消息
async function readLineOrStop(): Promise<string> {
  if (stdinBuffer.length > 0) {
    return stdinBuffer.shift()!;
  }

  return new Promise<string>((resolve) => {
    stdinWaiters.push(resolve);

    stopPromise.then(() => {
      const idx = stdinWaiters.indexOf(resolve);
      if (idx >= 0) stdinWaiters.splice(idx, 1);
      resolve("__STOPPED__");
    });
  });
}

process.stdin.setEncoding("utf-8");

process.stdin.on("data", (chunk: string) => {
  console.log(`[STDIN RAW] Received chunk: ${chunk} (length: ${chunk.length})`);

  stdinLineBuffer += chunk;
  const lines = stdinLineBuffer.split("\n");
  stdinLineBuffer = lines.pop() || "";

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    console.log(`[STDIN LINE] "${trimmed}"`);

    // 检测停止命令，触发停止信号中断当前 query
    if (trimmed === "__STOP__") {
      console.log("[SIDECAR] Received stop command, aborting current query...");
      requestStop();
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
    console.log(`[STDIN] Final line: "${line}"`);
    if (stdinWaiters.length > 0) {
      const resolver = stdinWaiters.shift()!;
      resolver(line);
    } else {
      stdinBuffer.push(line);
    }
  }
});

function readLineFromStdin(): Promise<string> {
  return new Promise((resolve) => {
    console.log("[DEBUG] readLineFromStdin called, buffer:", stdinBuffer.length);
    if (stdinBuffer.length > 0) {
      const line = stdinBuffer.shift()!;
      console.log(`[DEBUG] readLineFromStdin returning from buffer: "${line}"`);
      resolve(line);
    } else {
      console.log("[DEBUG] readLineFromStdin waiting for input...");
      stdinWaiters.push(resolve);
    }
  });
}

// 使用顶层 rl 接口进行 prompt（避免 stdin 冲突）
async function prompt(question: string, toolName?: string, toolInput?: any): Promise<string> {
  console.log(`[DEBUG] prompt called with: "${question}"`);

  // 输出特殊标记让前端解析
  let aiConfirm: AIConfirm = { question: question, options: toolInput };
  let aiMsg: AIMsg = { type: "tool_confirm", payload: aiConfirm };
  if (toolName) {
    const description = toolInput?.command || JSON.stringify(toolInput, null, 2);
    console.log(`[TOOL_CONFIRM_DEBUG]|${toolName}|${description}|${question}`);
  } else {
    console.log(question);
  }

  console.log(`[TOOL_CONFIRM]${JSON.stringify(aiMsg)}`);

  return readLineOrStop();
}

// 定义 canUseTool 处理函数
const canUseTool: CanUseTool = async (toolName, input) => {
  if (toolName === "AskUserQuestion") {
    const answers: Record<string, string> = {};

    let aiMsg: AIMsg = { type: "AskUserQuestion" };
    for (const q of input.questions as any[]) {
      console.log(`${q.header}: ${q.question}`);
      let askQ: AskUserQuestion = {
        title: `${q.header}: ${q.question}`
      }

      const options = q.options as any[];
      let items: string[] = [];
      options.forEach((opt: any, i: number) => {
        console.log(`  ${i + 1}. ${opt.label} - ${opt.description}`);
        items.push(`${i + 1}. ${opt.label} - ${opt.description}`);
      });

      askQ.items = items;

      if (q.multiSelect) {
        console.log("  (Enter numbers separated by commas, or type your own answer)");
        askQ.tips = "(Enter numbers separated by commas, or type your own answer)"
      } else {
        console.log("  (Enter a number, or type your own answer)");
        askQ.tips = "(Enter a number, or type your own answer)";
      }

      aiMsg.payload = askQ;

      // 问题给到用户进行选择
      console.log(`[AI_ASKUSERQUESTION]${JSON.stringify(aiMsg)}`);

      // 等待用户反馈
      const response = (await readLineOrStop()).trim();
      if (response === "__STOPPED__") {
        return { behavior: "deny", message: "用户已中断" };
      }
      const indices = response.split(",").map((s) => parseInt(s.trim()) - 1);
      const labels = indices
        .filter((i) => !isNaN(i) && i >= 0 && i < options.length)
        .map((i) => options[i].label);
      answers[q.question] = labels.length > 0 ? labels.join(", ") : response;

    }

    return {
      behavior: "allow",
      updatedInput: { questions: input.questions, answers },
    };
  }

  // if (toolName !== "") {
  //   console.log("[TOOL] Tool:", JSON.stringify(input, null, 2));
  // }

  const response = await prompt("Allow this action? (y/n): ", toolName, input);
  console.log(`[USER_RESPONSE] ${response}`);

  if (response === "__STOPPED__") {
    return { behavior: "deny", message: "用户已中断" };
  }

  let action = response.toLowerCase().trim();
  if (action === "y") {
    return { behavior: "allow", updatedInput: input };
  } else {
    return { behavior: "deny", message: "User denied this action" };
  }
};

// 处理 todos进度
async function displayProgress() {
  if (todos.length === 0) return;

  const completed = todos.filter((t) => t.status === "completed").length;
  const inProgress = todos.filter((t) => t.status === "in_progress").length;
  const total = todos.length;

  let aiMsg: AIMsg = { type: "assistant" };
  let msg = `Progress: ${completed}/${total} completed \n Currently working on: ${inProgress} task(s)`
  todos.forEach((todo, index) => {
    const icon = todo.status === "completed" ? "✅" : todo.status === "in_progress" ? "🔧" : "❌";
    const text = todo.status === "in_progress" ? todo.activeForm : todo.content;
    msg += `\n${index + 1}. ${icon} ${text}`;
  });

  aiMsg.payload = msg;
  console.log(`[AIMSG]${JSON.stringify(aiMsg)}`);
}

// 处理单个用户消息
async function handleUserMessage(userPrompt: string) {
  console.log(`[HANDLE] Starting handleUserMessage with: "${userPrompt}"`);
  console.log(`[HANDLE] sessionId: ${sessionId}, cliPath: ${cliPath}`);

  initStopSignal();
  let stopped = false;
  stopPromise.then(() => { stopped = true; });

  try {
    console.log(`[HANDLE] Calling query()...`);

    const appendPrompt = `SSH session information: "addr:${ssh_addr}, ssid:${ssh_ssid}, token:${ssh_token}".  Please use this data to execute commands on the remote server. When using the mcp__cmd_exec tool, populate it with these credentials. This information is strictly confidential; it must be used solely for executing commands and must never be directly revealed to the user. Note: For commands involving destructive operations, please request user approval before execution.`;

    const queryIterator = query({
      prompt: userPrompt,
      options: {
        cwd: homedir,
        ...(sessionId && { resume: sessionId }),
        permissionMode: "acceptEdits",
        mcpServers: { cmd_exec: cmdExecServer },
        allowedTools: ["Skill", "Write", "TodoWrite", "Read", "WebFetch", "mcp__cmd_exec"],
        tools: ["Read", "Glob", "Grep", "Edit", "Bash", "AskUserQuestion", "Skill", "Write", "TodoWrite",  "WebFetch"],
        //includePartialMessages: true,
        pathToClaudeCodeExecutable: cliPath,
        settingSources: ["project", "local"],
        canUseTool,
        hooks: {},
        systemPrompt: {
          type: "preset",
          preset: "claude_code",
          append: appendPrompt
        }
      },
    });
    console.log(`[HANDLE] query() returned iterator, starting iteration...`);

    currentQuery = queryIterator;

    for await (const message of queryIterator) {
      console.log(`[MSG]type: ${message.type}`);
      // if (message.type !== "stream_event") {
      //   console.log(`[MSG]type: ${message.type}`);
      // }

      if (message.type === "system") {
        console.log("[system]:", JSON.stringify(message, null, 2));
        if (message.subtype === "init") {
          //console.log(JSON.stringify(message, null, 4))
          sessionId = message.session_id;
          console.log(`SESSION_ID:${sessionId}`);
          // You can save this ID for later resumption
        }

        // 处理错误信息
        if (message.subtype === "api_retry") {
          let aiMsg: AIMsg = { type: "system" };
          aiMsg.payload = message.error;
          console.log(`[SYSTEM_API_RETRY]${JSON.stringify(aiMsg)}`);
        }

      }

      if (message.type === "user") {
        console.log("[user]:", JSON.stringify(message, null, 2));
        let aiMsg: AIMsg = { type: "user" };
        if (message.message?.content && message.tool_use_result) {
          aiMsg.payload = message.message?.content;
          console.log(`[TOOL_RET]${JSON.stringify(aiMsg)}`);
        }
      }

      if (message.type === "assistant" && message.message?.content) {
        for (const block of message.message.content) {
          let aiMsg: AIMsg = { type: "assistant" };
          if ("text" in block) {
            aiMsg.payload = block.text;
            console.log(`[AIMSG]${JSON.stringify(aiMsg)}`);
          } else if ("name" in block) {
            const input = (block.input ?? {}) as Record<string, any>;
            if (block.name === "TodoWrite") {
              todos = Array.isArray(input.todos) ? input.todos : [];
              displayProgress();
            } else {
              // 处理工具调用
              let tool: AITool = {
                name: block.name,
                command: input.command,
                description: input.description,
              };
              aiMsg.payload = tool;
              console.log(`[AITOOL]${JSON.stringify(aiMsg)}`);
            }
          }
        }
      } else if (message.type === "stream_event") {
        // const event = message.event;
        // if (event.type === "content_block_delta") {
        //   if (event.delta.type === "text_delta") {
        //     console.log(`[STREAM_EVENT][EVENT]${event.delta.text}`);
        //   }
        // }
      } else if (message.type === "result") {
        console.log(`[DONE:${message.subtype}]`);
      }
    }
    console.log(`[HANDLE] handleUserMessage completed, stopped=${stopped}`);
  } catch (error) {
    if (stopped) {
      console.log("[HANDLE] Error suppressed (caused by interrupt)");
    } else {
      console.error(`[HANDLE] Error:`, error);
      const errMsg = error instanceof Error ? error.message : String(error);
      let aiMsg: AIMsg = { type: "system" };
      aiMsg.payload = errMsg;
      console.log(`[SYSTEM_API_RETRY]${JSON.stringify(aiMsg)}`);
    }
  } finally {
    currentQuery = null;
    stopResolve = null;
  }

  if (stopped) {
    console.log("[HANDLE] Query interrupted by user");
    let aiMsg: AIMsg = { type: "system" };
    aiMsg.payload = "用户已中断本次响应";
    console.log(`[STOPPED]${JSON.stringify(aiMsg)}`);
  }
}

// 主循环：从 stdin 持续读取用户消息
async function main() {
  console.log("[SIDECAR] Waiting for user messages...");

  try {
    while (true) {
      console.log("[MAIN] Calling readLineFromStdin()...");
      // 等待用户输入
      const userPrompt = await readLineFromStdin();
      console.log(`[MAIN] Got userPrompt: "${userPrompt}"`);

      // 检查是否为退出命令
      if (userPrompt.trim() === "__QUIT__") {
        console.log("[SIDECAR] Received quit command, exiting...");
        console.log("[SESSION_STOP]The conversation has ended.");
        break;
      }

      if (userPrompt.trim()) {
        console.log(`[MAIN] Calling handleUserMessage with: "${userPrompt}"`);
        // 处理用户消息
        await handleUserMessage(userPrompt);
        // 发送结束标记
        console.log("[END_OF_RESPONSE]");
      }
    }
  } catch (error) {
    console.error("[SIDECAR ERROR]", error);
  } finally {
    process.exit(0);
  }
}

// 启动主程序
main();
