/**
 * 微信 iLink 机器人 HTTP 协议层。
 * 从 @tencent-weixin/openclaw-weixin@2.4.10 的 src/api/api.ts 抽取，
 * 去除 OpenClaw SDK 依赖（配置改环境变量、日志换内置实现），请求格式保持一致。
 */

import crypto from "node:crypto";

import { logger } from "../logger.js";
import type {
  BaseInfo,
  GetUpdatesReq,
  GetUpdatesResp,
  NotifyStartResp,
  NotifyStopResp,
  SendMessageReq,
  SendMessageResp,
} from "./types.js";

export const DEFAULT_BASE_URL = "https://ilinkai.weixin.qq.com";

export const BRIDGE_VERSION = "0.1.0";

/** iLink-App-Id：对齐插件 package.json 顶层 ilink_appid 字段。 */
const ILINK_APP_ID = "bot";

/**
 * iLink-App-ClientVersion: uint32 编码 0x00MMNNPP。
 * 对齐插件版本，避免服务端按过低版本降级处理。
 */
const ILINK_APP_CLIENT_VERSION = (() => {
  const parts = "2.4.10".split(".").map((p) => parseInt(p, 10) || 0);
  return ((parts[0]! & 0xff) << 16) | ((parts[1]! & 0xff) << 8) | (parts[2]! & 0xff);
})();

const DEFAULT_BOT_AGENT = `weixin-bridge/${BRIDGE_VERSION}`;

function botAgent(): string {
  const raw = process.env.WEIXIN_BRIDGE_BOT_AGENT?.trim();
  return raw ? raw : DEFAULT_BOT_AGENT;
}

function routeTag(): string | undefined {
  const raw = process.env.WEIXIN_BRIDGE_ROUTE_TAG?.trim();
  return raw || undefined;
}

/** 构造每个请求附带的 base_info。 */
export function buildBaseInfo(): BaseInfo {
  return { channel_version: BRIDGE_VERSION, bot_agent: botAgent() };
}

/** 长轮询 getUpdates 默认超时。 */
const DEFAULT_LONG_POLL_TIMEOUT_MS = 35_000;
/** 常规请求（sendMessage 等）默认超时。 */
const DEFAULT_API_TIMEOUT_MS = 15_000;
/** 轻量请求默认超时。 */
const DEFAULT_CONFIG_TIMEOUT_MS = 10_000;

function ensureTrailingSlash(url: string): string {
  return url.endsWith("/") ? url : `${url}/`;
}

/** X-WECHAT-UIN 头：随机 uint32 → 十进制字符串 → base64。 */
function randomWechatUin(): string {
  const uint32 = crypto.randomBytes(4).readUInt32BE(0);
  return Buffer.from(String(uint32), "utf-8").toString("base64");
}

function buildCommonHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    "iLink-App-Id": ILINK_APP_ID,
    "iLink-App-ClientVersion": String(ILINK_APP_CLIENT_VERSION),
  };
  const tag = routeTag();
  if (tag) headers.SKRouteTag = tag;
  return headers;
}

function buildHeaders(opts: { token?: string }): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    AuthorizationType: "ilink_bot_token",
    "X-WECHAT-UIN": randomWechatUin(),
    ...buildCommonHeaders(),
  };
  if (opts.token?.trim()) {
    headers.Authorization = `Bearer ${opts.token.trim()}`;
  }
  return headers;
}

/** 请求未带 token 时抛出，避免无意义的 401 往返。 */
function requireToken(token: string | undefined, label: string): string {
  const t = token?.trim();
  if (!t) {
    throw new Error(
      `${label}: 未登录 — 请先执行 \`npm run login\` 扫码获取凭证`,
    );
  }
  return t;
}

/**
 * 把 fetch 层错误归类，用于日志与重试判断（不含 HTTP 4xx/5xx，那些另行抛出）。
 */
export function classifyFetchError(err: unknown): {
  type: "dns" | "tcp" | "tls" | "timeout" | "unknown";
  description: string;
  code?: string;
} {
  if (err instanceof Error && err.name === "AbortError") {
    return { type: "timeout", description: "request timeout" };
  }
  const cause = (err as NodeJS.ErrnoException)?.cause;
  const causeCode = (cause as { code?: string })?.code ?? "";
  const causeStr = String(cause ?? err ?? "") + " " + String(causeCode);
  const matchedCode = causeCode || (typeof cause === "string" ? cause : "");

  if (/ENOTFOUND|EAI_AGAIN|getaddrinfo/i.test(causeStr)) {
    return {
      type: "dns",
      description: "DNS 解析失败",
      ...(matchedCode ? { code: matchedCode } : {}),
    };
  }
  if (/ECONNREFUSED/i.test(causeStr)) {
    return { type: "tcp", description: "TCP 连接被拒绝", ...(matchedCode ? { code: matchedCode } : {}) };
  }
  if (/UND_ERR_CONNECT_TIMEOUT|ETIMEDOUT|ENETUNREACH|EHOSTUNREACH/i.test(causeStr)) {
    return {
      type: "tcp",
      description: "TCP 连接超时或不可达",
      ...(matchedCode ? { code: matchedCode } : {}),
    };
  }
  if (/UND_ERR_SOCKET|SSL|TLS|CERT|UNABLE_TO_VERIFY|DEPTH_ZERO/i.test(causeStr)) {
    return { type: "tls", description: "TLS 握手失败", ...(matchedCode ? { code: matchedCode } : {}) };
  }
  return { type: "unknown", description: "网络请求失败" };
}

/** GET 请求微信 API，返回原始响应文本；HTTP 非 2xx 抛错。 */
export async function apiGetFetch(params: {
  baseUrl: string;
  endpoint: string;
  timeoutMs?: number;
  label: string;
}): Promise<string> {
  const url = new URL(params.endpoint, ensureTrailingSlash(params.baseUrl));
  const hdrs = buildCommonHeaders();
  logger.debug(`GET ${url.toString()}`);

  const timeoutMs = params.timeoutMs;
  const controller = timeoutMs != null && timeoutMs > 0 ? new AbortController() : undefined;
  const t =
    controller != null && timeoutMs != null
      ? setTimeout(() => controller.abort(), timeoutMs)
      : undefined;
  try {
    const res = await fetch(url.toString(), {
      method: "GET",
      headers: hdrs,
      ...(controller ? { signal: controller.signal } : {}),
    });
    if (t !== undefined) clearTimeout(t);
    const rawText = await res.text();
    logger.debug(`${params.label} status=${res.status} raw=${rawText.slice(0, 300)}`);
    if (!res.ok) {
      throw new Error(`${params.label} ${res.status}: ${rawText}`);
    }
    return rawText;
  } catch (err) {
    if (t !== undefined) clearTimeout(t);
    const c = classifyFetchError(err);
    logger.error(
      `${params.label}: GET 失败 type=${c.type} desc=${c.description}${c.code ? ` code=${c.code}` : ""} err=${String(err)}`,
    );
    throw err;
  }
}

/** POST JSON 到微信 API，返回原始响应文本；HTTP 非 2xx 抛错。 */
export async function apiPostFetch(params: {
  baseUrl: string;
  endpoint: string;
  body: string;
  token?: string;
  timeoutMs?: number;
  label: string;
  abortSignal?: AbortSignal;
}): Promise<string> {
  const url = new URL(params.endpoint, ensureTrailingSlash(params.baseUrl));
  const hdrs = buildHeaders({ token: params.token });
  logger.debug(`POST ${url.toString()} body=${params.body.slice(0, 300)}`);

  const controller = params.timeoutMs !== undefined ? new AbortController() : undefined;
  const t =
    controller != null && params.timeoutMs !== undefined
      ? setTimeout(() => controller.abort(), params.timeoutMs)
      : undefined;
  if (params.abortSignal && controller && !params.abortSignal.aborted) {
    const onAbort = () => controller.abort();
    params.abortSignal.addEventListener("abort", onAbort, { once: true });
    params.abortSignal.addEventListener("abort", () => clearTimeout(t), { once: true });
  }
  const signal = controller?.signal ?? params.abortSignal;
  try {
    const res = await fetch(url.toString(), {
      method: "POST",
      headers: hdrs,
      body: params.body,
      ...(signal ? { signal } : {}),
    });
    if (t !== undefined) clearTimeout(t);
    const rawText = await res.text();
    logger.debug(`${params.label} status=${res.status} raw=${rawText.slice(0, 300)}`);
    if (!res.ok) {
      throw new Error(`${params.label} ${res.status}: ${rawText}`);
    }
    return rawText;
  } catch (err) {
    if (t !== undefined) clearTimeout(t);
    const c = classifyFetchError(err);
    logger.error(
      `${params.label}: POST 失败 type=${c.type} desc=${c.description}${c.code ? ` code=${c.code}` : ""} err=${String(err)}`,
    );
    throw err;
  }
}

const LOSSLESS_ID_FIELDS = new Set(["message_id", "msg_id", "svr_id"]);

/**
 * JSON.parse 前把 uint64 消息 ID 字段改写为字符串，避免精度丢失。
 * 扫描器只改写对象属性名后的数字字面量，不会碰字符串内容。
 */
export function parseWeixinApiJson<T>(rawText: string): T {
  let output = "";
  let index = 0;
  while (index < rawText.length) {
    if (rawText[index] !== '"') {
      output += rawText[index++];
      continue;
    }

    const stringStart = index;
    index++;
    let escaped = false;
    while (index < rawText.length) {
      const char = rawText[index++];
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === '"') {
        break;
      }
    }
    const stringToken = rawText.slice(stringStart, index);
    output += stringToken;

    let cursor = index;
    while (/\s/.test(rawText[cursor] ?? "")) cursor++;
    if (rawText[cursor] !== ":") continue;

    let key: unknown;
    try {
      key = JSON.parse(stringToken);
    } catch {
      continue;
    }
    if (typeof key !== "string" || !LOSSLESS_ID_FIELDS.has(key)) continue;

    output += rawText.slice(index, cursor + 1);
    cursor++;
    while (/\s/.test(rawText[cursor] ?? "")) {
      output += rawText[cursor++];
    }
    const numberStart = cursor;
    if (rawText[cursor] === "-") cursor++;
    while (/\d/.test(rawText[cursor] ?? "")) cursor++;
    if (cursor > numberStart && !(cursor === numberStart + 1 && rawText[numberStart] === "-")) {
      output += `"${rawText.slice(numberStart, cursor)}"`;
      index = cursor;
    } else {
      index = numberStart;
    }
  }
  return JSON.parse(output) as T;
}

/**
 * 长轮询收消息。服务端会挂住请求直到有新消息或超时；
 * 客户端超时返回 ret=0 空响应，调用方直接重试即可。
 */
export async function getUpdates(
  params: GetUpdatesReq & {
    baseUrl: string;
    token?: string;
    timeoutMs?: number;
    abortSignal?: AbortSignal;
  },
): Promise<GetUpdatesResp> {
  const timeout = params.timeoutMs ?? DEFAULT_LONG_POLL_TIMEOUT_MS;
  const token = requireToken(params.token, "getUpdates");
  try {
    const rawText = await apiPostFetch({
      baseUrl: params.baseUrl,
      endpoint: "ilink/bot/getupdates",
      body: JSON.stringify({
        get_updates_buf: params.get_updates_buf ?? "",
        base_info: buildBaseInfo(),
      }),
      token,
      timeoutMs: timeout,
      label: "getUpdates",
      abortSignal: params.abortSignal,
    });
    return parseWeixinApiJson<GetUpdatesResp>(rawText);
  } catch (err) {
    // 长轮询超时与外部中止都是正常退出路径。
    if (err instanceof Error && err.name === "AbortError") {
      logger.debug(
        params.abortSignal?.aborted
          ? "getUpdates: 外部信号中止"
          : `getUpdates: 客户端超时 ${timeout}ms，返回空响应`,
      );
      return { ret: 0, msgs: [], get_updates_buf: params.get_updates_buf };
    }
    throw err;
  }
}

/** 服务端用 errcode=-14 表示 token 失效（HTTP 仍为 200）。 */
export const STALE_TOKEN_ERRCODE = -14;

export class StaleTokenError extends Error {
  constructor() {
    super("token 失效(errcode -14)：会话过期，请重新扫码登录");
    this.name = "StaleTokenError";
  }
}

/** 发送一条消息，返回服务端消息 ID；ret/errcode 非 0 抛错。 */
export async function sendMessage(
  params: { baseUrl: string; token?: string; timeoutMs?: number } & { body: SendMessageReq },
): Promise<SendMessageResp> {
  const token = requireToken(params.token, "sendMessage");
  const rawText = await apiPostFetch({
    baseUrl: params.baseUrl,
    endpoint: "ilink/bot/sendmessage",
    body: JSON.stringify({ ...params.body, base_info: buildBaseInfo() }),
    token,
    timeoutMs: params.timeoutMs ?? DEFAULT_API_TIMEOUT_MS,
    label: "sendMessage",
  });
  const resp = parseWeixinApiJson<SendMessageResp>(rawText);
  // 注意：后端在 HTTP 200 下用 errcode 报错（如 {"errcode":-14,"errmsg":"session timeout"}），
  // ret 与 errcode 都要检查。
  if (resp.errcode !== undefined && resp.errcode !== 0) {
    if (resp.errcode === STALE_TOKEN_ERRCODE) throw new StaleTokenError();
    throw new Error(`sendMessage errcode=${resp.errcode} errmsg=${resp.errmsg ?? "(无)"}`);
  }
  if (resp.ret && resp.ret !== 0) {
    throw new Error(`sendMessage ret=${resp.ret} errmsg=${resp.errmsg ?? "(无)"}`);
  }
  return resp;
}

/** 通知服务端客户端启动。 */
export async function notifyStart(params: {
  baseUrl: string;
  token?: string;
}): Promise<NotifyStartResp> {
  const token = requireToken(params.token, "notifyStart");
  const rawText = await apiPostFetch({
    baseUrl: params.baseUrl,
    endpoint: "ilink/bot/msg/notifystart",
    body: JSON.stringify({ base_info: buildBaseInfo() }),
    token,
    timeoutMs: DEFAULT_CONFIG_TIMEOUT_MS,
    label: "notifyStart",
  });
  return JSON.parse(rawText) as NotifyStartResp;
}

/** 通知服务端客户端即将停止（进程退出前调用）。 */
export async function notifyStop(params: {
  baseUrl: string;
  token?: string;
}): Promise<NotifyStopResp> {
  const token = requireToken(params.token, "notifyStop");
  const rawText = await apiPostFetch({
    baseUrl: params.baseUrl,
    endpoint: "ilink/bot/msg/notifystop",
    body: JSON.stringify({ base_info: buildBaseInfo() }),
    token,
    timeoutMs: DEFAULT_CONFIG_TIMEOUT_MS,
    label: "notifyStop",
  });
  return JSON.parse(rawText) as NotifyStopResp;
}
