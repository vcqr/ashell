/** 本地 HTTP 服务：接收 ashell custom 平台（或 curl）的推送并转发到微信。 */

import http from "node:http";

import { getBotState, sendText, stripMarkdown, NotLoggedInError, SessionPausedError } from "./bot.js";
import { loadCredentials, loadPeers } from "./store.js";
import { logger } from "./logger.js";

export interface ServerOptions {
  port: number;
  host: string;
  /** 可选的 Bearer Token，设置后请求必须带 Authorization: Bearer <token>。 */
  token?: string;
}

interface NotifyBody {
  /** 标题，与 content 之间以空行分隔。 */
  title?: string;
  /** 正文。 */
  content?: string;
  /** 兼容字段：等同 content。 */
  text?: string;
  /** 兼容字段：等同 content。 */
  markdown?: string;
  /** 指定接收者 ilink 用户 ID；缺省用配对目标。 */
  to?: string;
  /** "plain" 跳过 markdown 清洗，默认 "md"。 */
  format?: "md" | "plain";
}

function readBody(req: http.IncomingMessage, limit = 1024 * 1024): Promise<string> {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks: Buffer[] = [];
    req.on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (size > limit) {
        reject(new Error("请求体超过 1MiB 限制"));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf-8")));
    req.on("error", reject);
  });
}

function sendJson(res: http.ServerResponse, status: number, data: unknown): void {
  const body = JSON.stringify(data);
  res.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": Buffer.byteLength(body),
  });
  res.end(body);
}

function authorized(req: http.IncomingMessage, opts: ServerOptions): boolean {
  if (!opts.token) return true;
  const header = req.headers.authorization ?? "";
  return header === `Bearer ${opts.token}`;
}

export function startServer(opts: ServerOptions): http.Server {
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "localhost"}`);

    try {
      if (!authorized(req, opts)) {
        sendJson(res, 401, { ok: false, error: "未授权：缺少或错误的 Bearer Token" });
        return;
      }

      if (req.method === "GET" && url.pathname === "/status") {
        const cred = loadCredentials();
        const bot = getBotState();
        sendJson(res, 200, {
          ok: true,
          loggedIn: Boolean(cred?.token),
          botId: cred?.botId,
          userId: cred?.userId,
          polling: bot.running,
          pausedUntil: bot.pausedUntil || undefined,
          lastPollAt: bot.lastPollAt,
          lastInboundAt: bot.lastInboundAt,
          lastError: bot.lastError,
          peers: Object.keys(loadPeers()).length,
        });
        return;
      }

      if (req.method === "GET" && url.pathname === "/peers") {
        sendJson(res, 200, { ok: true, peers: loadPeers() });
        return;
      }

      if ((req.method === "POST" && (url.pathname === "/notify" || url.pathname === "/send"))) {
        const raw = await readBody(req);
        let body: NotifyBody;
        try {
          body = JSON.parse(raw) as NotifyBody;
        } catch {
          // 非 JSON：按纯文本处理。
          body = { content: raw };
        }
        const content = (body.content ?? body.text ?? body.markdown ?? "").toString();
        if (!content.trim() && !body.title?.trim()) {
          sendJson(res, 400, { ok: false, error: "缺少 title/content 内容" });
          return;
        }
        const composed =
          body.title?.trim() && content.trim()
            ? `${body.title.trim()}\n\n${content}`
            : `${body.title?.trim() ?? ""}${content}`;
        const text = body.format === "plain" ? composed.trim() : stripMarkdown(composed).trim();
        if (!text) {
          sendJson(res, 400, { ok: false, error: "清洗后内容为空" });
          return;
        }
        const ids = await sendText(body.to, text);
        logger.info(`已推送: to=${body.to ?? "(默认)"} chars=${text.length} msgs=${ids.length}`);
        sendJson(res, 200, { ok: true, messageIds: ids, chars: text.length });
        return;
      }

      sendJson(res, 404, { ok: false, error: `未知路由 ${req.method} ${url.pathname}` });
    } catch (err) {
      if (err instanceof NotLoggedInError) {
        sendJson(res, 503, { ok: false, error: err.message });
        return;
      }
      if (err instanceof SessionPausedError) {
        sendJson(res, 503, { ok: false, error: err.message });
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      logger.error(`HTTP 处理失败: ${message}`);
      sendJson(res, 500, { ok: false, error: message });
    }
  });

  server.listen(opts.port, opts.host, () => {
    logger.info(`HTTP 服务已启动: http://${opts.host}:${opts.port}`);
    logger.info(`推送接口: POST http://${opts.host}:${opts.port}/notify  body: {"title":"...","content":"..."}`);
  });

  return server;
}
