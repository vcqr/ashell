import type { HostNode } from "@/types"

/**
 * 主机列表视图共享小工具：HostTree（树形/平铺渲染函数）与
 * HostCardGrid（卡片视图）共用的纯函数。
 */

/** 主机的地址列文本：serial 只显地址，其余协议端口非 22 时附带 `:port` */
export function hostAddrTextOfNode(node: HostNode): string {
  const addr = node.host ?? ""
  if (!addr) return ""
  if (node.protocol === "serial") return addr
  const port = node.port ?? "22"
  return port !== "22" ? `${addr}:${port}` : addr
}

/** 按搜索词切分 label，命中的子串单独成段（大小写不敏感，与 NTree pattern 同口径） */
export function splitHighlightSegments(
  label: string,
  rawQuery: string,
): Array<{ text: string; hit: boolean }> {
  const q = rawQuery.toLowerCase()
  if (!q) return [{ text: label, hit: false }]
  const lower = label.toLowerCase()
  const segs: Array<{ text: string; hit: boolean }> = []
  let i = 0
  while (i < label.length) {
    const idx = lower.indexOf(q, i)
    if (idx === -1) {
      segs.push({ text: label.slice(i), hit: false })
      break
    }
    if (idx > i) segs.push({ text: label.slice(i, idx), hit: false })
    segs.push({ text: label.slice(idx, idx + q.length), hit: true })
    i = idx + q.length
  }
  return segs.length > 0 ? segs : [{ text: label, hit: false }]
}
