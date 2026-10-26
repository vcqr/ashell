/**
 * 机器人会话：长轮询保活 + context token 采集 + 文本发送。
 * 轮询行为对齐插件 monitor：-14 token 失效暂停 1 小时，其余错误指数退避。
 */

import {
  classifyFetchError,
  DEFAULT_BASE_URL,
  getUpdates,
  notifyStart,
  notifyStop,
  sendMessage,
  STALE_TOKEN_ERRCODE,
  StaleTokenError,
} from "./protocol/api.js";
import {
  MessageItemType,
  MessageState,
  MessageType,
} from "./protocol/types.js";
import type { WeixinMessage } from "./protocol/types.js";
import type { Credentials } from "./store.js";
import { loadCredentials, loadPeers, loadSyncBuf, savePeer, saveSyncBuf } from "./store.js";
import { logger } from "./logger.js";

export class NotLoggedInError extends Error {}

/** token 失效冷却中（-14），重新扫码后自动解除。 */
export class SessionPausedError extends Error {}

const RETRY_DELAY_MS = 2_000;
const MAX_CONSECUTIVE_FAILURES = 5;
const BACKOFF_DELAY_MS = 30_000;
/** errcode -14：token 失效，与官方插件一致按 1 小时冷却处理（常量在 protocol/api.ts）。 */
const STALE_PAUSE_MS = 60 * 60 * 1000;
/** 单条消息长度上限（字符），超出拆分为多条。 */
const MAX_TEXT_CHARS = 1800;

export interface BotState {
  running: boolean;
  pausedUntil: number;
  lastPollAt?: number;
  lastInboundAt?: number;
  lastError?: string;
}

const state: BotState = { running: false, pausedUntil: 0 };
/** 触发冷却时使用的 token；换新 token（重新扫码）后自动解除冷却。 */
let pausedToken: string | undefined;

/** 若当前 token 与冷却时不同（已重新登录），清除冷却。 */
function clearPauseIfTokenChanged(): void {
  if (state.pausedUntil > Date.now() && pausedToken && pausedToken !== loadCredentials()?.token) {
    logger.info("检测到新凭证，解除会话冷却");
    state.pausedUntil = 0;
    pausedToken = undefined;
  }
}

export function getBotState(): BotState {
  return { ...state };
}

function credentials(): Credentials {
  const cred = loadCredentials();
  if (!cred?.token) {
    throw new NotLoggedInError("未登录：请先执行 `npm run login` 扫码获取凭证");
  }
  return cred;
}

function baseUrl(): string {
  const cred = loadCredentials();
  return cred?.baseUrl?.trim() || DEFAULT_BASE_URL;
}

// ---------------------------------------------------------------------------
// 发送
// ---------------------------------------------------------------------------

/**
 * 轻量 markdown 转纯文本：微信机器人消息不渲染 markdown。
 * 只处理最常见的语法（代码围栏/行内代码/加粗/标题/链接），保留原始换行。
 */
export function stripMarkdown(text: string): string {
  return text
    .replace(/```[a-zA-Z0-9_-]*\n?/g, "")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/(^|\n)#{1,6}\s+/g, "$1")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, "$1($2)")
    // 通知文案常见的装饰符号
    .replace(/^\s*[-*]\s+/gm, "• ")
    .trimEnd();
}

function splitText(text: string): string[] {
  if (text.length <= MAX_TEXT_CHARS) return [text];
  const chunks: string[] = [];
  let rest = text;
  while (rest.length > MAX_TEXT_CHARS) {
    // 优先在换行处断开，其次硬切。
    let cut = rest.lastIndexOf("\n", MAX_TEXT_CHARS);
    if (cut < MAX_TEXT_CHARS / 2) cut = MAX_TEXT_CHARS;
    chunks.push(rest.slice(0, cut));
    rest = rest.slice(cut).replace(/^\n/, "");
  }
  if (rest) chunks.push(rest);
  return chunks;
}

function clientId(): string {
  return `wb:${Date.now()}-${Math.random().toString(16).slice(2, 10)}`;
}

/**
 * 发送文本到指定用户（默认推送目标：扫码者 → 最近活跃的配对用户）。
 * 无 context token 也允许发送（与插件行为一致），但配对后可达性更好。
 */
export async function sendText(to: string | undefined, rawText: string): Promise<string[]> {
  const cred = credentials();
  const target = to?.trim() || cred.userId?.trim() || mostRecentPeer();
  if (!target) {
    throw new Error(
      "没有可用的推送目标：请用微信给机器人发一条消息完成配对，或在请求里指定 to 字段",
    );
  }

  if (state.pausedUntil > Date.now()) {
    clearPauseIfTokenChanged();
    if (state.pausedUntil > Date.now()) {
      const min = Math.ceil((state.pausedUntil - Date.now()) / 60_000);
      throw new SessionPausedError(
        `会话冷却中（token 失效），约 ${min} 分钟后重试或重新登录`,
      );
    }
  }

  const peers = loadPeers();
  const contextToken = peers[target]?.contextToken;

  const messageIds: string[] = [];
  for (const chunk of splitText(rawText)) {
    let resp;
    try {
      resp = await sendMessage({
        baseUrl: baseUrl(),
        token: cred.token,
        body: {
          msg: {
            from_user_id: "",
            to_user_id: target,
            client_id: clientId(),
            message_type: MessageType.BOT,
            message_state: MessageState.FINISH,
            item_list: chunk ? [{ type: MessageItemType.TEXT, text_item: { text: chunk } }] : undefined,
            context_token: contextToken,
          },
        },
      });
    } catch (err) {
      if (err instanceof StaleTokenError) {
        // 发送时发现 token 失效：与轮询同样进入冷却，并向上返回 503。
        state.pausedUntil = Date.now() + STALE_PAUSE_MS;
        pausedToken = cred.token;
        state.lastError = err.message;
        throw new SessionPausedError(`${err.message}（已进入冷却，重新扫码后自动恢复）`);
      }
      throw err;
    }
    messageIds.push(resp.message_id ?? "");
  }
  return messageIds;
}

function mostRecentPeer(): string | undefined {
  const peers = loadPeers();
  let latest: string | undefined;
  let latestAt = 0;
  for (const [userId, peer] of Object.entries(peers)) {
    const at = Date.parse(peer.updatedAt) || 0;
    if (at >= latestAt) {
      latestAt = at;
      latest = userId;
    }
  }
  return latest;
}

// ---------------------------------------------------------------------------
// 长轮询循环
// ---------------------------------------------------------------------------

/** 从入站消息提取文本预览（用于配对日志）。 */
function messagePreview(msg: WeixinMessage): string {
  for (const item of msg.item_list ?? []) {
    if (item.type === MessageItemType.TEXT) {
      const text = item.text_item?.text ?? "";
      return text.length > 50 ? `${text.slice(0, 50)}…` : text;
    }
    if (item.type != null && item.type !== MessageItemType.NONE) {
      const typeName = Object.entries(MessageItemType).find(([, v]) => v === item.type)?.[0];
      return `(${typeName ?? item.type})`;
    }
  }
  return "";
}

/** 单次 getUpdates 轮询 + 入站处理；返回是否正常。 */
async function pollOnce(abortSignal: AbortSignal, buf: string): Promise<string> {
  const cred = credentials();
  const resp = await getUpdates({
    baseUrl: baseUrl(),
    token: cred.token,
    get_updates_buf: buf,
    abortSignal,
  });

  if ((resp.ret !== undefined && resp.ret !== 0) || (resp.errcode !== undefined && resp.errcode !== 0)) {
    const stale = resp.errcode === STALE_TOKEN_ERRCODE || resp.ret === STALE_TOKEN_ERRCODE;
    if (stale) {
      state.pausedUntil = Date.now() + STALE_PAUSE_MS;
      pausedToken = cred.token;
      state.lastError = `token 失效(errcode -14)，暂停至 ${new Date(state.pausedUntil).toISOString()}`;
      logger.error(`${state.lastError} — 请重新执行 npm run login`);
      return buf;
    }
    state.lastError = `getUpdates ret=${resp.ret} errcode=${resp.errcode} errmsg=${resp.errmsg ?? ""}`;
    logger.error(state.lastError);
    throw new Error(state.lastError);
  }

  state.lastPollAt = Date.now();
  state.lastError = undefined;

  let currentBuf = buf;
  const newBuf = saveSyncBuf(resp);
  if (newBuf) currentBuf = newBuf;

  for (const msg of resp.msgs ?? []) {
    const from = msg.from_user_id ?? "";
    if (!from) continue;
    state.lastInboundAt = Date.now();
    if (msg.context_token) {
      savePeer(from, {
        contextToken: msg.context_token,
        updatedAt: new Date().toISOString(),
        lastText: messagePreview(msg),
      });
      logger.info(`配对更新: from=${from} text="${messagePreview(msg)}"`);
    }
  }
  return currentBuf;
}

/** 启动长轮询循环（保活 + 配对采集），返回停止函数。 */
export function startPolling(): (persistStop?: boolean) => Promise<void> {
  credentials();
  const controller = new AbortController();
  state.running = true;
  state.pausedUntil = 0;

  let buf = loadSyncBuf();
  logger.info(
    buf ? `沿用已有 get_updates_buf（${buf.length} 字节）` : "无历史 buf，从头开始同步",
  );

  // 上线告知（失败不阻塞）。
  notifyStart({ baseUrl: baseUrl(), token: loadCredentials()?.token }).catch((err) => {
    logger.warn(`notifyStart 失败（不影响运行）: ${String(err)}`);
  });

  const loop = (async () => {
    let consecutiveFailures = 0;
    while (!controller.signal.aborted) {
      if (state.pausedUntil > Date.now()) {
        clearPauseIfTokenChanged();
        await sleep(Math.min(state.pausedUntil - Date.now() + 1000, 10_000));
        continue;
      }
      try {
        buf = await pollOnce(controller.signal, buf);
        consecutiveFailures = 0;
      } catch (err) {
        if (controller.signal.aborted) break;
        consecutiveFailures++;
        const c = classifyFetchError(err);
        logger.error(
          `getUpdates 异常 (${consecutiveFailures}/${MAX_CONSECUTIVE_FAILURES}): type=${c.type} desc=${c.description}`,
        );
        if (consecutiveFailures >= MAX_CONSECUTIVE_FAILURES) {
          consecutiveFailures = 0;
          await sleep(BACKOFF_DELAY_MS);
        } else {
          await sleep(RETRY_DELAY_MS);
        }
      }
    }
    logger.info("轮询循环已退出");
  })();

  return async (persistStop = true) => {
    controller.abort();
    state.running = false;
    await loop.catch(() => {});
    if (persistStop) {
      try {
        await notifyStop({ baseUrl: baseUrl(), token: loadCredentials()?.token });
      } catch {
        // 退出时的下线告知失败可忽略。
      }
    }
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
