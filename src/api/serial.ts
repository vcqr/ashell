import { buildWsUrl } from "./client"

/** 串口终端 WebSocket URL */
export function buildSerialTerminalWsUrl(
  hostId: number,
  query: { sid?: string } = {},
): Promise<string> {
  return buildWsUrl(`/api/serial/terminal/${hostId}`, query)
}
