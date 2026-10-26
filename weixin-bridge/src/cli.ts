/** CLI 入口：login / serve / status / send / peers / logout */

import { loginWithQr } from "./login.js";
import { sendText, startPolling, getBotState, stripMarkdown } from "./bot.js";
import { startServer } from "./server.js";
import { clearCredentials, loadCredentials, loadPeers } from "./store.js";
import { BRIDGE_VERSION } from "./protocol/api.js";
import { logger } from "./logger.js";

const USAGE = `weixin-bridge v${BRIDGE_VERSION} — 微信通知桥

用法:
  npm run login            扫码登录（保存凭证）
  npm start                启动推送服务（HTTP + 长轮询保活）
  node dist/cli.js status  查看登录与轮询状态
  node dist/cli.js peers   查看已配对的微信用户
  node dist/cli.js send <文本> [--to <userId>] [--plain]   手动发一条
  node dist/cli.js logout  清除本地凭证

环境变量:
  WEIXIN_BRIDGE_PORT       HTTP 端口（默认 7420）
  WEIXIN_BRIDGE_HOST       监听地址（默认 127.0.0.1）
  WEIXIN_BRIDGE_TOKEN      设置后 /notify、/status 需带 Bearer Token
  WEIXIN_BRIDGE_STATE_DIR  状态目录（默认 ~/.weixin-bridge）
  WEIXIN_BRIDGE_DEBUG      =1 输出调试日志
`;

function parseArgs(argv: string[]): { flags: Record<string, string | boolean>; rest: string[] } {
  const flags: Record<string, string | boolean> = {};
  const rest: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--to") {
      flags.to = argv[++i] ?? "";
    } else if (a === "--plain") {
      flags.plain = true;
    } else if (a === "--port") {
      flags.port = argv[++i] ?? "";
    } else if (a === "--host") {
      flags.host = argv[++i] ?? "";
    } else {
      rest.push(a);
    }
  }
  return { flags, rest };
}

async function cmdServe(argv: string[]): Promise<void> {
  const { flags } = parseArgs(argv);
  const cred = loadCredentials();
  if (!cred?.token) {
    logger.error("未登录：请先执行 `npm run login` 扫码获取凭证");
    process.exit(1);
  }

  const port = Number(flags.port || process.env.WEIXIN_BRIDGE_PORT || 7420);
  const host = String(flags.host || process.env.WEIXIN_BRIDGE_HOST || "127.0.0.1");
  const token = process.env.WEIXIN_BRIDGE_TOKEN?.trim() || undefined;

  const server = startServer({ port, host, token });
  const stopPolling = startPolling();

  let stopping = false;
  const shutdown = async (signal: string) => {
    if (stopping) return;
    stopping = true;
    logger.info(`收到 ${signal}，正在退出…`);
    server.close();
    await stopPolling(true);
    process.exit(0);
  };
  process.on("SIGINT", () => void shutdown("SIGINT"));
  process.on("SIGTERM", () => void shutdown("SIGTERM"));
}

async function cmdSend(argv: string[]): Promise<void> {
  const { flags, rest } = parseArgs(argv);
  const text = rest.join(" ");
  if (!text.trim()) {
    console.error("用法: node dist/cli.js send <文本> [--to <userId>] [--plain]");
    process.exit(1);
  }
  const composed = flags.plain ? text : stripMarkdown(text);
  const ids = await sendText(typeof flags.to === "string" ? flags.to : undefined, composed);
  console.log(`已发送 ${ids.length} 条消息`);
}

async function cmdStatus(): Promise<void> {
  const cred = loadCredentials();
  if (!cred) {
    console.log("未登录（无凭证）。执行 npm run login 扫码登录。");
    return;
  }
  console.log(
    JSON.stringify(
      {
        botId: cred.botId,
        userId: cred.userId,
        savedAt: cred.savedAt,
        baseUrl: cred.baseUrl,
        polling: getBotState().running,
        peers: Object.keys(loadPeers()).length,
      },
      null,
      2,
    ),
  );
}

async function main(): Promise<void> {
  const [cmd, ...rest] = process.argv.slice(2);

  switch (cmd) {
    case "login": {
      console.log("开始微信扫码登录…");
      const result = await loginWithQr();
      console.log(`\n✅ 登录成功 botId=${result.botId}${result.userId ? ` 扫码者=${result.userId}` : ""}`);
      console.log(`凭证已保存（token 落盘 0600）。下一步：\n  npm start     启动推送服务\n`);
      break;
    }
    case "serve":
    case "start":
      await cmdServe(rest);
      break;
    case "send":
      await cmdSend(rest);
      break;
    case "status":
      await cmdStatus();
      break;
    case "peers": {
      const peers = loadPeers();
      console.log(JSON.stringify(peers, null, 2));
      break;
    }
    case "logout":
      clearCredentials();
      console.log("已清除本地凭证。");
      break;
    case "help":
    case "--help":
    case "-h":
    case undefined:
      process.stdout.write(USAGE);
      break;
    default:
      console.error(`未知命令: ${cmd}\n`);
      process.stdout.write(USAGE);
      process.exit(1);
  }
}

main().catch((err) => {
  logger.error(String(err instanceof Error ? err.message : err));
  process.exit(1);
});
