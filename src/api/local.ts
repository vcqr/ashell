import { buildWsUrl } from "./client"

/** 本地 PTY 终端 WebSocket URL */
export function buildLocalTerminalWsUrl(
  query: { sid?: string; shell?: string } = {},
): Promise<string> {
  return buildWsUrl("/api/local/terminal", query)
}
