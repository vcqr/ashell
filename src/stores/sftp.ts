import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { TransferTask } from '@/types'

/** SFTP 跨组件状态：每个 sid 的当前路径 + 上传/下载任务表 */
export const useSftpStore = defineStore('sftp', () => {
  /** sid -> 当前目录 */
  const currentPath = ref<Record<string, string>>({})
  /** sid -> 上传任务 */
  const uploads = ref<Record<string, TransferTask[]>>({})
  /** sid -> 下载任务 */
  const downloads = ref<Record<string, TransferTask[]>>({})

  function setPath(sid: string, path: string) {
    currentPath.value = { ...currentPath.value, [sid]: path }
  }

  function getPath(sid: string): string {
    return currentPath.value[sid] ?? '/'
  }

  function listUploads(sid: string): TransferTask[] {
    return uploads.value[sid] ?? []
  }

  function listDownloads(sid: string): TransferTask[] {
    return downloads.value[sid] ?? []
  }

  function addUpload(sid: string, task: TransferTask) {
    const next = uploads.value[sid] ? [...uploads.value[sid]!] : []
    next.push(task)
    uploads.value = { ...uploads.value, [sid]: next }
  }

  function addDownload(sid: string, task: TransferTask) {
    const next = downloads.value[sid] ? [...downloads.value[sid]!] : []
    next.push(task)
    downloads.value = { ...downloads.value, [sid]: next }
  }

  function updateUpload(sid: string, id: string, patch: Partial<TransferTask>) {
    const list = uploads.value[sid]
    if (!list) return
    const idx = list.findIndex((t) => t.id === id)
    if (idx < 0) return
    const updated = [...list]
    updated[idx] = { ...list[idx]!, ...patch }
    uploads.value = { ...uploads.value, [sid]: updated }
  }

  function updateDownload(sid: string, id: string, patch: Partial<TransferTask>) {
    const list = downloads.value[sid]
    if (!list) return
    const idx = list.findIndex((t) => t.id === id)
    if (idx < 0) return
    const updated = [...list]
    updated[idx] = { ...list[idx]!, ...patch }
    downloads.value = { ...downloads.value, [sid]: updated }
  }

  function clearTransfers(sid: string) {
    if (uploads.value[sid]) {
      const next = { ...uploads.value }
      delete next[sid]
      uploads.value = next
    }
    if (downloads.value[sid]) {
      const next = { ...downloads.value }
      delete next[sid]
      downloads.value = next
    }
  }

  /** 仅清除已结束（done/error/cancelled）的上传任务，保留 running/pending */
  function clearFinishedUploads(sid: string) {
    const list = uploads.value[sid]
    if (!list) return
    const kept = list.filter(
      (t) => t.status === "running" || t.status === "pending",
    )
    if (kept.length === list.length) return
    uploads.value = { ...uploads.value, [sid]: kept }
  }

  /** 仅清除已结束（done/error/cancelled）的下载任务，保留 running/pending */
  function clearFinishedDownloads(sid: string) {
    const list = downloads.value[sid]
    if (!list) return
    const kept = list.filter(
      (t) => t.status === "running" || t.status === "pending",
    )
    if (kept.length === list.length) return
    downloads.value = { ...downloads.value, [sid]: kept }
  }

  function clearSession(sid: string) {
    clearTransfers(sid)
    if (currentPath.value[sid]) {
      const next = { ...currentPath.value }
      delete next[sid]
      currentPath.value = next
    }
  }

  return {
    currentPath,
    uploads,
    downloads,
    setPath,
    getPath,
    listUploads,
    listDownloads,
    addUpload,
    addDownload,
    updateUpload,
    updateDownload,
    clearFinishedUploads,
    clearFinishedDownloads,
    clearTransfers,
    clearSession,
  }
})
