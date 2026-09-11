import { isTauri } from "@/utils/platform"

/** 写文本到系统剪贴板（桌面 plugin-clipboard-manager / Web Clipboard API） */
export async function copyText(text: string): Promise<void> {
  if (isTauri) {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager")
    await writeText(text)
    return
  }
  await navigator.clipboard.writeText(text)
}

/** 读取系统剪贴板文本；Web 端需页面处于前台且用户授权，失败抛错由调用方兜底 */
export async function pasteText(): Promise<string> {
  if (isTauri) {
    const { readText } = await import("@tauri-apps/plugin-clipboard-manager")
    return await readText()
  }
  return await navigator.clipboard.readText()
}
