import { detectMac, detectWindows, isTauri } from "@/utils/platform"

/**
 * CSS 窗口圆角。macOS 的主壳窗口（主窗 / 终端 tab 新窗）经 lib.rs 转为原生
 * Titled+Overlay，系统自带圆角与阴影，这里不处理；仍需 CSS 圆角的是——
 * - macOS 独立面板小窗（sftp/ai/hostinfo）：无边框 + 透明 + CSS 圆角，
 *   系统方形阴影由 apply_native_window_frame(native=false) 关闭
 * - Windows 全部窗口：无边框拿不到系统圆角，统一 CSS 方案
 * Linux 的 GTK 透明窗口兼容性差，保持直角。
 *
 * 最大化/全屏时关闭圆角（否则屏幕四角露出透明缺口）；Win11 贴边分屏
 * 不算最大化，与系统行为一致保留圆角。
 *
 * 样式见 main.css 的 --app-window-radius / body.window-rounded。
 * 已知边界：Teleport 到 body 的 naive 浮层容器单独补了圆角；自绘的
 * position:fixed 悬浮元素若贴窗口角，需要自行适配。
 */
export function initWindowRounded(): void {
  if (!isTauri) return
  const mac = detectMac()
  if (!mac && !detectWindows()) return
  document.body.classList.add(mac ? "platform-mac" : "platform-windows")

  // mac 上仅独立面板小窗走 CSS 圆角；主壳窗口（无 newwin 参数或
  // kind=host/local）由原生样式接管，CSS 圆角会造成双重圆角缝隙
  const kind = new URLSearchParams(window.location.search).get("kind")
  const soloPanel = kind === "sftp" || kind === "ai" || kind === "hostinfo"
  if (mac && !soloPanel) return

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
