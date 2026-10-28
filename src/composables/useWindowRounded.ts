import { isTauri } from "@/utils/platform"

/**
 * 无边框窗口圆角（社区方案，jishuzhan.net/article/2062848427322585089）。
 * 窗口配置透明 + shadow:false + decorations:false（tauri.conf.json 主窗 /
 * newWindow.ts 独立窗），WebView 背景透明（lib.rs set_background_color），
 * 圆角与阴影由 CSS 承担（main.css body.window-rounded + ::after）。
 *
 * 本 composable 只负责一件事：真全屏时摘掉 body.window-rounded——全屏下
 * 窗口铺满整屏，CSS 圆角会在屏幕四角露出透明缺口（系统惯例此时也是方角）。
 * 最大化 / 绿钮缩放保持圆角（与原生应用行为一致，用户明确要求）；
 * 贴边分屏同理保留。
 */
export function initWindowRounded(): void {
  if (!isTauri) return

  void (async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window")
    const win = getCurrentWindow()
    const apply = async () => {
      const fullscreen = await win.isFullscreen()
      document.body.classList.toggle("window-rounded", !fullscreen)
    }
    await apply()
    await win.onResized(() => void apply())
  })()
}
