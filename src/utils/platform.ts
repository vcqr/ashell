/** 是否运行在 Tauri WebView（桌面端）；浏览器访问 ashell-server 时为 false */
export const isTauri = "__TAURI_INTERNALS__" in window

/** 是否为浏览器访问 Web 服务器的形态 */
export const isWeb = !isTauri

/** 平台检测：userAgentData 优先（Chromium），退化到 userAgent + platform */
export function detectMac(): boolean {
  if (typeof navigator === "undefined") return false
  const ua = navigator.userAgent || ""
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } })
      .userAgentData?.platform ||
    navigator.platform ||
    ""
  return /Mac|iPhone|iPad|iPod/i.test(`${ua} ${platform}`)
}

/**
 * 用系统默认方式打开外部链接。
 * 桌面端走 plugin-opener（系统浏览器），Web 端开新标签页。
 */
export function openExternal(url: string): void {
  if (isTauri) {
    void import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url))
    return
  }
  window.open(url, "_blank", "noopener,noreferrer")
}

/**
 * crypto.randomUUID 仅在安全上下文（HTTPS / localhost）可用；
 * 局域网 http://IP 访问时降级手拼 v4 UUID（getRandomValues 非安全上下文仍可用）。
 */
function randomUuid(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID()
  }
  const bytes = new Uint8Array(16)
  if (typeof crypto !== "undefined" && typeof crypto.getRandomValues === "function") {
    crypto.getRandomValues(bytes)
  } else {
    for (let i = 0; i < 16; i++) bytes[i] = Math.floor(Math.random() * 256)
  }
  bytes[6] = (bytes[6]! & 0x0f) | 0x40
  bytes[8] = (bytes[8]! & 0x3f) | 0x80
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

/**
 * 当前窗口 id。
 * 桌面端是 Tauri window label；Web 端每标签页随机生成（sessionStorage 持久，
 * 刷新保持稳定，跨标签页互异），供跨窗口广播等按窗口寻址的场景使用。
 */
export function getWindowId(): string {
  if (isTauri) return ""
  let id = sessionStorage.getItem("ashell:window-id")
  if (!id) {
    id = `web-${randomUuid().slice(0, 8)}`
    try {
      sessionStorage.setItem("ashell:window-id", id)
    } catch {
      // sessionStorage 不可用时退化为会话内临时 id
    }
  }
  return id
}
