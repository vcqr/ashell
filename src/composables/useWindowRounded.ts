import { detectMac, detectWindows, isTauri } from "@/utils/platform"

/**
 * 无边框窗口圆角。decorations:false 的窗口拿不到系统圆角
 * （Win11 自动圆角只给标准标题栏窗口，macOS borderless NSWindow 是方角），
 * 统一用「窗口透明 + body CSS 圆角」实现：
 * - 仅 macOS / Windows 启用；Linux 的 GTK 透明窗口兼容性差，保持直角
 * - macOS 绿钮缩放（isMaximized）下原生窗口本就保留圆角，这里跟随系统
 *   行为不清圆角；仅真全屏关闭。Windows 最大化按系统约定去圆角，
 *   贴边分屏（Win+方向键）不算最大化，保留圆角
 * - 独立窗口（终端/SFTP/AI/主机信息）由 newWindow.ts 开 transparent，
 *   圆角样式同一套 body class 生效
 *
 * 样式见 main.css 的 --app-window-radius / body.window-rounded。
 * 已知边界：Teleport 到 body 的 naive 浮层容器单独补了圆角；自绘的
 * position:fixed 悬浮元素若贴窗口角，需要自行适配。
 */
export function initWindowRounded(): void {
  if (!isTauri) return
  const mac = detectMac()
  if (mac) {
    document.body.classList.add("platform-mac")
  } else if (detectWindows()) {
    document.body.classList.add("platform-windows")
  } else {
    return
  }

  void (async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window")
    const win = getCurrentWindow()
    const apply = async () => {
      const [maximized, fullscreen] = await Promise.all([
        win.isMaximized(),
        win.isFullscreen(),
      ])
      const rounded = mac ? !fullscreen : !fullscreen && !maximized
      document.body.classList.toggle("window-rounded", rounded)
    }
    await apply()
    await win.onResized(() => void apply())
  })()
}
