import { getApiInfo, request } from "./client"
import type { IconItem } from "@/types"

/** 列出所有可用的主机图标 */
export function listIcons(): Promise<IconItem[]> {
  return request<IconItem[]>("/api/icons")
}

/**
 * 给 <img src> 拼一个带 token 的图标 URL。
 *
 * 必须用 query token 而不是 Authorization header——`<img>` 的 fetch
 * 是浏览器隐式发起的，无法注入自定义 header；和 WebSocket 同一原因。
 *
 * 可选 `version` 参数会被加到 query string，配合后端返回的 mtime 作为
 * 缓存破坏键：用户替换图标文件后 mtime 改变，URL 自然变化，浏览器/img
 * 缓存会重新拉取；同一 mtime 下 URL 不变，可继续命中缓存。
 */
export async function buildIconUrl(
  name: string,
  version?: number | string,
): Promise<string> {
  const info = await getApiInfo()
  const sp = new URLSearchParams()
  sp.set("token", info.token)
  if (version != null) sp.set("v", String(version))
  return `${info.base_url}/api/icons/${encodeURIComponent(name)}?${sp.toString()}`
}
