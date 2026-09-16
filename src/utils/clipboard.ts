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

/**
 * 清洗待粘贴文本：剥掉 ANSI 转义序列与其它控制字符（保留 \r \n \t）。
 *
 * 从带颜色的终端输出、保留了 ESC 字节的日志文件（tee 捕获的彩色输出）、
 * 堡垒机页面等复制的内容常混有字面 ESC 字节；粘贴后这些字节随 shell 回显
 * 被 xterm 当成 CSI/SGR 解释——典型表现是 `\x1b[7m` 之类把粘贴内容反显成
 * "浅底色块"，文字与底色相近时完全看不清。转义序列还可能携带隐藏指令
 * （终端注入），vim 插入模式里混入的 ESC 也会打断输入。统一在粘贴入口剥掉。
 */
export function sanitizePastedText(text: string): string {
  return text
    // OSC/DCS/PM/APC 字符串序列：ESC ]/P/X/^/_ … BEL 或 ST；无终止符则吃到串尾
    .replace(/\x1b[\]PX^_][^\u0007\u001b\u009c]*(?:\u0007|\u001b\\|\u009c)?/g, "")
    // CSI 序列：ESC [ 参数/中间字节 … 最终字节（SGR 颜色、光标移动等）
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    // 其余 ESC 序列（带中间字节的两/三字节形式，以及串尾截断的孤立 ESC）
    .replace(/\x1b[ -/]*[@-~]?/g, "")
    // C1 控制字符（0x9b 在部分解析路径下等同 CSI）
    .replace(/[\u0080-\u009f]/g, "")
    // 其余 C0 控制字符与 DEL（\r \n \t 保留，交给 term.paste 规范化并做 bracketed paste 包裹）
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, "")
}
