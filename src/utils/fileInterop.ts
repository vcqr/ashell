import { request } from "@/api/client"
import { isTauri } from "@/utils/platform"

/**
 * 文件对话框/下载的平台适配层。
 *
 * 桌面端走 Tauri 原生对话框 commands（保存/选择本地绝对路径）；
 * Web 端浏览器拿不到任意绝对路径，改为：
 * - 保存 → Blob + a[download]（落到浏览器默认下载目录）
 * - 选择文本文件 → input[type=file] 读内容
 * - 选择图片 → input[type=file] 拿 File（壁纸等走 multipart 上传）
 * - 选择私钥 → input[type=file] 读内容后上传到服务端，换回服务端路径
 */

/** 通用单文件选择（accept 为 input accept 属性值） */
function pickFile(accept: string): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input")
    input.type = "file"
    if (accept) input.accept = accept
    input.onchange = () => {
      resolve(input.files?.[0] ?? null)
    }
    // 取消对话框时无 change 事件，用 focus 恢复探测兜底
    window.addEventListener(
      "focus",
      () => setTimeout(() => resolve(input.files?.[0] ?? null), 300),
      { once: true },
    )
    input.click()
  })
}

/** 保存文本文件。返回保存的文件名（Web）/绝对路径（桌面）；取消返回 null */
export async function saveTextFile(
  defaultFilename: string | null,
  content: string,
): Promise<string | null> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core")
    return invoke<string | null>("save_text_file", {
      defaultFilename,
      content,
    })
  }
  const name = defaultFilename || `ashell-export-${Date.now()}.txt`
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" })
  const url = URL.createObjectURL(blob)
  const a = document.createElement("a")
  a.href = url
  a.download = name
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
  return name
}

/** 打开文本文件并读取内容；取消返回 null */
export async function openTextFile(): Promise<string | null> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core")
    return invoke<string | null>("open_text_file")
  }
  const file = await pickFile(".json,application/json,text/plain")
  if (!file) return null
  return await file.text()
}

/** 选择一张图片，返回 File（仅 Web；桌面端选路径后走 setWallpaper(path)） */
export async function pickImageFile(): Promise<File | null> {
  if (isTauri) return null
  return pickFile("image/png,image/jpeg,image/gif,image/webp,image/bmp,image/svg+xml")
}

/**
 * 选择私钥文件（仅 Web；桌面端 HostForm 直接走对话框拿路径）。
 * 浏览器拿不到本地绝对路径：读出内容上传到服务端 ~/.ashell/keys/，
 * 换回服务端绝对路径供 SSH 连接使用。
 */
export async function uploadPrivateKey(): Promise<string | null> {
  const file = await pickFile("")
  if (!file) return null
  const content = await file.arrayBuffer()
  const form = new FormData()
  form.append("file", new Blob([content]), file.name)
  const res = await request<{ path: string }>("/api/keys/upload", {
    method: "POST",
    body: form,
    timeout: 0,
  })
  return res?.path ?? null
}
