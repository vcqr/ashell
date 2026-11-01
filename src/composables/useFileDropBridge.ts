import { getCurrentWebview } from "@tauri-apps/api/webview"
import { isTauri } from "@/utils/platform"

/** 落点路由载荷：CSS 像素坐标 + 拖入对象的真实磁盘路径 */
interface FileDropPayload {
  x: number
  y: number
  paths: string[]
}

/** drop 按落点元素冒泡派发的 CustomEvent 名（TerminalView 等组件监听） */
export const DROP_EVENT = "ashell-drop"
/** 悬停/离开用 window 级 CustomEvent（leave 无落点，无法按元素路由） */
export const DRAG_OVER_EVENT = "ashell-drag-over"
export const DRAG_LEAVE_EVENT = "ashell-drag-leave"

declare global {
  interface Window {
    /** 拖放路径回程派发器：定位落点元素并派发 ashell-drop CustomEvent */
    __ashellFileDrop?: (payload: FileDropPayload) => void
  }
}

let installed = false

/**
 * OS 拖文件到终端 / SFTP 面板 → 按落点派发路径的桥（全平台统一）。
 *
 * 各平台 WebView 的 HTML5 drop 事件都拿不到文件完整路径（File.path 是
 * Electron 专有，WKWebView / WebView2 的 DOM File 均不提供），因此统一启用
 * Tauri 原生 drag-drop handler（tauri.conf.json 与 newWindow.ts 的
 * dragDropEnabled），由 wry 在 NSDraggingInfo / IDropTarget 层读出真实路径，
 * 以 tauri://drag-drop 事件（paths + 相对 webview 左上角的物理像素 position）
 * 发给页面，这里换算坐标后回程派发。
 *
 * 注意：原生 handler 会拦截窗口内一切拖拽，桌面端 HTML5 dnd（dragover/drop）
 * 不再触发——各组件的 HTML5 处理器仅在浏览器访问的 Web 形态生效。
 *
 * 派发形式：
 * - drop → 在落点元素上派发冒泡的 ashell-drop（detail.paths），组件按需监听；
 * - enter/over → window 级 ashell-drag-over（detail 为 CSS 像素坐标），
 *   leave → window 级 ashell-drag-leave，供 SFTP 面板等做悬停提示遮罩。
 */
export function installFileDropBridge(): void {
  if (installed || !isTauri) return
  installed = true

  const toCssPoint = (px: number, py: number) => {
    const dpr = window.devicePixelRatio || 1
    return { x: px / dpr, y: py / dpr }
  }

  window.__ashellFileDrop = ({ x, y, paths }) => {
    const el = document.elementFromPoint(x, y)
    el?.dispatchEvent(
      new CustomEvent(DROP_EVENT, { detail: { paths }, bubbles: true }),
    )
  }

  void getCurrentWebview()
    .onDragDropEvent((event) => {
      const p = event.payload
      if (p.type === "drop") {
        const { x, y } = toCssPoint(p.position.x, p.position.y)
        window.__ashellFileDrop?.({ x, y, paths: p.paths })
      } else if (p.type === "enter" || p.type === "over") {
        window.dispatchEvent(
          new CustomEvent(DRAG_OVER_EVENT, {
            detail: toCssPoint(p.position.x, p.position.y),
          }),
        )
      } else {
        window.dispatchEvent(new CustomEvent(DRAG_LEAVE_EVENT))
      }
    })
    .catch((e) => {
      console.warn("[ashell] drag-drop listener install failed:", e)
    })
}
