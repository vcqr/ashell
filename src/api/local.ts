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
 *  同名本地文件会被覆盖；返回写入字节数。 */
export function downloadToLocal(
  sid: string,
  remotePath: string,
  localDir: string,
  opts: { signal?: GenericAbortSignal } = {},
): Promise<{ bytes: number }> {
  return request<{ bytes: number }>("/api/local/fs/download_to_local", {
    method: "POST",
    json: { sid, remote_path: remotePath, local_dir: localDir },
    signal: opts.signal,
  })
}

/** 本地文件流式直传到远端（"上传 ->"按钮；Rust 进程内中转，
 *  webview 无法从路径构造 File 对象，大文件也不进浏览器内存）。
 *  同名远端文件会被覆盖；返回写入字节数。 */
export function uploadLocalToRemote(
  sid: string,
  localPath: string,
  remotePath: string,
  opts: { signal?: GenericAbortSignal } = {},
): Promise<{ bytes: number }> {
  return request<{ bytes: number }>("/api/local/fs/upload_to_remote", {
    method: "POST",
    json: { sid, local_path: localPath, remote_path: remotePath },
    signal: opts.signal,
  })
}
