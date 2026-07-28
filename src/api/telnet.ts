import { buildWsUrl } from "./client"

/** Telnet 终端 WebSocket URL */
export function buildTelnetTerminalWsUrl(
  hostId: number,
  query: { sid?: string } = {},
): Promise<string> {
  return buildWsUrl(`/api/telnet/terminal/${hostId}`, query)
}
