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
 * 当前窗口 id。
 * 桌面端是 Tauri window label；Web 端每标签页随机生成（sessionStorage 持久，
 * 刷新保持稳定，跨标签页互异），供跨窗口广播等按窗口寻址的场景使用。
 */
export function getWindowId(): string {
  if (isTauri) return ""
  let id = sessionStorage.getItem("ashell:window-id")
  if (!id) {
    id = `web-${crypto.randomUUID().slice(0, 8)}`
    try {
      sessionStorage.setItem("ashell:window-id", id)
    } catch {
      // sessionStorage 不可用时退化为会话内临时 id
    }
  }
  return id
}
