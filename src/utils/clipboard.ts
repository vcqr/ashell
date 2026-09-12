import { isTauri } from "@/utils/platform"

/** 非安全上下文（http://IP 访问）下 navigator.clipboard 不可用时的复制降级 */
function legacyCopy(text: string): void {
  const ta = document.createElement("textarea")
  ta.value = text
  ta.style.position = "fixed"
  ta.style.opacity = "0"
  document.body.appendChild(ta)
  ta.select()
  try {
    if (!document.execCommand("copy")) {
      throw new Error("execCommand copy returned false")
    }
  } finally {
    ta.remove()
  }
}

/** 写文本到系统剪贴板（桌面 plugin-clipboard-manager / Web Clipboard API） */
export async function copyText(text: string): Promise<void> {
  if (isTauri) {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager")
    await writeText(text)
    return
  }
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
    return
  }
  // 非安全上下文（局域网 http://IP）：降级 execCommand
  legacyCopy(text)
}

/**
 * 读取系统剪贴板文本。
 * 桌面端经 plugin-clipboard-manager（免 WKWebView 授权提示）；
 * Web 端需页面处于前台且为安全上下文（HTTPS/localhost），失败抛错由调用方兜底。
 */
export async function pasteText(): Promise<string> {
  if (isTauri) {
    const { readText } = await import("@tauri-apps/plugin-clipboard-manager")
    return await readText()
  }
  return await navigator.clipboard.readText()
}
