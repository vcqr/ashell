import { WebviewWindow } from "@tauri-apps/api/webviewWindow"
import type { TerminalTab } from "@/types"

let seq = 0

/**
 * 把一个 tab 复制到独立的新窗口。
 *
 * 新窗口加载同一个 index.html（全新 Vue 实例 + 独立 Pinia store），通过 URL
 * query string 传递启动参数。后端 axum 是同进程共享的，新窗口经
 * invoke("get_api_info") 拿到相同的 addr/token，因此 SSH/SFTP 都能正常工作。
 *
 * SSH 会话（sid）绑定到前端 tab，不跨窗口迁移——新窗口会建立自己的新连接。
 */
export async function openTabInNewWindow(tab: TerminalTab): Promise<void> {
  seq += 1
  const label = `ashell-win-${Date.now()}-${seq}`

  const base = window.location.href.split("?")[0]
  const params = new URLSearchParams()
  params.set("newwin", "1")
  if (tab.kind === "local") {
    params.set("kind", "local")
    if (tab.shell) params.set("shell", tab.shell)
  } else {
    params.set("kind", "host")
    if (tab.hostId !== undefined) {
      params.set("hostId", String(tab.hostId))
    }
  }
  params.set("title", tab.title)
  const url = `${base}?${params.toString()}`

  const webview = new WebviewWindow(label, {
    url,
    title: tab.title,
    width: 1000,
    height: 700,
    decorations: false,
    center: true,
  })

  webview.once("tauri://error", (e) => {
    console.error("[ashell] failed to create new window:", e)
  })
}
