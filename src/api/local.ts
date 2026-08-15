import type { GenericAbortSignal } from "axios"
import { buildWsUrl, request } from "./client"
import type { SftpListResp } from "@/types"

/** 本地 PTY 终端 WebSocket URL */
export function buildLocalTerminalWsUrl(
  query: { sid?: string; shell?: string } = {},
): Promise<string> {
  return buildWsUrl("/api/local/terminal", query)
}

/** 列本地目录（SFTP 双栏的本地侧）。path 为空 = 用户家目录。
 *  响应结构复用 SftpListResp（后端本地栏与远程同构）。 */
export function listLocalFs(path?: string): Promise<SftpListResp> {
  return request<SftpListResp>("/api/local/fs/list", {
    params: { path: path || undefined },
  })
}

/** 列本地"根"：Windows 为所有盘符（C:、D:…），Unix 为 "/"。
 *  响应 path 为空串，前端显示"此电脑"。 */
export function listLocalFsRoots(): Promise<SftpListResp> {
  return request<SftpListResp>("/api/local/fs/roots")
}

/** 远端文件直接流式落盘到本地目录（跳过前端 blob 与另存为对话框）。
 *  同名本地文件会被覆盖；返回写入字节数。
 *  传 taskId 时后端按该 id 记账进度，可用 transferProgress 轮询。 */
export function downloadToLocal(
  sid: string,
  remotePath: string,
  localDir: string,
  opts: { signal?: GenericAbortSignal; taskId?: string } = {},
): Promise<{ bytes: number }> {
  return request<{ bytes: number }>("/api/local/fs/download_to_local", {
    method: "POST",
    json: {
      sid,
      remote_path: remotePath,
      local_dir: localDir,
      task_id: opts.taskId,
    },
    // 传输在 Rust 进程内进行，耗时随文件大小增长；关闭 axios 默认 30s 超时
    timeout: 0,
    signal: opts.signal,
  })
}

/** 本地文件流式直传到远端（"上传 ->"按钮；Rust 进程内中转，
 *  webview 无法从路径构造 File 对象，大文件也不进浏览器内存）。
 *  同名远端文件会被覆盖；返回写入字节数。
 *  传 taskId 时后端按该 id 记账进度，可用 transferProgress 轮询。 */
export function uploadLocalToRemote(
  sid: string,
  localPath: string,
  remotePath: string,
  opts: { signal?: GenericAbortSignal; taskId?: string } = {},
): Promise<{ bytes: number }> {
  return request<{ bytes: number }>("/api/local/fs/upload_to_remote", {
    method: "POST",
    json: {
      sid,
      local_path: localPath,
      remote_path: remotePath,
      task_id: opts.taskId,
    },
    // 传输在 Rust 进程内进行，耗时随文件大小增长；关闭 axios 默认 30s 超时
    timeout: 0,
    signal: opts.signal,
  })
}

/** 轮询 Rust 进程内直传任务的已传字节数（task_id -> bytes）。
 *  未知 id（未开始 / 已结束）不出现在结果里。 */
export function transferProgress(
  taskIds: string[],
): Promise<Record<string, number>> {
  return request<Record<string, number>>("/api/local/fs/progress", {
    method: "POST",
    json: { task_ids: taskIds },
  })
}

/** 把 OS 拖入的文件保存到本地目录（双栏：拖放落在本地栏）。
 *  webview 只能拿到 File 对象，字节流经本地 HTTP（axios multipart，XHR）回传，
 *  name 可含子目录（"sub/a.txt"，父目录自动创建）；同名本地文件覆盖。
 *  进度走 XHR onUploadProgress（不能用 fetch+ReadableStream：Chromium 会强制
 *  HTTP/2，与本地明文 HTTP/1.1 后端 ALPN 协商失败）。 */
export function saveLocalFile(
  dir: string,
  name: string,
  file: File,
  opts: {
    signal?: GenericAbortSignal
    onProgress?: (loaded: number, total: number) => void
  } = {},
): Promise<{ bytes: number }> {
  const fd = new FormData()
  fd.append("file", file, file.name)
  return request<{ bytes: number }>("/api/local/fs/save_file", {
    method: "POST",
    params: { dir, name },
    body: fd,
    timeout: 0,
    signal: opts.signal,
    onUploadProgress: opts.onProgress
      ? (e) => opts.onProgress?.(e.loaded, e.total ?? file.size)
      : undefined,
  })
}
