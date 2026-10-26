/** 本地状态持久化：凭证、get_updates_buf、会话 context tokens。 */

import fs from "node:fs";
import path from "node:path";
import os from "node:os";

import type { GetUpdatesResp } from "./protocol/types.js";

const STATE_DIR = process.env.WEIXIN_BRIDGE_STATE_DIR?.trim()
  ? path.resolve(process.env.WEIXIN_BRIDGE_STATE_DIR)
  : path.join(os.homedir(), ".weixin-bridge");

function statePath(name: string): string {
  return path.join(STATE_DIR, name);
}

function readJson<T>(filePath: string): T | null {
  try {
    if (fs.existsSync(filePath)) {
      return JSON.parse(fs.readFileSync(filePath, "utf-8")) as T;
    }
  } catch {
    // 文件损坏视为不存在，重新登录/重建即可。
  }
  return null;
}

function writeJson(filePath: string, data: unknown, mode?: number): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, JSON.stringify(data, null, 2), "utf-8");
  if (mode !== undefined) {
    try {
      fs.chmodSync(filePath, mode);
    } catch {
      // best-effort
    }
  }
}

export interface Credentials {
  token: string;
  botId: string;
  baseUrl: string;
  /** 扫码者的 ilink 用户 ID，作为默认推送目标。 */
  userId?: string;
  savedAt?: string;
}

export function loadCredentials(): Credentials | null {
  return readJson<Credentials>(statePath("credentials.json"));
}

export function saveCredentials(cred: Omit<Credentials, "savedAt">): void {
  writeJson(
    statePath("credentials.json"),
    { ...cred, savedAt: new Date().toISOString() } satisfies Credentials,
    0o600,
  );
}

export function clearCredentials(): void {
  for (const name of ["credentials.json", "sync.json", "peers.json"]) {
    try {
      fs.unlinkSync(statePath(name));
    } catch {
      // ignore
    }
  }
}

// ---------------------------------------------------------------------------
// get_updates_buf 持久化（断点续传，重启不丢消息）
// ---------------------------------------------------------------------------

export function loadSyncBuf(): string {
  const data = readJson<{ get_updates_buf?: string }>(statePath("sync.json"));
  return typeof data?.get_updates_buf === "string" ? data.get_updates_buf : "";
}

export function saveSyncBuf(resp: GetUpdatesResp): string {
  if (resp.get_updates_buf != null && resp.get_updates_buf !== "") {
    writeJson(statePath("sync.json"), { get_updates_buf: resp.get_updates_buf });
    return resp.get_updates_buf;
  }
  return "";
}

// ---------------------------------------------------------------------------
// context tokens：userId → 最近一次下发的 context_token
// ---------------------------------------------------------------------------

export interface Peer {
  contextToken?: string;
  updatedAt: string;
  lastText?: string;
}

export function loadPeers(): Record<string, Peer> {
  return readJson<Record<string, Peer>>(statePath("peers.json")) ?? {};
}

export function savePeer(userId: string, peer: Peer): void {
  const peers = loadPeers();
  peers[userId] = peer;
  writeJson(statePath("peers.json"), peers);
}
