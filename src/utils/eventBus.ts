import { isTauri } from "@/utils/platform"

/** 取消监听函数（与 @tauri-apps/api/event 的 UnlistenFn 同形） */
export type UnlistenFn = () => void

/**
 * 跨窗口/标签页事件总线适配层。
 *
 * 桌面端：Tauri event（emit 广播到所有窗口，多窗口独立 Vue 实例）。
 * Web 端：BroadcastChannel（同源所有标签页广播，浏览器不回投给发送方，
 *         与 Tauri 事件语义天然一致）。
 *
 * 用途：多 Tab 广播输入（broadcast store）、AI sidecar 事件（ai store 已改走
 * 专用 WS，不经此总线）。
 */
let webChannel: BroadcastChannel | null = null

function channel(): BroadcastChannel {
  if (!webChannel) {
    webChannel = new BroadcastChannel("ashell:events")
    window.addEventListener("beforeunload", () => {
      webChannel?.close()
      webChannel = null
    })
  }
  return webChannel
}

/** 订阅事件；Web 端事件名与桌面端保持一致 */
export async function busListen<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event")
    return listen<T>(event, (e) => handler(e.payload))
  }
  const fn = (e: MessageEvent) => {
    const data = e.data as { event?: string; payload?: unknown } | null
    if (data && data.event === event) {
      handler(data.payload as T)
    }
  }
  channel().addEventListener("message", fn)
  return () => channel().removeEventListener("message", fn)
}

/** 广播事件到所有窗口/标签页 */
export function busEmit<T>(event: string, payload: T): void {
  if (isTauri) {
    void import("@tauri-apps/api/event").then(({ emit }) => emit(event, payload))
    return
  }
  try {
    channel().postMessage({ event, payload })
  } catch {
    // channel 已关闭等场景静默忽略
  }
}
