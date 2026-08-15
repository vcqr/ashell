<script setup lang="ts">
import { computed, h, onBeforeUnmount, ref, watch } from "vue"
import {
  NBadge,
  NButton,
  NDataTable,
  NDropdown,
  NEmpty,
  NIcon,
  NInput,
  NModal,
  NSpace,
  NSpin,
  NTooltip,
  NUpload,
  useDialog,
  useMessage,
} from "naive-ui"
import type { DataTableColumns, InputInst, UploadCustomRequestOptions } from "naive-ui"
import {
  ArrowUpOutline,
  ArrowBackOutline,
  ArrowForwardOutline,
  CloseOutline,
  CloudUploadOutline,
  CopyOutline,
  CreateOutline,
  DocumentOutline,
  DownloadOutline,
  EyeOutline,
  FolderOpenOutline,
  FolderOutline,
  LaptopOutline,
  LinkOutline,
  RefreshOutline,
  SendOutline,
  SparklesOutline,
  TrashOutline,
} from "@vicons/ionicons5"
import {
  downloadStream,
  isAbortError,
  listSftp,
  mkdir as mkdirApi,
  removeDir,
  removeFile,
  rename as renameApi,
  touch as touchApi,
  uploadStream,
} from "@/api/sftp"
import { useI18n } from "vue-i18n"
import { useSftpStore } from "@/stores/sftp"
import { useStartupStore } from "@/stores/startup"
import type { SftpFile, TransferTask } from "@/types"
import { humanSize } from "@/utils/humanSize"
import { joinPath, normalizePath, parentPath } from "@/utils/pathJoin"
import { formatUnix } from "@/utils/time"
import MkdirDialog from "@/components/sftp/MkdirDialog.vue"
import RenameDialog from "@/components/sftp/RenameDialog.vue"
import SftpUploadList from "@/components/sftp/SftpUploadList.vue"
import SftpDownloadList from "@/components/sftp/SftpDownloadList.vue"
import FileEditor from "@/components/sftp/FileEditor.vue"
import FilePreview from "@/components/sftp/FilePreview.vue"
import LocalPane from "@/components/sftp/LocalPane.vue"
import { isPreviewable } from "@/utils/fileType"
import { downloadToLocal, uploadLocalToRemote } from "@/api/local"
import { useFileDrag } from "@/composables/useFileDrag"

interface Props {
  open: boolean
  sid: string | null
  hostName?: string
  hostAddr?: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
  "send-to-ai": [text: string]
}>()

const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()
const store = useSftpStore()
const startupStore = useStartupStore()

const loading = ref(false)
const files = ref<SftpFile[]>([])
const currentPath = ref<string>("/")
const selectedKey = ref<string | null>(null)

const mkdirOpen = ref(false)
const mkdirMode = ref<"mkdir" | "touch">("mkdir")
const renameOpen = ref(false)
const renameTarget = ref<SftpFile | null>(null)

const pathEditing = ref(false)
const pathDraft = ref("")
const pathInputRef = ref<InputInst | null>(null)

const uploadModalOpen = ref(false)
const downloadModalOpen = ref(false)

const editorOpen = ref(false)
const editorFile = ref<SftpFile | null>(null)

const previewOpen = ref(false)
const previewFile = ref<SftpFile | null>(null)

const ctxMenuVisible = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuTarget = ref<SftpFile | null>(null)

const panelRef = ref<HTMLElement | null>(null)

// ===== 发送给 AI 提示词浮层 =====
const aiPromptVisible = ref(false)
const aiPromptText = ref("")
const aiPromptX = ref(0)
const aiPromptY = ref(0)
const aiSelectionText = ref("")

const uploads = computed<TransferTask[]>(() =>
  props.sid ? store.listUploads(props.sid) : [],
)
const downloads = computed<TransferTask[]>(() =>
  props.sid ? store.listDownloads(props.sid) : [],
)

const activeUploadCount = computed(
  () =>
    uploads.value.filter((t) => t.status === "running" || t.status === "pending")
      .length,
)
const activeDownloadCount = computed(
  () =>
    downloads.value.filter(
      (t) => t.status === "running" || t.status === "pending",
    ).length,
)

const finishedUploadCount = computed(
  () =>
    uploads.value.filter(
      (t) => t.status === "done" || t.status === "error" || t.status === "cancelled",
    ).length,
)
const finishedDownloadCount = computed(
  () =>
    downloads.value.filter(
      (t) => t.status === "done" || t.status === "error" || t.status === "cancelled",
    ).length,
)

function clearFinishedUploads() {
  if (!props.sid) return
  store.clearFinishedUploads(props.sid)
}

function clearFinishedDownloads() {
  if (!props.sid) return
  store.clearFinishedDownloads(props.sid)
}

function startPathEdit() {
  if (!props.sid) return
  pathDraft.value = currentPath.value
  pathEditing.value = true
  void Promise.resolve().then(() => {
    pathInputRef.value?.focus()
    pathInputRef.value?.select()
  })
}

function cancelPathEdit() {
  pathEditing.value = false
  pathDraft.value = ""
}

function submitPathEdit() {
  if (!pathEditing.value) return
  const next = normalizePath(pathDraft.value || "/")
  pathEditing.value = false
  pathDraft.value = ""
  if (next === currentPath.value) return
  void load(next)
}

function onPathKeydown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault()
    submitPathEdit()
  } else if (e.key === "Escape") {
    e.preventDefault()
    cancelPathEdit()
  }
}

async function copyCurrentPath() {
  try {
    await navigator.clipboard.writeText(currentPath.value)
    message.success(t("sftp.message.copied"))
  } catch {
    message.error(t("sftp.message.copyFailed"))
  }
}

const drawerTitle = computed(() => {
  const host = props.hostName ?? "SFTP"
  const addr = props.hostAddr?.trim()
  return addr ? `${host} (${addr})` : host
})

/* ---------- data loading ---------- */

async function load(path?: string) {
  if (!props.sid) return
  const sid = props.sid
  loading.value = true
  try {
    const target = path !== undefined ? normalizePath(path) : currentPath.value
    const resp = await listSftp(sid, target)
    const sorted = [...resp.files].sort((a, b) => {
      // 目录优先；同类按名字
      const da = a.file_type === "dir" ? 0 : 1
      const db = b.file_type === "dir" ? 0 : 1
      if (da !== db) return da - db
      return a.file_name.toLowerCase().localeCompare(b.file_name.toLowerCase())
    })
    files.value = sorted
    currentPath.value = resp.path || target
    selectedKey.value = null
    store.setPath(sid, currentPath.value)
  } catch (e) {
    message.error(t("sftp.message.loadFailed", { error: (e as Error).message }))
  } finally {
    loading.value = false
  }
}

function refresh() {
  void load()
}

function goUp() {
  void load(parentPath(currentPath.value))
}

function enterDir(file: SftpFile) {
  if (file.file_type === "dir" || file.file_type === "symlink") {
    void load(file.full_path)
  }
}

/* ---------- 创建 / 重命名 / 删除 ---------- */

function openMkdir() {
  mkdirMode.value = "mkdir"
  mkdirOpen.value = true
}

function openTouch() {
  mkdirMode.value = "touch"
  mkdirOpen.value = true
}

async function onMkdirSubmit(name: string) {
  if (!props.sid) return
  const target = joinPath(currentPath.value, name)
  try {
    if (mkdirMode.value === "mkdir") {
      await mkdirApi(props.sid, target)
      message.success(t("sftp.message.folderCreated"))
    } else {
      await touchApi(props.sid, target)
      message.success(t("sftp.message.fileCreated"))
    }
    mkdirOpen.value = false
    await load()
  } catch (e) {
    message.error(t("sftp.message.createFailed", { error: (e as Error).message }))
  }
}

function openRename(file: SftpFile) {
  renameTarget.value = file
  renameOpen.value = true
}

async function onRenameSubmit(newName: string) {
  if (!props.sid || !renameTarget.value) return
  const oldPath = renameTarget.value.full_path
  const newPath = joinPath(parentPath(oldPath), newName)
  try {
    await renameApi(props.sid, oldPath, newPath)
    message.success(t("sftp.message.renameSuccess"))
    renameOpen.value = false
    renameTarget.value = null
    await load()
  } catch (e) {
    message.error(t("sftp.message.renameFailed", { error: (e as Error).message }))
  }
}

async function onRemove(file: SftpFile) {
  if (!props.sid) return
  try {
    if (file.file_type === "dir") {
      await removeDir(props.sid, file.full_path)
    } else {
      await removeFile(props.sid, file.full_path)
    }
    message.success(t("sftp.message.deleted"))
    await load()
  } catch (e) {
    message.error(t("sftp.message.deleteFailed", { error: (e as Error).message }))
  }
}

function confirmRemove(file: SftpFile) {
  dialog.warning({
    title: t("sftp.dialog.deleteTitle"),
    content: t("sftp.dialog.deleteConfirm", {
      type: file.file_type === "dir" ? t("sftp.dialog.typeFolder") : t("sftp.dialog.typeFile"),
      name: file.file_name,
    }),
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: () => {
      void onRemove(file)
    },
  })
}

/* ---------- 在线编辑 ---------- */

function openEditor(file: SftpFile) {
  editorFile.value = file
  editorOpen.value = true
}

function openPreview(file: SftpFile) {
  previewFile.value = file
  previewOpen.value = true
}

function onEditorSaved() {
  void load()
}

/* ---------- 下载 ---------- */

function genId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

async function onDownload(file: SftpFile) {
  if (!props.sid) return
  if (file.file_type !== "file") {
    message.warning(t("sftp.message.downloadOnlyFile"))
    return
  }
  // 双栏模式：直接落盘到本地栏当前目录，不弹另存为对话框
  if (dualPane.value && localDir.value) {
    await downloadToLocalDir(file)
    return
  }
  const confirmed = await new Promise<boolean>((resolve) => {
    dialog.info({
      title: t("sftp.dialog.downloadTitle"),
      content: t("sftp.dialog.downloadConfirm", { name: file.file_name }),
      positiveText: t("common.download"),
      negativeText: t("common.cancel"),
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false),
    })
  })
  if (!confirmed) return
  const sid = props.sid
  const ctrl = new AbortController()
  const taskId = genId()
  const task: TransferTask = {
    id: taskId,
    sid,
    filename: file.full_path,
    total: file.size_bytes ?? 0,
    loaded: 0,
    status: "running",
    controller: ctrl,
    startedAt: Date.now(),
  }
  store.addDownload(sid, task)
  try {
    const { blob, contentLength, suggestedFilename } = await downloadStream(
      sid,
      file.full_path,
      {
        signal: ctrl.signal,
        onProgress: (loaded, total) => {
          store.updateDownload(sid, taskId, {
            loaded,
            total: total > 0 ? total : loaded,
          })
        },
      },
    )
    const total = contentLength > 0 ? contentLength : blob.size
    const url = URL.createObjectURL(blob)
    const a = document.createElement("a")
    a.href = url
    a.download = suggestedFilename || file.file_name
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(url)
    store.updateDownload(sid, taskId, {
      loaded: blob.size,
      total,
      status: "done",
    })
    message.success(t("sftp.message.downloaded", { name: suggestedFilename || file.file_name }))
  } catch (e) {
    if (isAbortError(e)) {
      store.updateDownload(sid, taskId, { status: "cancelled" })
    } else {
      const err = e as Error
      store.updateDownload(sid, taskId, { status: "error", error: err.message })
      message.error(t("sftp.message.downloadFailed", { error: err.message }))
    }
  }
}

/** 双栏模式：远端文件直落本地栏当前目录（Rust 进程内流式写盘） */
async function downloadToLocalDir(file: SftpFile) {
  if (!props.sid) return
  const sid = props.sid
  const ctrl = new AbortController()
  const taskId = genId()
  const task: TransferTask = {
    id: taskId,
    sid,
    filename: file.full_path,
    remoteDir: currentPath.value,
    total: file.size_bytes ?? 0,
    loaded: 0,
    status: "running",
    controller: ctrl,
    startedAt: Date.now(),
  }
  store.addDownload(sid, task)
  try {
    const { bytes } = await downloadToLocal(sid, file.full_path, localDir.value, {
      signal: ctrl.signal,
    })
    const total = bytes > 0 ? bytes : (file.size_bytes ?? 0)
    store.updateDownload(sid, taskId, { loaded: total, total, status: "done" })
    message.success(t("sftp.message.downloadedTo", { dir: localDir.value }))
    localPaneRef.value?.refresh()
  } catch (e) {
    if (isAbortError(e)) {
      store.updateDownload(sid, taskId, { status: "cancelled" })
    } else {
      const err = e as Error
      store.updateDownload(sid, taskId, { status: "error", error: err.message })
      message.error(t("sftp.message.downloadFailed", { error: err.message }))
    }
  }
}

/** 双栏模式：本地栏勾选的文件直传远程当前目录（Rust 进程内流式中转） */
async function onLocalUpload(sel: SftpFile[]) {
  if (!props.sid || sel.length === 0) return
  const sid = props.sid

  // 同名覆盖确认：只问一次，列出冲突名
  const names = new Set(sel.map((f) => f.file_name.toLowerCase()))
  const conflicts = files.value
    .filter((f) => names.has(f.file_name.toLowerCase()))
    .map((f) => f.file_name)
  if (conflicts.length > 0) {
    const preview = conflicts.slice(0, 5).join(", ")
    const more = conflicts.length > 5 ? ` …(+${conflicts.length - 5})` : ""
    const ok = await new Promise<boolean>((resolve) => {
      dialog.warning({
        title: t("sftp.dialog.overwriteTitle"),
        content: `${preview}${more}`,
        positiveText: t("common.overwrite"),
        negativeText: t("common.cancel"),
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
        onMaskClick: () => resolve(false),
      })
    })
    if (!ok) return
  }

  for (const f of sel) {
    if (f.file_type !== "file") continue
    const remotePath = joinPath(currentPath.value, f.file_name)
    const ctrl = new AbortController()
    const taskId = genId()
    const total = f.size_bytes ?? 0
    const task: TransferTask = {
      id: taskId,
      sid,
      filename: remotePath,
      remoteDir: currentPath.value,
      total,
      loaded: 0,
      status: "running",
      controller: ctrl,
      startedAt: Date.now(),
    }
    store.addUpload(sid, task)
    try {
      await uploadLocalToRemote(sid, f.full_path, remotePath, { signal: ctrl.signal })
      // JSON 请求没有上传进度回调，完成时一次性置满
      store.updateUpload(sid, taskId, { loaded: total, total, status: "done" })
    } catch (e) {
      if (isAbortError(e)) {
        store.updateUpload(sid, taskId, { status: "cancelled" })
      } else {
        const err = e as Error
        store.updateUpload(sid, taskId, { status: "error", error: err.message })
        message.error(t("sftp.message.uploadFailed", { error: err.message }))
      }
    }
  }
  await load()
}

function cancelDownload(id: string) {
  if (!props.sid) return
  const t = store.listDownloads(props.sid).find((x) => x.id === id)
  if (!t) return
  t.controller?.abort()
  store.updateDownload(props.sid, id, { status: "cancelled" })
}

/* ---------- 上传 ---------- */

/** 上传单个文件到当前目录（含同名覆盖确认、任务进度条、取消）。
 *  按钮上传与 OS 拖放共用此通道。返回是否实际完成上传。 */
async function uploadOneFile(file: File): Promise<boolean> {
  if (!props.sid) return false
  const sid = props.sid
  // 同名覆盖确认（与 demo 保持一致：基于当前目录已加载的文件列表判定）
  const exists = files.value.some(
    (f) => f.file_name.toLowerCase() === file.name.toLowerCase(),
  )
  if (exists) {
    const ok = await new Promise<boolean>((resolve) => {
      dialog.warning({
        title: t("sftp.dialog.overwriteTitle"),
        content: t("sftp.dialog.overwriteConfirm", { name: file.name }),
        positiveText: t("common.overwrite"),
        negativeText: t("common.cancel"),
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
        onMaskClick: () => resolve(false),
      })
    })
    if (!ok) return false
  }
  const ctrl = new AbortController()
  const taskId = genId()
  const remotePath = joinPath(currentPath.value, file.name)
  const task: TransferTask = {
    id: taskId,
    sid,
    filename: remotePath,
    remoteDir: currentPath.value,
    total: file.size,
    loaded: 0,
    status: "running",
    controller: ctrl,
    startedAt: Date.now(),
  }
  store.addUpload(sid, task)
  try {
    await uploadStream({
      sid,
      filename: remotePath,
      file,
      signal: ctrl.signal,
      onProgress: (loaded, total) => {
        store.updateUpload(sid, taskId, {
          loaded,
          total: total > 0 ? total : loaded,
        })
      },
    })
    store.updateUpload(sid, taskId, {
      loaded: file.size,
      total: file.size,
      status: "done",
    })
    message.success(t("sftp.message.uploaded", { name: file.name }))
    await load()
    return true
  } catch (e) {
    if (isAbortError(e)) {
      store.updateUpload(sid, taskId, { status: "cancelled" })
    } else {
      const err = e as Error
      store.updateUpload(sid, taskId, { status: "error", error: err.message })
      message.error(t("sftp.message.uploadFailed", { error: err.message }))
    }
    return false
  }
}

async function customUpload(opts: UploadCustomRequestOptions) {
  const file = opts.file.file
  if (!file) {
    opts.onError()
    return
  }
  const done = await uploadOneFile(file)
  if (done) {
    opts.onFinish()
  } else {
    opts.onError()
  }
}

function cancelUpload(id: string) {
  if (!props.sid) return
  const t = store.listUploads(props.sid).find((x) => x.id === id)
  if (!t) return
  t.controller?.abort()
  store.updateUpload(props.sid, id, { status: "cancelled" })
}

/* ---------- 上传目录（webkitdirectory） ---------- */

const folderInputRef = ref<HTMLInputElement | null>(null)

function triggerUploadFolder() {
  if (!props.sid) return
  const el = folderInputRef.value
  if (!el) return
  el.value = ""
  el.click()
}

interface FolderEntry {
  file: File
  /** webkitRelativePath 形如 "myfolder/sub/a.txt" */
  relPath: string
}

function collectFolderEntries(fl: FileList): FolderEntry[] {
  const out: FolderEntry[] = []
  for (let i = 0; i < fl.length; i++) {
    const f = fl.item(i)
    if (!f) continue
    const raw = (f as File & { webkitRelativePath?: string }).webkitRelativePath ?? ""
    const rel = raw.replace(/\\/g, "/")
    if (!rel) continue
    out.push({ file: f, relPath: rel })
  }
  return out
}

async function onFolderInputChange(e: Event) {
  const input = e.target as HTMLInputElement
  const fl = input.files
  if (!fl || fl.length === 0 || !props.sid) {
    input.value = ""
    return
  }
  const entries = collectFolderEntries(fl)
  input.value = ""
  if (entries.length === 0) {
    message.warning(t("sftp.message.noFileSelected"))
    return
  }

  // 顶层目录名（取第一个文件 webkitRelativePath 的第一段）
  const first = entries[0]
  if (!first) return
  const topName = first.relPath.split("/")[0] ?? ""
  if (!topName) {
    message.error(t("sftp.message.cannotParseUploadDir"))
    return
  }

  // 顶层同名检查：只问一次
  const exists = files.value.some(
    (f) => f.file_name.toLowerCase() === topName.toLowerCase(),
  )
  if (exists) {
    const ok = await new Promise<boolean>((resolve) => {
      dialog.warning({
        title: t("sftp.dialog.uploadOverwriteTitle"),
        content: t("sftp.dialog.uploadOverwriteConfirm", { name: topName }),
        positiveText: t("common.overwrite"),
        negativeText: t("common.cancel"),
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
        onMaskClick: () => resolve(false),
      })
    })
    if (!ok) return
  }

  await uploadFolderEntries(entries)
  await load()
}

async function uploadFolderEntries(entries: FolderEntry[]) {
  if (!props.sid) return
  const sid = props.sid
  const baseDir = currentPath.value

  // 先把所有需要的子目录预先创建一遍（去重 + 按层级排序），减少首次写文件的等待
  const dirSet = new Set<string>()
  for (const ent of entries) {
    const segs = ent.relPath.split("/")
    if (segs.length <= 1) continue
    let acc = ""
    for (let i = 0; i < segs.length - 1; i++) {
      acc = acc ? `${acc}/${segs[i]}` : (segs[i] ?? "")
      if (acc) dirSet.add(acc)
    }
  }
  const dirs = [...dirSet].sort((a, b) => {
    const da = a.split("/").length
    const db = b.split("/").length
    if (da !== db) return da - db
    return a.localeCompare(b)
  })
  for (const rel of dirs) {
    try {
      await mkdirApi(sid, joinPath(baseDir, rel))
    } catch (e) {
      message.error(t("sftp.message.dirUploadFailed", { path: rel, error: (e as Error).message }))
      return
    }
  }

  // 串行上传文件
  let okCount = 0
  let failCount = 0
  for (const ent of entries) {
    const remotePath = joinPath(baseDir, ent.relPath)
    const ctrl = new AbortController()
    const taskId = genId()
    const remoteDir = parentPath(remotePath)
    const task: TransferTask = {
      id: taskId,
      sid,
      filename: remotePath,
      remoteDir,
      total: ent.file.size,
      loaded: 0,
      status: "running",
      controller: ctrl,
      startedAt: Date.now(),
    }
    store.addUpload(sid, task)
    try {
      await uploadStream({
        sid,
        filename: remotePath,
        file: ent.file,
        signal: ctrl.signal,
        onProgress: (loaded, total) => {
          store.updateUpload(sid, taskId, {
            loaded,
            total: total > 0 ? total : loaded,
          })
        },
      })
      store.updateUpload(sid, taskId, {
        loaded: ent.file.size,
        total: ent.file.size,
        status: "done",
      })
      okCount++
    } catch (e) {
      if (isAbortError(e)) {
        store.updateUpload(sid, taskId, { status: "cancelled" })
      } else {
        const err = e as Error
        store.updateUpload(sid, taskId, { status: "error", error: err.message })
      }
      failCount++
    }
  }
  if (failCount === 0) {
    message.success(t("sftp.message.dirUploadDone", { count: okCount }))
  } else {
    message.warning(t("sftp.message.dirUploadPartial", { ok: okCount, fail: failCount }))
  }
}

/* ---------- OS 级拖放上传（从资源管理器 / Finder 拖入面板） ----------
 * 窗口已关闭 dragDropEnabled（tauri.conf.json），HTML5 dnd 事件可用，
 * drop 直接拿到 File 对象，复用与按钮上传完全相同的上传通道
 * （uploadStream 的 XHR 进度 / 取消；文件夹复用 uploadFolderEntries）。 */

const dropHover = ref(false)

function onPanelDragOver(e: DragEvent) {
  if (!props.open || !props.sid) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = "copy"
  dropHover.value = true
}

function onPanelDragLeave() {
  dropHover.value = false
}

async function onPanelDrop(e: DragEvent) {
  e.preventDefault()
  dropHover.value = false
  if (!props.open || !props.sid) return
  const dt = e.dataTransfer
  if (!dt || dt.items.length === 0) return

  const topFiles: File[] = []
  const folders: { name: string; entries: FolderEntry[] }[] = []

  for (let i = 0; i < dt.items.length; i++) {
    const entry = dt.items[i]?.webkitGetAsEntry?.()
    if (!entry) continue
    if (entry.isFile) {
      const file = await new Promise<File | null>((resolve) =>
        (entry as FileSystemFileEntry).file(resolve, () => resolve(null)),
      )
      if (file) topFiles.push(file)
    } else if (entry.isDirectory) {
      const out: FolderEntry[] = []
      await walkDropDirectory(entry as FileSystemDirectoryEntry, entry.name, out)
      if (out.length > 0) folders.push({ name: entry.name, entries: out })
    }
  }

  // 顶层文件：与按钮上传完全同一条通道（含覆盖确认、进度、取消）
  for (const f of topFiles) {
    await uploadOneFile(f)
  }
  // 文件夹：顶层同名确认后复用既有目录上传流程
  for (const folder of folders) {
    const exists = files.value.some(
      (f) => f.file_name.toLowerCase() === folder.name.toLowerCase(),
    )
    if (exists) {
      const ok = await new Promise<boolean>((resolve) => {
        dialog.warning({
          title: t("sftp.dialog.uploadOverwriteTitle"),
          content: t("sftp.dialog.uploadOverwriteConfirm", { name: folder.name }),
          positiveText: t("common.overwrite"),
          negativeText: t("common.cancel"),
          onPositiveClick: () => resolve(true),
          onNegativeClick: () => resolve(false),
          onClose: () => resolve(false),
          onMaskClick: () => resolve(false),
        })
      })
      if (!ok) continue
    }
    await uploadFolderEntries(folder.entries)
  }
  await load()
}

/** 遍历拖入的目录为 FolderEntry 列表（relPath 与 webkitdirectory 的 webkitRelativePath 同形） */
async function walkDropDirectory(
  dir: FileSystemDirectoryEntry,
  prefix: string,
  out: FolderEntry[],
): Promise<void> {
  const entries = await readAllDirectoryEntries(dir.createReader())
  for (const ent of entries) {
    const rel = `${prefix}/${ent.name}`
    if (ent.isFile) {
      const file = await new Promise<File | null>((resolve) =>
        (ent as FileSystemFileEntry).file(resolve, () => resolve(null)),
      )
      if (file) out.push({ file, relPath: rel })
    } else if (ent.isDirectory) {
      await walkDropDirectory(ent as FileSystemDirectoryEntry, rel, out)
    }
  }
}

/** readEntries 单次最多返回 100 条，需循环读取直到返回空 */
function readAllDirectoryEntries(reader: FileSystemDirectoryReader): Promise<FileSystemEntry[]> {
  return new Promise((resolve, reject) => {
    const all: FileSystemEntry[] = []
    const readBatch = () => {
      reader.readEntries(
        (batch) => {
          if (batch.length === 0) {
            resolve(all)
            return
          }
          all.push(...batch)
          readBatch()
        },
        (err) => reject(err),
      )
    }
    readBatch()
  })
}

/* ---------- 表格列 ---------- */

/** 文件类型配色：用于文件图标 */
const FILE_TYPE_COLORS = {
  dir: "#f1c27d",
  symlink: "#7c5cff",
  file: "#9aa0a6",
}

function fileIcon(file: SftpFile) {
  if (file.file_type === "dir") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.dir }, { default: () => h(FolderOutline) })
  }
  if (file.file_type === "symlink") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.symlink }, { default: () => h(LinkOutline) })
  }
  return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.file }, { default: () => h(DocumentOutline) })
}

function dirFirst(a: SftpFile, b: SftpFile): number {
  const da = a.file_type === "dir" ? 0 : 1
  const db = b.file_type === "dir" ? 0 : 1
  return da - db
}

/** 权限单段着色（perm 为 9 位符号串，start 为段起始下标 0/3/6）：
 *  rwx 绿、rw- 蓝、其余默认。 */
function triadColor(perm: string, start: number): string | null {
  const r = perm.charAt(start) === "r"
  const w = perm.charAt(start + 1) === "w"
  const x = perm.charAt(start + 2) === "x"
  if (r && w && x) return "#7ed491"
  if (r && w) return "#5e9bff"
  return null
}

const columns = computed<DataTableColumns<SftpFile>>(() => [
  {
    title: t("sftp.columns.name"),
    key: "file_name",
    minWidth: 220,
    sorter: (a, b) => {
      const d = dirFirst(a, b)
      if (d !== 0) return d
      return a.file_name.toLowerCase().localeCompare(b.file_name.toLowerCase())
    },
    render(row) {
      return h("div", { class: "name-cell" }, [
        fileIcon(row),
        h("span", { class: "name-text", title: row.full_path }, [
          row.file_name,
          row.file_type === "symlink" && row.link_path
            ? ` -> ${row.link_path}`
            : "",
        ]),
      ])
    },
  },
  {
    title: t("sftp.columns.size"),
    key: "size",
    width: 96,
    resizable: true,
    sorter: (a, b) => {
      const d = dirFirst(a, b)
      if (d !== 0) return d
      const sa = typeof a.size_bytes === "number" ? a.size_bytes : -1
      const sb = typeof b.size_bytes === "number" ? b.size_bytes : -1
      return sa - sb
    },
    render(row) {
      if (row.file_type === "dir") return "-"
      if (typeof row.size_bytes === "number") return humanSize(row.size_bytes)
      return row.size || "-"
    },
  },
  {
    title: t("sftp.columns.permission"),
    key: "permissions",
    width: 110,
    resizable: true,
    render(row) {
      const perm = row.permissions || "-"
      if (perm.length < 9) return perm
      return h("span", {}, [
        h("span", { style: { color: triadColor(perm, 0) ?? undefined } }, perm.slice(0, 3)),
        h("span", { style: { color: triadColor(perm, 3) ?? undefined } }, perm.slice(3, 6)),
        h("span", { style: { color: triadColor(perm, 6) ?? undefined } }, perm.slice(6, 9)),
      ])
    },
  },
  {
    title: t("sftp.columns.userGroup"),
    key: "user",
    width: 140,
    resizable: true,
    render(row) {
      return `${row.user || "-"} / ${row.group || "-"}`
    },
  },
  {
    title: t("sftp.columns.modifyTime"),
    key: "mtime",
    width: 170,
    resizable: true,
    sorter: (a, b) => {
      const d = dirFirst(a, b)
      if (d !== 0) return d
      const ma = typeof a.mtime === "number" ? a.mtime : null
      const mb = typeof b.mtime === "number" ? b.mtime : null
      if (ma === null && mb === null) return 0
      if (ma === null) return 1
      if (mb === null) return -1
      return ma - mb
    },
    render(row) {
      return formatUnix(row.mtime ?? null)
    },
  },
])

const rowKey = (row: SftpFile) => row.full_path

function rowProps(row: SftpFile) {
  return {
    class: selectedKey.value === row.full_path ? "row-selected" : "",
    style: {
      // 双栏模式下文件行可拖到本地栏，提示 grab
      cursor: dualPane.value && row.file_type === "file" ? "grab" : "default",
    },
    onPointerdown: (e: PointerEvent) => {
      if (dualPane.value) remoteDrag.onRowPointerdown(row, e)
    },
    onClick: (e: MouseEvent) => {
      e.stopPropagation()
      selectedKey.value = row.full_path
      ctxMenuVisible.value = false
    },
    onDblclick: (e: MouseEvent) => {
      e.stopPropagation()
      if (row.file_type === "dir" || row.file_type === "symlink") {
        enterDir(row)
      } else if (row.file_type === "file") {
        if (isPreviewable(row.file_name)) {
          openPreview(row)
        } else {
          void onDownload(row)
        }
      }
    },
    onContextmenu: (e: MouseEvent) => {
      e.preventDefault()
      e.stopPropagation()
      selectedKey.value = row.full_path
      openCtxMenu(e, row)
    },
  }
}

/* ---------- 右键菜单 ---------- */

function openCtxMenu(e: MouseEvent, row: SftpFile | null) {
  ctxMenuTarget.value = row
  ctxMenuVisible.value = false
  ctxMenuX.value = e.clientX
  ctxMenuY.value = e.clientY
  // 重新打开（参考 NDropdown 上下文菜单写法）
  void Promise.resolve().then(() => {
    ctxMenuVisible.value = true
  })
}

function onBlankContextMenu(e: MouseEvent) {
  e.preventDefault()
  openCtxMenu(e, null)
}

const ctxMenuOptions = computed(() => {
  const target = ctxMenuTarget.value
  if (!target) {
    return [
      {
        label: t("sftp.ctxMenu.newFolder"),
        key: "new-mkdir",
        icon: () => h(NIcon, null, { default: () => h(FolderOpenOutline) }),
      },
      {
        label: t("sftp.ctxMenu.newFile"),
        key: "new-touch",
        icon: () => h(NIcon, null, { default: () => h(DocumentOutline) }),
      },
      { type: "divider", key: "d1" },
      {
        label: t("sftp.ctxMenu.refresh"),
        key: "refresh",
        icon: () => h(NIcon, null, { default: () => h(RefreshOutline) }),
      },
    ]
  }
  const opts: Array<Record<string, unknown>> = []
  if (target.file_type === "file") {
    if (isPreviewable(target.file_name)) {
      opts.push({
        label: t("sftp.ctxMenu.preview"),
        key: "preview",
        icon: () => h(NIcon, null, { default: () => h(EyeOutline) }),
      })
    }
    opts.push({
      label: t("sftp.ctxMenu.edit"),
      key: "edit",
      icon: () => h(NIcon, null, { default: () => h(CreateOutline) }),
    })
    opts.push({
      label: t("sftp.ctxMenu.download"),
      key: "download",
      icon: () => h(NIcon, null, { default: () => h(DownloadOutline) }),
    })
  }
  opts.push({
    label: t("sftp.ctxMenu.rename"),
    key: "rename",
    icon: () => h(NIcon, null, { default: () => h(CreateOutline) }),
  })
  opts.push({
    label: t("sftp.ctxMenu.delete"),
    key: "remove",
    icon: () => h(NIcon, null, { default: () => h(TrashOutline) }),
  })
  opts.push({ type: "divider", key: "d1" })
  opts.push({
    label: t("sftp.ctxMenu.copyPath"),
    key: "copy-path",
    icon: () => h(NIcon, null, { default: () => h(CopyOutline) }),
  })
  if (startupStore.aiAssistantEnabled) {
    opts.push({
      label: t("sftp.ctxMenu.sendToAi"),
      key: "send-to-ai",
      icon: () => h(NIcon, null, { default: () => h(SparklesOutline) }),
    })
  }
  opts.push({
    label: t("sftp.ctxMenu.properties"),
    key: "props",
    icon: () => h(NIcon, null, { default: () => h(DocumentOutline) }),
  })
  return opts
})

function showProps(file: SftpFile) {
  const lines = [
    t("sftp.properties.name") + file.file_name,
    t("sftp.properties.path") + file.full_path,
    t("sftp.properties.type") + file.file_type,
    t("sftp.properties.size") + (
      typeof file.size_bytes === "number" ? humanSize(file.size_bytes) : file.size || "-"
    ),
    t("sftp.properties.permission") + (file.permissions || "-"),
    t("sftp.properties.userGroup") + `${file.user || "-"} / ${file.group || "-"}`,
    t("sftp.properties.modifyTime") + formatUnix(file.mtime ?? null),
  ]
  dialog.info({
    title: t("sftp.properties.title"),
    content: () => h("div", { style: "white-space:pre-line" }, lines.join("\n")),
    positiveText: t("common.confirm"),
  })
}

async function copyPath(file: SftpFile) {
  try {
    await navigator.clipboard.writeText(file.full_path)
    message.success(t("sftp.message.copied"))
  } catch {
    message.error(t("sftp.message.copyFailed"))
  }
}

function onCtxMenuSelect(key: string | number) {
  ctxMenuVisible.value = false
  const target = ctxMenuTarget.value
  if (!target) {
    if (key === "new-mkdir") openMkdir()
    else if (key === "new-touch") openTouch()
    else if (key === "refresh") refresh()
    return
  }
  if (key === "preview") openPreview(target)
  else if (key === "download") void onDownload(target)
  else if (key === "edit") openEditor(target)
  else if (key === "rename") openRename(target)
  else if (key === "remove") confirmRemove(target)
  else if (key === "copy-path") void copyPath(target)
  else if (key === "send-to-ai") openAiPrompt(target)
  else if (key === "props") showProps(target)
}

/* ---------- 发送给 AI 提示词浮层 ---------- */

function openAiPrompt(file: SftpFile) {
  aiSelectionText.value = file.full_path
  // aside 有 transform，position:fixed 会以 aside 为参照；
  // 改用 absolute 定位，坐标转为面板内相对值
  const rect = panelRef.value?.getBoundingClientRect()
  if (rect) {
    aiPromptX.value = 16
    // 弹窗约 40px 高，靠近底部时往上偏移，避免溢出面板
    const POPOVER_HEIGHT = 40
    const maxY = rect.height - POPOVER_HEIGHT - 8
    aiPromptY.value = Math.max(8, Math.min(ctxMenuY.value - rect.top, maxY))
  } else {
    aiPromptX.value = ctxMenuX.value
    aiPromptY.value = ctxMenuY.value
  }
  aiPromptText.value = ""
  aiPromptVisible.value = true
}

function submitAiPrompt() {
  const sel = aiSelectionText.value
  if (!sel.trim()) {
    cancelAiPrompt()
    return
  }
  const prompt = aiPromptText.value.trim()
  const combined = prompt ? `${prompt}\n\n${sel}` : sel
  aiPromptVisible.value = false
  aiPromptText.value = ""
  aiSelectionText.value = ""
  emit("send-to-ai", combined)
}

function cancelAiPrompt() {
  aiPromptVisible.value = false
  aiPromptText.value = ""
  aiSelectionText.value = ""
}

function onAiPromptKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault()
    submitAiPrompt()
  } else if (e.key === "Escape") {
    e.preventDefault()
    cancelAiPrompt()
  }
}

/* ---------- 新建下拉 ---------- */

const newMenuOptions = computed(() => [
  {
    label: t("sftp.newMenu.newFolder"),
    key: "mkdir",
    icon: () => h(NIcon, null, { default: () => h(FolderOpenOutline) }),
  },
  {
    label: t("sftp.newMenu.newFile"),
    key: "touch",
    icon: () => h(NIcon, null, { default: () => h(DocumentOutline) }),
  },
])

function onNewSelect(key: string | number) {
  if (key === "mkdir") openMkdir()
  else if (key === "touch") openTouch()
}

/* ---------- 监听打开 ---------- */

watch(
  () => [props.open, props.sid] as const,
  ([open, sid]) => {
    if (open && sid) {
      const stored = store.getPath(sid)
      currentPath.value = stored
      void load(stored)
    }
  },
  { immediate: true },
)

/* ---------- 拖拽改变面板宽度 ---------- */
const MIN_WIDTH = 480
const DEFAULT_WIDTH = 800
// 双栏（本地 + 远程）模式下的宽度约束与记忆 key 独立于单栏，
// 切换模式时互不污染各自记忆的宽度
const DUAL_MIN_WIDTH = 760
const DUAL_DEFAULT_WIDTH = 1040
const WIDTH_KEY = "ashell:sftp-width"
const DUAL_WIDTH_KEY = "ashell:sftp-dual-width"
const DUAL_PANE_KEY = "ashell:sftp-dualpane"
const LOCAL_DIR_KEY = "ashell:sftp-local-dir"

// 拖动上限取视口宽度的 90%，避免抽屉完全盖住主界面
function getMaxWidth(): number {
  return Math.round(window.innerWidth * 0.9)
}

/** 双栏开关（默认单栏）。持久化，下次打开 drawer 恢复 */
const dualPane = ref(
  typeof localStorage !== "undefined" && localStorage.getItem(DUAL_PANE_KEY) === "1",
)

/** 本地栏当前目录，持久化 */
const localDir = ref(
  typeof localStorage !== "undefined" ? localStorage.getItem(LOCAL_DIR_KEY) || "" : "",
)

function persistLocalDir(v: string) {
  localDir.value = v
  try {
    localStorage.setItem(LOCAL_DIR_KEY, v)
  } catch {
    // ignore
  }
}

const localPaneRef = ref<InstanceType<typeof LocalPane> | null>(null)

/** 本地栏勾选数（LocalPane selection-change 上报，供中间条按钮启停） */
const localSelectedCount = ref(0)

/* ---------- 远程侧拖拽：远程行 -> 本地栏（下载） ---------- */

const remoteDrag = useFileDrag({
  collectFiles(row) {
    // 一期远程为单选列表，仅支持单文件拖拽
    return row.file_type === "file" ? [row] : []
  },
  onDrop(files, zone) {
    if (zone === "local" && localDir.value) {
      for (const f of files) void downloadToLocalDir(f)
    }
  },
})

/** 中间条 -> ：把本地栏勾选的文件上传到远程当前目录 */
function transferUp() {
  const sel = localPaneRef.value?.getSelectedFiles() ?? []
  if (sel.length > 0) void onLocalUpload(sel)
}

/** 中间条 <- ：把远程当前选中的文件下载到本地栏当前目录 */
function transferDown() {
  const f = files.value.find(
    (x) => x.full_path === selectedKey.value && x.file_type === "file",
  )
  if (f && localDir.value) void downloadToLocalDir(f)
}

function activeWidthKey(): string {
  return dualPane.value ? DUAL_WIDTH_KEY : WIDTH_KEY
}

function activeMinWidth(): number {
  return dualPane.value ? DUAL_MIN_WIDTH : MIN_WIDTH
}

function activeDefaultWidth(): number {
  return dualPane.value ? DUAL_DEFAULT_WIDTH : DEFAULT_WIDTH
}

function toggleDualPane() {
  // 先把当前宽度保存到旧模式的 key，再切换并加载新模式宽度
  saveWidth(width.value)
  dualPane.value = !dualPane.value
  try {
    localStorage.setItem(DUAL_PANE_KEY, dualPane.value ? "1" : "0")
  } catch {
    // ignore
  }
  width.value = loadWidth()
}

const width = ref<number>(0)
const resizing = ref(false)

function loadWidth(): number {
  const raw =
    typeof localStorage !== "undefined" ? localStorage.getItem(activeWidthKey()) : null
  const n = raw ? Number(raw) : NaN
  if (!Number.isFinite(n)) return activeDefaultWidth()
  return Math.min(getMaxWidth(), Math.max(activeMinWidth(), n))
}

function saveWidth(v: number) {
  try {
    localStorage.setItem(activeWidthKey(), String(v))
  } catch {
    // ignore
  }
}

width.value = loadWidth()

function onResizeStart(e: PointerEvent) {
  e.preventDefault()
  resizing.value = true
  window.addEventListener("pointermove", onResizeMove)
  window.addEventListener("pointerup", onResizeEnd)
  window.addEventListener("pointercancel", onResizeEnd)
}

function onResizeMove(e: PointerEvent) {
  // Panel anchored to the right edge; width = viewport width - cursor X.
  const next = Math.round(window.innerWidth - e.clientX)
  width.value = Math.min(getMaxWidth(), Math.max(activeMinWidth(), next))
}

function onResizeEnd() {
  if (!resizing.value) return
  resizing.value = false
  saveWidth(width.value)
  window.removeEventListener("pointermove", onResizeMove)
  window.removeEventListener("pointerup", onResizeEnd)
  window.removeEventListener("pointercancel", onResizeEnd)
}

onBeforeUnmount(onResizeEnd)

const panelStyle = computed(() => ({
  width: `${width.value}px`,
  transition: resizing.value ? "none" : "transform 0.25s ease, box-shadow 0.15s ease",
  transform: props.open ? "translateX(0)" : "translateX(100%)",
}))

function onClose() {
  emit("update:open", false)
}
</script>

<template>
  <Teleport to="body">
    <aside
      ref="panelRef"
      class="sftp-panel"
      :class="{ open: props.open, resizing: resizing }"
      :style="panelStyle"
      :aria-hidden="!props.open"
      @dragover="onPanelDragOver"
      @dragleave="onPanelDragLeave"
      @drop="onPanelDrop"
    >
      <div
        class="resize-handle"
        :title="t('common.dragToResize')"
        @pointerdown="onResizeStart"
      />

      <!-- OS 拖放文件进入面板时的提示遮罩 -->
      <div v-if="dropHover && props.sid" class="drop-overlay">
        <div class="drop-overlay-inner">
          <NIcon :size="28">
            <CloudUploadOutline />
          </NIcon>
          <span>{{ t("sftp.dropHint", { path: currentPath }) }}</span>
        </div>
      </div>

      <header class="panel-header">
        <span class="drawer-title">{{ drawerTitle }}</span>
        <NSpace :size="6" align="center" :wrap="false">
          <NButton size="small" quaternary circle :title="t('sftp.refresh')" @click="refresh">
            <template #icon>
              <NIcon><RefreshOutline /></NIcon>
            </template>
          </NButton>
          <NButton size="small" quaternary circle :title="t('sftp.close')" @click="onClose">
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </NSpace>
      </header>

      <div class="panel-body">
        <div v-if="!props.sid" class="empty-wrap">
          <NEmpty :description="t('sftp.needSession')" />
        </div>

        <div
          v-else
          class="sftp-body"
          :class="{ dual: dualPane }"
          @contextmenu="onBlankContextMenu"
        >
          <LocalPane
            v-if="dualPane"
            ref="localPaneRef"
            class="local-pane-slot"
            :dir="localDir"
            @update:dir="persistLocalDir"
            @selection-change="localSelectedCount = $event"
            @transfer-up="onLocalUpload"
          />
          <div v-if="dualPane" class="transfer-bar">
            <NTooltip placement="left">
              <template #trigger>
                <NButton
                  size="small"
                  secondary
                  :disabled="!props.sid || localSelectedCount === 0"
                  @click="transferUp"
                >
                  <template #icon>
                    <NIcon><ArrowForwardOutline /></NIcon>
                  </template>
                </NButton>
              </template>
              {{ t("sftp.transferBar.up") }}
            </NTooltip>
            <NTooltip placement="left">
              <template #trigger>
                <NButton
                  size="small"
                  secondary
                  :disabled="
                    !props.sid ||
                    !localDir ||
                    !files.some(
                      (x) =>
                        x.full_path === selectedKey && x.file_type === 'file',
                    )
                  "
                  @click="transferDown"
                >
                  <template #icon>
                    <NIcon><ArrowBackOutline /></NIcon>
                  </template>
                </NButton>
              </template>
              {{ t("sftp.transferBar.down") }}
            </NTooltip>
          </div>
          <div class="remote-pane" data-drop-zone="remote">
          <div class="path-bar">
            <NButton
              size="small"
              quaternary
              circle
              :title="t('sftp.goUp')"
              :disabled="currentPath === '/'"
              @click="goUp"
            >
              <template #icon>
                <NIcon><ArrowUpOutline /></NIcon>
              </template>
            </NButton>
            <div class="address-bar">
              <NInput
                v-if="pathEditing"
                ref="pathInputRef"
                v-model:value="pathDraft"
                size="small"
                placeholder="/path/to/dir"
                class="address-input"
                @keydown="onPathKeydown"
                @blur="submitPathEdit"
              />
              <div
                v-else
                class="address-display"
                :title="currentPath"
                tabindex="0"
                role="textbox"
                @click="startPathEdit"
                @keydown.enter.prevent="startPathEdit"
                @keydown.space.prevent="startPathEdit"
              >
                {{ currentPath }}
              </div>
            </div>
            <NButton
              size="small"
              quaternary
              circle
              :title="t('sftp.copyPath')"
              :disabled="pathEditing"
              @click="copyCurrentPath"
            >
              <template #icon>
                <NIcon><CopyOutline /></NIcon>
              </template>
            </NButton>
          </div>

          <div class="toolbar">
            <div class="toolbar-left">
              <NDropdown
                trigger="click"
                :options="newMenuOptions"
                @select="onNewSelect"
              >
                <NButton size="small" secondary>
                  <template #icon>
                    <NIcon><CreateOutline /></NIcon>
                  </template>
                  {{ t("sftp.newButton") }}
                </NButton>
              </NDropdown>
              <NUpload
                class="upload-trigger"
                multiple
                :show-file-list="false"
                :custom-request="customUpload"
                :disabled="!props.sid"
              >
                <NButton size="small" secondary>
                  <template #icon>
                    <NIcon><CloudUploadOutline /></NIcon>
                  </template>
                  {{ t("sftp.uploadButton") }}
                </NButton>
              </NUpload>
              <NButton
                size="small"
                secondary
                :disabled="!props.sid"
                :title="t('sftp.uploadFolderTitle')"
                @click="triggerUploadFolder"
              >
                <template #icon>
                  <NIcon><FolderOpenOutline /></NIcon>
                </template>
                {{ t("sftp.uploadFolderButton") }}
              </NButton>
              <input
                ref="folderInputRef"
                type="file"
                webkitdirectory
                directory
                multiple
                style="display: none"
                @change="onFolderInputChange"
              />
              <NButton
                size="small"
                secondary
                :type="dualPane ? 'primary' : 'default'"
                :title="t('sftp.localPane.toggleTitle')"
                @click="toggleDualPane"
              >
                <template #icon>
                  <NIcon><LaptopOutline /></NIcon>
                </template>
                {{ t("sftp.localPane.toggleButton") }}
              </NButton>
            </div>
            <div class="toolbar-right">
              <NBadge
                class="badge-btn"
                :value="activeUploadCount"
                :show="activeUploadCount > 0"
                :max="99"
                type="info"
              >
                <NButton
                  size="small"
                  quaternary
                  :title="t('sftp.uploadListTitle')"
                  @click="uploadModalOpen = true"
                >
                  <template #icon>
                    <NIcon><CloudUploadOutline /></NIcon>
                  </template>
                  {{ t("sftp.uploadListButton") }}
                </NButton>
              </NBadge>
              <NBadge
                class="badge-btn"
                :value="activeDownloadCount"
                :show="activeDownloadCount > 0"
                :max="99"
                type="info"
              >
                <NButton
                  size="small"
                  quaternary
                  :title="t('sftp.downloadListTitle')"
                  @click="downloadModalOpen = true"
                >
                  <template #icon>
                    <NIcon><DownloadOutline /></NIcon>
                  </template>
                  {{ t("sftp.downloadListButton") }}
                </NButton>
              </NBadge>
            </div>
          </div>

          <NSpin :show="loading" class="table-wrap">
            <NDataTable
              size="small"
              :columns="columns"
              :data="files"
              :row-key="rowKey"
              :row-props="rowProps"
              :bordered="false"
              :single-line="false"
              flex-height
              class="file-table"
            />
          </NSpin>
          </div>

          <!-- 远程侧拖拽跟随标签。Teleport 到 body：本 aside 有 transform
               （开合动画），fixed 的包含块会变成它导致坐标漂移 -->
          <Teleport to="body">
            <div
              v-if="remoteDrag.dragging.value"
              class="drag-ghost"
              :style="{
                left: `${remoteDrag.ghostX.value}px`,
                top: `${remoteDrag.ghostY.value}px`,
              }"
            >
              {{ t("sftp.transferBar.dragGhost", { count: remoteDrag.dragCount.value }) }}
            </div>
          </Teleport>
        </div>

        <NDropdown
          trigger="manual"
          placement="bottom-start"
          :show="ctxMenuVisible"
          :options="ctxMenuOptions"
          :x="ctxMenuX"
          :y="ctxMenuY"
          @clickoutside="ctxMenuVisible = false"
          @select="onCtxMenuSelect"
        />

        <Transition name="ai-prompt-fade">
          <div
            v-if="aiPromptVisible"
            class="ai-prompt-popover"
            :style="{ left: `${aiPromptX}px`, top: `${aiPromptY}px` }"
            @keydown.stop
          >
            <NInput
              v-model:value="aiPromptText"
              size="small"
              :placeholder="t('terminal.aiPromptPlaceholder')"
              clearable
              class="ai-prompt-input"
              @keydown="onAiPromptKeydown"
            />
            <NTooltip placement="top">
              <template #trigger>
                <NButton
                  size="small"
                  quaternary
                  class="ai-prompt-btn"
                  @click="submitAiPrompt"
                >
                  <NIcon :size="14">
                    <SendOutline />
                  </NIcon>
                </NButton>
              </template>
              {{ t('terminal.sendToAi') }}
            </NTooltip>
            <NTooltip placement="top">
              <template #trigger>
                <NButton
                  size="small"
                  quaternary
                  class="ai-prompt-btn"
                  @click="cancelAiPrompt"
                >
                  <NIcon :size="14">
                    <CloseOutline />
                  </NIcon>
                </NButton>
              </template>
              {{ t('terminal.search.close') }}
            </NTooltip>
          </div>
        </Transition>

        <NModal
          v-model:show="uploadModalOpen"
          preset="card"
          :title="t('sftp.uploadListTitle')"
          style="width: 560px"
          :mask-closable="true"
          :draggable="true"
        >
          <template #header-extra>
            <NButton
              size="tiny"
              quaternary
              :disabled="finishedUploadCount === 0"
              @click="clearFinishedUploads"
            >
              <template #icon>
                <NIcon><TrashOutline /></NIcon>
              </template>
              {{ t("common.clearCompleted") }}
            </NButton>
          </template>
          <SftpUploadList :tasks="uploads" @cancel="cancelUpload" />
        </NModal>
        <NModal
          v-model:show="downloadModalOpen"
          preset="card"
          :title="t('sftp.downloadListTitle')"
          style="width: 560px"
          :mask-closable="true"
          :draggable="true"
        >
          <template #header-extra>
            <NButton
              size="tiny"
              quaternary
              :disabled="finishedDownloadCount === 0"
              @click="clearFinishedDownloads"
            >
              <template #icon>
                <NIcon><TrashOutline /></NIcon>
              </template>
              {{ t("common.clearCompleted") }}
            </NButton>
          </template>
          <SftpDownloadList :tasks="downloads" @cancel="cancelDownload" />
        </NModal>

        <MkdirDialog
          v-model:open="mkdirOpen"
          :mode="mkdirMode"
          :current-path="currentPath"
          @submit="onMkdirSubmit"
        />
        <RenameDialog
          v-model:open="renameOpen"
          :old-name="renameTarget?.file_name ?? ''"
          @submit="onRenameSubmit"
        />

        <FileEditor
          v-model:open="editorOpen"
          :sid="props.sid"
          :file="editorFile"
          @saved="onEditorSaved"
        />

        <FilePreview
          v-model:open="previewOpen"
          :sid="props.sid"
          :file="previewFile"
          @download="onDownload"
        />
      </div>
    </aside>
  </Teleport>
</template>

<style scoped>
.sftp-panel {
  position: fixed;
  top: var(--ashell-header-h);
  right: var(--ashell-activity-w, 0px);
  bottom: 0;
  background: var(--ashell-panel-bg);
  border-left: 1px solid var(--ashell-border);
  display: flex;
  flex-direction: column;
  z-index: 1000;
  user-select: text;
}

.sftp-panel.open {
  box-shadow: -8px 0 24px var(--ashell-shadow);
}

.sftp-panel.resizing {
  user-select: none;
}

.resize-handle {
  position: absolute;
  top: 0;
  left: -3px;
  width: 6px;
  height: 100%;
  cursor: col-resize;
  z-index: 1;
  background: transparent;
  transition: background 0.15s ease;
}

.resize-handle:hover,
.sftp-panel.resizing .resize-handle {
  background: rgba(124, 92, 255, 0.45);
}

.drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(124, 92, 255, 0.08);
  border: 2px dashed rgba(124, 92, 255, 0.7);
  pointer-events: none;
}

.drop-overlay-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 16px 24px;
  border-radius: 8px;
  background: var(--ashell-panel-bg);
  color: var(--ashell-text-strong);
  font-size: 13px;
  box-shadow: 0 4px 16px var(--ashell-shadow);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: nowrap;
  gap: 12px;
  width: 100%;
  min-width: 0;
  padding: 14px 16px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.panel-body {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
  padding: 12px 16px;
}

.drawer-title {
  flex: 1 1 auto;
  min-width: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.panel-header :deep(.n-space) {
  flex-shrink: 0;
  flex-wrap: nowrap !important;
}

.empty-wrap {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 240px;
}

.sftp-body {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
  height: 100%;
  gap: 10px;
}

/* 双栏模式：本地栏在左、远程栏在右 */
.sftp-body.dual {
  flex-direction: row;
}

.local-pane-slot {
  flex: 0 0 42%;
  min-width: 0;
}

.transfer-bar {
  flex: 0 0 44px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 0 2px;
}

.drag-ghost {
  position: fixed;
  z-index: 9999;
  pointer-events: none;
  padding: 4px 10px;
  border-radius: 4px;
  background: var(--ashell-accent, #7c5cff);
  color: #fff;
  font-size: 12px;
  transform: translate(12px, 12px);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  white-space: nowrap;
}

.remote-pane {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  gap: 10px;
}

.path-bar {
  display: flex;
  align-items: center;
  flex-wrap: nowrap;
  gap: 8px;
  padding: 4px 0;
  min-width: 0;
}

.path-bar > .n-button {
  flex-shrink: 0;
}

.address-bar {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
}

.address-display {
  flex: 1 1 auto;
  min-width: 0;
  height: 28px;
  line-height: 28px;
  padding: 0 10px;
  border: 1px solid var(--ashell-border-soft);
  border-radius: 4px;
  background: var(--ashell-input-bg, transparent);
  font-family: var(--n-font-family-mono);
  font-size: 12px;
  color: var(--ashell-text-strong);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: text;
  outline: none;
  transition: border-color 0.15s ease, background 0.15s ease;
}

.address-display:hover {
  border-color: var(--ashell-border);
}

.address-display:focus {
  border-color: rgba(124, 92, 255, 0.6);
}

.address-input {
  flex: 1 1 auto;
  min-width: 0;
}

.address-input :deep(.n-input__input-el) {
  font-family: var(--n-font-family-mono);
  font-size: 12px;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: nowrap;
  flex-shrink: 0;
  min-width: 0;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ashell-border-soft);
}

.toolbar-left,
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: nowrap;
  min-width: 0;
}

.toolbar-right {
  flex-shrink: 0;
}

.toolbar :deep(.n-upload) {
  display: inline-flex;
  width: auto;
  flex: 0 0 auto;
}

.toolbar :deep(.n-upload-trigger) {
  display: inline-flex;
  width: auto;
}

.badge-btn {
  display: inline-flex;
  align-items: center;
}

.badge-btn :deep(.n-badge-sup) {
  pointer-events: none;
}

.table-wrap {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.table-wrap :deep(.n-spin-container),
.table-wrap :deep(.n-spin-content) {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.file-table {
  flex: 1 1 auto;
  min-height: 0;
}

.file-table :deep(.n-data-table) {
  height: 100%;
}

.file-table :deep(.n-data-table-tr) {
  cursor: pointer;
}

.file-table :deep(.n-data-table-tr.row-selected .n-data-table-td) {
  background-color: var(--ashell-row-selected, rgba(99, 153, 255, 0.16));
}

:deep(.name-cell) {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

:deep(.name-cell .name-text) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ashell-text-strong);
}

.ai-prompt-popover {
  position: absolute;
  z-index: 1100;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-border, #3a3f4b);
  border-radius: 6px;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.25);
}

.ai-prompt-input {
  width: 260px;
}

.ai-prompt-btn {
  flex-shrink: 0;
}

.ai-prompt-fade-enter-active,
.ai-prompt-fade-leave-active {
  transition: opacity 120ms ease, transform 120ms ease;
}
.ai-prompt-fade-enter-from,
.ai-prompt-fade-leave-to {
  opacity: 0;
  transform: scale(0.8);
}
</style>
