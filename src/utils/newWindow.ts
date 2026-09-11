import { WebviewWindow } from "@tauri-apps/api/webviewWindow"
import type { ChatMessage, TerminalTab } from "@/types"

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
    // 与主窗口（tauri.conf.json）保持一致：关闭 wry 原生拖放拦截，
    // 否则 Windows 下 DOM drop 事件的多文件数据会被吞成 1 个
    dragDropEnabled: false,
  })

  webview.once("tauri://error", (e) => {
    console.error("[ashell] failed to create new window:", e)
  })
}

export interface OpenSftpWindowOptions {
  sid: string
  title: string
  addr?: string
  /** 关联主机 id（SFTP 面板"提权"等需要主机凭证上下文的功能依赖它） */
  hostId?: number | null
}

/**
 * 把 SFTP 面板弹出到独立窗口。
 *
 * 与终端新窗口（重新建连）不同，SFTP 窗口直接复用当前 SSH 会话：会话
 * 存活在后端 axum 进程里，新窗口凭相同 token 用同一个 sid 调 SFTP 接口
 * 即可。因此独立窗口的生命周期跟随原终端 tab--tab 断开/关闭后，独立
 * 窗口内的 SFTP 操作会失败。
 */
export async function openSftpInNewWindow(opts: OpenSftpWindowOptions): Promise<void> {
  seq += 1
  const label = `ashell-sftp-${Date.now()}-${seq}`

  const base = window.location.href.split("?")[0]
  const params = new URLSearchParams()
  params.set("newwin", "1")
  params.set("kind", "sftp")
  params.set("sid", opts.sid)
  if (opts.addr) params.set("addr", opts.addr)
  if (opts.hostId != null) params.set("hostId", String(opts.hostId))
  params.set("title", opts.title)
  const url = `${base}?${params.toString()}`

  const webview = new WebviewWindow(label, {
    url,
    title: opts.title ? `SFTP - ${opts.title}` : "SFTP",
    width: 1100,
    height: 720,
    minWidth: 860,
    minHeight: 480,
    decorations: false,
    center: true,
    // SFTP 独立窗口同样走 HTML5 dnd 接收 OS 文件拖放，必须关闭原生拦截
    dragDropEnabled: false,
  })

  webview.once("tauri://error", (e) => {
    console.error("[ashell] failed to create sftp window:", e)
  })
}

export interface OpenAiWindowOptions {
  ssid: string
  title?: string
  /** 弹出时移交的对话历史（新窗口经 localStorage 取回后删除） */
  history: ChatMessage[]
}

/**
 * 把 AI 助手面板弹出到独立窗口。
 *
 * 与 SFTP 窗口同策略：sidecar 进程按 ssid 存活于后端，不随窗口迁移；
 * 新窗口凭 has_sidecar 附着监听同一事件流（Tauri emit 广播到所有窗口），
 * 不重复拉起进程。对话历史经 localStorage 一次性移交。原终端 tab 关闭
 * 后 sidecar 被回收，本窗口继续发送会失败。
 */
export async function openAiInNewWindow(opts: OpenAiWindowOptions): Promise<void> {
  seq += 1
  const label = `ashell-ai-${Date.now()}-${seq}`

  try {
    localStorage.setItem(
      `ashell:ai-handover:${opts.ssid}`,
      JSON.stringify({ messages: opts.history }),
    )
  } catch {
    // localStorage 不可用时降级为新窗口空白历史
  }

  const base = window.location.href.split("?")[0]
  const params = new URLSearchParams()
  params.set("newwin", "1")
  params.set("kind", "ai")
  params.set("ssid", opts.ssid)
  if (opts.title) params.set("title", opts.title)
  const url = `${base}?${params.toString()}`

  const webview = new WebviewWindow(label, {
    url,
    title: opts.title ? `AI - ${opts.title}` : "AI",
    width: 480,
    height: 720,
    minWidth: 360,
    minHeight: 480,
    decorations: false,
    center: true,
    // 统一与主窗口一致，避免 wry 原生拖放拦截干扰 DOM 事件
    dragDropEnabled: false,
  })

  webview.once("tauri://error", (e) => {
    console.error("[ashell] failed to create ai window:", e)
  })
}
