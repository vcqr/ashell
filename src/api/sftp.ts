import { buildWsUrl, getAxios, request } from "./client"
import type { SftpListResp } from "@/types"

/** 判断错误是否属于"用户主动取消"（AbortController.abort / axios CanceledError）。
 *
 * client.ts 的响应拦截器已把 axios 的 CanceledError 规范化为 `DOMException('AbortError')`，
 * 因此这里覆盖两种形态：原生 AbortError（name === 'AbortError'）以及尚未被拦截器
 * 触达的极少数路径里的 axios CanceledError（带 code === 'ERR_CANCELED'）。
 *
 * 语义上对齐 demo 中 `axios.isCancel(err)` 的用法。 */
export function isAbortError(err: unknown): boolean {
  if (!err || typeof err !== "object") return false
  const e = err as { name?: string; code?: string }
  return e.name === "AbortError" || e.code === "ERR_CANCELED"
}

/** 从 Content-Disposition 头中解析出 filename（支持普通 filename= 与 RFC5987 filename*=）。 */
function parseContentDispositionFilename(header: string | undefined): string | undefined {
  if (!header) return undefined
  // 优先解析 filename*=UTF-8''xxx（RFC 5987）
  const star = /filename\*\s*=\s*(?:UTF-8'')?([^;]+)/i.exec(header)
  if (star && star[1]) {
    try {
      return decodeURIComponent(star[1].trim().replace(/^"|"$/g, ""))
    } catch {
      // fall through
    }
  }
  const plain = /filename\s*=\s*("([^"]*)"|([^;]+))/i.exec(header)
  if (plain) {
    const v = (plain[2] ?? plain[3] ?? "").trim()
    if (v) return v
  }
  return undefined
}

function rafThrottle(
  fn: (loaded: number, total: number) => void,
): (loaded: number, total: number) => void {
  let scheduled = false
  let lastLoaded = 0
  let lastTotal = 0
  return (loaded, total) => {
    lastLoaded = loaded
    lastTotal = total
    if (scheduled) return
    scheduled = true
    requestAnimationFrame(() => {
      scheduled = false
      fn(lastLoaded, lastTotal)
    })
  }
}

/** 显式打开一个独立的 SFTP 会话 */
export function openSftp(hostId: number, sid?: string): Promise<{ sid: string }> {
  return request<{ sid: string }>("/api/ssh/sftp/open", {
    method: "POST",
    json: { host_id: hostId, sid },
  })
}

export function listSftp(sid: string, path?: string): Promise<SftpListResp> {
  return request<SftpListResp>("/api/ssh/sftp", {
    params: { sid, path },
  })
}

export function mkdir(sid: string, path: string): Promise<void> {
  return request<void>("/api/ssh/sftp/mkdir", {
    method: "POST",
    json: { sid, path },
  })
}

export function touch(sid: string, path: string): Promise<void> {
  return request<void>("/api/ssh/sftp/touch", {
    method: "POST",
    json: { sid, path },
  })
}

export function removeFile(sid: string, path: string): Promise<void> {
  return request<void>("/api/ssh/sftp/remove_file", {
    method: "POST",
    json: { sid, path },
  })
}

export function removeDir(sid: string, path: string): Promise<void> {
  return request<void>("/api/ssh/sftp/remove_dir", {
    method: "POST",
    json: { sid, path },
  })
}

export function rename(sid: string, oldPath: string, newPath: string): Promise<void> {
  return request<void>("/api/ssh/sftp/rename", {
    method: "POST",
    json: { sid, old_path: oldPath, new_path: newPath },
  })
}

export function closeSftp(sid: string): Promise<void> {
  return request<void>("/api/ssh/sftp/close", {
    method: "POST",
    json: { sid },
  })
}

/** 流式下载文件
 *
 * 返回 Blob + Content-Length；通过 axios 的 onDownloadProgress 报告进度。
 * AbortSignal 可用于取消（被取消时 axios 抛 AbortError）。
 */
export interface DownloadOptions {
  signal?: AbortSignal
  onProgress?: (loaded: number, total: number) => void
}

export interface DownloadResult {
  blob: Blob
  /** 服务端 Content-Length，可能为 0（未知） */
  contentLength: number
  /** 解析自 Content-Disposition 的建议保存文件名；解析失败为 undefined */
  suggestedFilename?: string
}

export async function downloadStream(
  sid: string,
  filename: string,
  opts: DownloadOptions = {},
): Promise<DownloadResult> {
  const ins = getAxios()
  const notify = opts.onProgress ? rafThrottle(opts.onProgress) : undefined
  const resp = await ins.request<Blob>({
    url: "/api/ssh/sftp/download",
    method: "GET",
    params: { sid, filename },
    responseType: "blob",
    signal: opts.signal,
    timeout: 0,
    onDownloadProgress: notify
      ? (e) => {
          notify(e.loaded, e.total ?? 0)
        }
      : undefined,
  })
  const lenHeader = resp.headers["content-length"]
  const contentLength = typeof lenHeader === "string" ? Number(lenHeader) : 0
  const cdHeader = resp.headers["content-disposition"]
  const suggestedFilename = parseContentDispositionFilename(
    typeof cdHeader === "string" ? cdHeader : undefined,
  )
  const blob = resp.data
  if (notify && opts.onProgress) {
    const finalTotal = contentLength || blob.size
    notify(blob.size, finalTotal)
    opts.onProgress(blob.size, finalTotal)
  }
  return { blob, contentLength, suggestedFilename }
}

/** 上传：multipart/form-data，使用 axios 的 onUploadProgress（底层是 XHR） */
export interface UploadOptions {
  sid: string
  /** 远端目标完整路径 */
  filename: string
  file: File | Blob
  /** 用于 multipart 中 file 字段的名字（默认使用 file.name） */
  fieldFilename?: string
  signal?: AbortSignal
  onProgress?: (loaded: number, total: number) => void
}

export async function uploadStream(opts: UploadOptions): Promise<void> {
  const fileName =
    opts.fieldFilename ??
    ("name" in opts.file && typeof opts.file.name === "string"
      ? opts.file.name
      : "upload.bin")
  const total =
    "size" in opts.file && typeof opts.file.size === "number" ? opts.file.size : 0

  const fd = new FormData()
  fd.append("file", opts.file, fileName)

  const ins = getAxios()
  const notify = opts.onProgress ? rafThrottle(opts.onProgress) : undefined
  await ins.request({
    url: "/api/ssh/sftp/upload",
    method: "POST",
    params: { sid: opts.sid, filename: opts.filename },
    data: fd,
    signal: opts.signal,
    timeout: 0,
    onUploadProgress: notify
      ? (e) => {
          notify(e.loaded, e.total ?? total)
        }
      : undefined,
  })
  if (notify && opts.onProgress) {
    notify(total, total)
    opts.onProgress(total, total)
  }
}

/** 读取远程文本文件内容（复用 download 流，转 text） */
export async function readText(sid: string, filename: string): Promise<string> {
  const { blob } = await downloadStream(sid, filename)
  return blob.text()
}

/** 将文本内容写回远程文件（复用 upload 流，用 Blob 包装） */
export async function writeText(
  sid: string,
  filename: string,
  content: string,
): Promise<void> {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" })
  const basename = filename.split("/").pop() || "edit.bin"
  await uploadStream({ sid, filename, file: blob, fieldFilename: basename })
}

/** 终端 WebSocket URL */
export function buildTerminalWsUrl(
  hostId: number,
  query: { sid?: string; cols?: number; rows?: number; term?: string } = {},
): Promise<string> {
  return buildWsUrl(`/api/ssh/terminal/${hostId}`, query)
}
