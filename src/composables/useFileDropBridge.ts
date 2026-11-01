import { invoke } from "@tauri-apps/api/core"
import { detectWindows, isTauri } from "@/utils/platform"

/** Rust file_drop_bridge eval 回页面的载荷 */
interface FileDropPayload {
  x: number
  y: number
  paths: string[]
}

declare global {
  interface Window {
    /** 由 Rust 侧 eval 调用：定位落点元素并派发 ashell-drop CustomEvent */
    __ashellFileDrop?: (payload: FileDropPayload) => void
  }
}

let installed = false

/**
 * 资源管理器拖文件到终端 → 填充路径的桥。
 *
 * WebView2 的 HTML5 drop 拿不到文件完整路径（File.path 是 Electron 专有），
 * 由 Rust 侧（src-tauri/src/file_drop_bridge.rs）经 WebMessageReceived 的
 * AdditionalObjects 拿到 ICoreWebView2File::Path 后 eval 回页面；
 * 这里负责定义回程派发器并安装后端处理器。
 *
 * 落点路由：drop 事件自带的 clientX/Y 原样传回，elementFromPoint 定位元素后
 * 派发冒泡的 ashell-drop CustomEvent（detail.paths），终端等组件按需监听。
 */
export function installFileDropBridge(): void {
  if (installed || !isTauri || !detectWindows()) return
  installed = true

  window.__ashellFileDrop = ({ x, y, paths }) => {
    const el = document.elementFromPoint(x, y)
    el?.dispatchEvent(
      new CustomEvent("ashell-drop", { detail: { paths }, bubbles: true }),
    )
  }

  void invoke("install_file_drop_bridge").catch((e) => {
    console.warn("[ashell] file drop bridge install failed:", e)
  })
}
