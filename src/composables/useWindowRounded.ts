import { detectWindows, isTauri } from "@/utils/platform"

/**
 * 无边框窗口圆角（社区方案，jishuzhan.net/article/2062848427322585089）。
 * 窗口配置透明 + shadow:false + decorations:false（tauri.conf.json 主窗 /
 * newWindow.ts 独立窗），WebView 背景透明（lib.rs set_background_color），
 * 圆角与阴影由 CSS 承担（main.css body.window-rounded + ::after）。
 *
 * 本 composable 负责：摘掉 body.window-rounded 的时机——
 * - 真全屏（各平台）：窗口铺满整屏，CSS 圆角会在屏幕四角露出透明缺口；
 * - 最大化（仅 Windows）：原生惯例最大化贴满屏幕即为直角，圆角会顶出
 *   屏幕边框感；macOS 屏幕四角本身带圆弧，最大化 / 绿钮缩放保留圆角
 *   （与原生应用行为一致，用户明确要求）。贴边分屏不视作最大化，保留圆角。
 */
export function initWindowRounded(): void {
  if (!isTauri) return

  void (async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window")
    const win = getCurrentWindow()
    const squareWhenMaximized = detectWindows()
    let seq = 0
    const apply = async () => {
      const id = ++seq
      const fullscreen = await win.isFullscreen()
      const maximized = squareWhenMaximized && (await win.isMaximized())
      // 最大化会连发多个 resize 事件，IPC 查询可能交错返回，旧结果覆盖新状态
      if (id !== seq) return
      document.body.classList.toggle("window-rounded", !fullscreen && !maximized)
    }
    await apply()
    await win.onResized(() => void apply())
  })()
}
