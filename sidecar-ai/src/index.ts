/**
 * AShell AI sidecar（统一二进制）。
 *
 * 内嵌多个 Agent 引擎（claude / pi），启动时按类型选择：
 *   app-ai <workspace> <ssid> <token> <addr> [engine]
 * engine 缺省时读 ~/.ashell/ai/.env 的 SIDECAR_TYPE，仍缺省为 claude。
 *
 * 与宿主的通信全部走 stdin/stdout 行协议：
 * - stdin ：用户消息一行一条；"__STOP__" 中断当前轮次；"__QUIT__" 结束会话
 * - stdout：[TAG]{json} 前缀协议（见 protocol.ts），其余行为调试日志
 */
import { config } from "dotenv";
import { resolve } from "path";
import { existsSync } from "fs";
import { StdinIO } from "./stdin";
import { createEngine } from "./engines/factory";
import {
  END_OF_RESPONSE,
  SESSION_STOP,
  emitSystemError,
  emitStopped,
} from "./protocol";

const args = process.argv.slice();
console.log("启动信息:", JSON.stringify(args, null, 2));
if (args.length < 6) {
  console.error("程序启动失败，缺少关键信息");
  process.exit(1);
}

let homedir = String(args[2]);
if (!existsSync(homedir)) {
  console.error("程序启动失败，缺少关键信息");
  process.exit(1);
}
// Windows 反斜杠在 .env / SDK 路径传入时易被当作转义字符，统一成正斜杠
homedir = homedir.replace(/\\/g, "/");

const sshSsid = String(args[3]);
const sshToken = String(args[4]);
const sshAddr = String(args[5]);

// 先清掉继承的 anthropic 变量，确保 dotenv 加载的工作区配置生效
delete process.env.ANTHROPIC_API_KEY;
delete process.env.ANTHROPIC_BASE_URL;
delete process.env.ANTHROPIC_MODEL;
delete process.env.ANTHROPIC_AUTH_TOKEN;

config({ path: resolve(homedir, ".env"), override: true, quiet: true });

// 引擎选择优先级：启动参数 > .env SIDECAR_TYPE > claude
const engineType =
  (typeof args[6] === "string" && args[6].trim()) ||
  process.env.SIDECAR_TYPE?.trim() ||
  "claude";

async function main(): Promise<void> {
  const io = new StdinIO();
  const engine = await createEngine(engineType, {
    homedir,
    ssid: sshSsid,
    token: sshToken,
    addr: sshAddr,
    io,
  });
  io.onStop = () => engine.stop();

  console.log(`[SIDECAR] engine=${engine.name}, waiting for user messages...`);

  while (true) {
    const userPrompt = await io.readLine();
    const trimmed = userPrompt.trim();
    if (!trimmed) continue;

    if (trimmed === "__QUIT__") {
      console.log("[SIDECAR] Received quit command, exiting...");
      console.log(SESSION_STOP);
      break;
    }

    try {
      await engine.handle(userPrompt);
    } catch (error) {
      if (!engine.stopped) {
        console.error("[SIDECAR ERROR]", error);
        emitSystemError(error instanceof Error ? error.message : String(error));
      }
    }

    if (engine.stopped) emitStopped();
    console.log(END_OF_RESPONSE);
  }
}

main()
  .catch((error) => {
    console.error("[SIDECAR FATAL]", error);
    emitSystemError(error instanceof Error ? error.message : String(error));
  })
  .finally(() => process.exit(0));
