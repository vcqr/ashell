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
import type {
  DataTableColumns,
  DataTableSortState,
  InputInst,
  UploadCustomRequestOptions,
} from "naive-ui"
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
  LaptopOutline,
  OpenOutline,
  RefreshOutline,
  SendOutline,
  SparklesOutline,
  TrashOutline,
} from "@vicons/ionicons5"
import { FileRegular, Folder, Link } from "@vicons/fa"
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
import type { OsDropEntry, OsDropFolder, SftpFile, TransferTask } from "@/types"
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
import {
  downloadToLocal,
  listLocalFs,
  transferProgress,
  uploadLocalToRemote,
} from "@/api/local"
import { openSftpInNewWindow } from "@/utils/newWindow"
import { useFileDrag } from "@/composables/useFileDrag"
import { useMultiSelect } from "@/composables/useMultiSelect"

interface Props {
  open: boolean
  sid: string | null
  hostName?: string
  hostAddr?: string
  /** 独立窗口模式：面板铺满窗口、无宽度拖拽，关闭按钮直接关窗口 */
  standalone?: boolean
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

/* ---------- 远程栏多选（与本地栏共用 useMultiSelect 语义） ---------- */

const {
  selectedFiles: remoteSelectedFiles,
  isSelected: isRemoteSelected,
  selectExclusive: selectRemoteExclusive,
  onRowClick: onRemoteRowClick,
  collectForTransfer: collectRemoteForTransfer,
  clearSelection: clearRemoteSelection,
} = useMultiSelect(files)

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
    // 排序状态跨目录保留（与受控前 NDataTable 内部排序行为一致）
    files.value = applySort(resp.files)
    currentPath.value = resp.path || target
    clearRemoteSelection()
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

/** 逐条删除（目录 removeDir / 文件 removeFile），汇总成功/失败后刷新。
 *  单条失败沿用具体错误文案；多条时部分成功给 warning 汇总。 */
async function removeEntries(targets: SftpFile[]) {
  if (!props.sid || targets.length === 0) return
  const sid = props.sid
  let ok = 0
  let fail = 0
  let firstError = ""
  for (const f of targets) {
    try {
      if (f.file_type === "dir") {
        await removeDir(sid, f.full_path)
      } else {
        await removeFile(sid, f.full_path)
      }
      ok++
    } catch (e) {
      fail++
      if (!firstError) firstError = (e as Error).message
    }
  }
  if (fail === 0) {
    message.success(t("sftp.message.deleted"))
  } else if (targets.length === 1) {
    message.error(t("sftp.message.deleteFailed", { error: firstError }))
  } else {
    message.warning(t("sftp.message.deletePartial", { ok, fail }))
  }
  await load()
}

function confirmRemove(file: SftpFile) {
  // 右键行在选择集内：作用于整个选择集（批量）；否则删除右键的单个条目
  const targets = isRemoteSelected(file) ? remoteSelectedFiles.value : [file]
  const content =
    targets.length > 1
      ? t("sftp.dialog.deleteConfirmMulti", { count: targets.length })
      : t("sftp.dialog.deleteConfirm", {
          type:
            file.file_type === "dir"
              ? t("sftp.dialog.typeFolder")
              : t("sftp.dialog.typeFile"),
          name: file.file_name,
        })
  dialog.warning({
    title: t("sftp.dialog.deleteTitle"),
    content,
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: () => {
      void removeEntries(targets)
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

/** 经 webview 下载单个文件到本机（下载任务 + 进度 + 取消） */
async function downloadViaBrowser(file: SftpFile) {
  if (!props.sid) return
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

function confirmDownload(content: string): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    dialog.info({
      title: t("sftp.dialog.downloadTitle"),
      content,
      positiveText: t("common.download"),
      negativeText: t("common.cancel"),
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false),
    })
  })
}

/** 双栏直落本地目录前检查同名：基于本地栏已加载列表（与上传侧同一模式），
 *  有冲突弹一次覆盖确认（批量只问一次，列出冲突名） */
async function confirmDownloadOverwrite(items: SftpFile[]): Promise<boolean> {
  if (items.length === 0) return true
  const localFiles = localPaneRef.value?.getLocalFiles() ?? []
  const names = new Set(items.map((f) => f.file_name.toLowerCase()))
  const conflicts = localFiles
    .filter((f) => names.has(f.file_name.toLowerCase()))
    .map((f) => f.file_name)
  if (conflicts.length === 0) return true
  const content =
    conflicts.length === 1 && items.length === 1
      ? t("sftp.dialog.overwriteConfirm", { name: conflicts[0]! })
      : `${conflicts.slice(0, 5).join(", ")}${
          conflicts.length > 5 ? ` …(+${conflicts.length - 5})` : ""
        }`
  return await new Promise<boolean>((resolve) => {
    dialog.warning({
      title: t("sftp.dialog.overwriteTitle"),
      content,
      positiveText: t("common.overwrite"),
      negativeText: t("common.cancel"),
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false),
    })
  })
}

async function onDownload(file: SftpFile) {
  if (!props.sid) return
  if (file.file_type !== "file") {
    message.warning(t("sftp.message.downloadOnlyFile"))
    return
  }
  // 双栏模式：直接落盘到本地栏当前目录，不弹另存为对话框（同名先确认）
  if (dualPane.value && localDir.value) {
    if (await confirmDownloadOverwrite([file])) {
      await downloadToLocalDir(file)
    }
    return
  }
  const confirmed = await confirmDownload(
    t("sftp.dialog.downloadConfirm", { name: file.file_name }),
  )
  if (!confirmed) return
  await downloadViaBrowser(file)
}

/** 批量下载（单栏：一次确认后逐个经浏览器下载）。
 *  双栏批量（含目录）走 downloadEntries。 */
async function onDownloadMulti(files: SftpFile[]) {
  if (!props.sid || files.length === 0) return
  const confirmed = await confirmDownload(
    t("sftp.dialog.downloadConfirmMulti", { count: files.length }),
  )
  if (!confirmed) return
  for (const f of files) {
    await downloadViaBrowser(f)
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
  const stopPolling = pollDirectTransferProgress("download", sid, taskId, task.total)
  try {
    const { bytes } = await downloadToLocal(sid, file.full_path, localDir.value, {
      signal: ctrl.signal,
      taskId,
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
  } finally {
    stopPolling()
  }
}

/** 递归收集远程目录树为文件清单（rel 相对顶层目录）。
 *  不跟随符号链接，深度上限防御异常深/自引用结构。 */
async function walkRemoteDir(
  remoteDir: string,
  prefix: string,
  list: Array<{ rel: string; file: SftpFile }>,
  depth = 0,
): Promise<void> {
  if (!props.sid) return
  if (depth > 32) return
  const resp = await listSftp(props.sid, remoteDir)
  for (const f of resp.files) {
    if (f.file_type === "symlink") continue
    const rel = prefix ? `${prefix}/${f.file_name}` : f.file_name
    if (f.file_type === "dir") {
      await walkRemoteDir(f.full_path, rel, list, depth + 1)
    } else {
      list.push({ rel, file: f })
    }
  }
}

/** 远程目录整树下载到本地栏当前目录（Rust 直传，逐文件落
 *  <本地当前目录>/<顶层目录名>/<相对路径>；目标子目录链由后端
 *  download_to_local 的 create_dir_all 自动创建，空目录不会被创建）。
 *  顶层同名确认由调用方 downloadEntries 统一处理。 */
async function downloadRemoteDirTree(dirRow: SftpFile) {
  if (!props.sid || dirRow.file_type !== "dir" || !localDir.value) return
  const sid = props.sid
  const topName = dirRow.file_name
  const list: Array<{ rel: string; file: SftpFile }> = []
  try {
    await walkRemoteDir(dirRow.full_path, "", list)
  } catch (e) {
    message.error(t("sftp.message.loadFailed", { error: (e as Error).message }))
    return
  }
  let okCount = 0
  let failCount = 0
  for (const ent of list) {
    const relDir = ent.rel.includes("/")
      ? ent.rel.slice(0, ent.rel.lastIndexOf("/"))
      : ""
    const targetDir = `${localDir.value}/${topName}${relDir ? `/${relDir}` : ""}`
    const ctrl = new AbortController()
    const taskId = genId()
    const total = ent.file.size_bytes ?? 0
    const task: TransferTask = {
      id: taskId,
      sid,
      filename: ent.file.full_path,
      remoteDir: dirRow.full_path,
      total,
      loaded: 0,
      status: "running",
      controller: ctrl,
      startedAt: Date.now(),
    }
    store.addDownload(sid, task)
    const stopPolling = pollDirectTransferProgress("download", sid, taskId, total)
    try {
      const { bytes } = await downloadToLocal(sid, ent.file.full_path, targetDir, {
        signal: ctrl.signal,
        taskId,
      })
      const done = bytes > 0 ? bytes : total
      store.updateDownload(sid, taskId, { loaded: done, total, status: "done" })
      okCount++
    } catch (e) {
      if (isAbortError(e)) {
        store.updateDownload(sid, taskId, { status: "cancelled" })
      } else {
        store.updateDownload(sid, taskId, {
          status: "error",
          error: (e as Error).message,
        })
      }
      failCount++
    } finally {
      stopPolling()
    }
  }
  if (failCount === 0) {
    message.success(t("sftp.message.dirDownloadDone", { count: okCount }))
  } else {
    message.warning(t("sftp.message.dirDownloadPartial", { ok: okCount, fail: failCount }))
  }
  localPaneRef.value?.refresh()
}

/** 双栏下载分流：文件直落本地栏目录 + 目录整树递归（同名一次确认，
 *  文件名与目录顶层名一起列出） */
async function downloadEntries(items: SftpFile[]) {
  if (!props.sid || items.length === 0 || !localDir.value) return
  if (!(await confirmDownloadOverwrite(items))) return
  const files = items.filter((f) => f.file_type === "file")
  const dirs = items.filter((f) => f.file_type === "dir")
  for (const f of files) {
    await downloadToLocalDir(f)
  }
  for (const d of dirs) {
    await downloadRemoteDirTree(d)
  }
}

/** 双栏模式：本地栏条目上传远程当前目录--文件批量直传（Rust 进程内
 *  流式中转），目录递归上传（onLocalDirUpload，顶层同名确认各自弹窗） */
async function onLocalUpload(sel: SftpFile[]) {
  if (!props.sid || sel.length === 0) return
  const sid = props.sid
  const upFiles = sel.filter((f) => f.file_type === "file")
  const upDirs = sel.filter((f) => f.file_type === "dir")

  // 文件同名覆盖确认：只问一次，列出冲突名
  if (upFiles.length > 0) {
    const names = new Set(upFiles.map((f) => f.file_name.toLowerCase()))
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
  }

  for (const f of upFiles) {
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
    const stopPolling = pollDirectTransferProgress("upload", sid, taskId, total)
    try {
      await uploadLocalToRemote(sid, f.full_path, remotePath, {
        signal: ctrl.signal,
        taskId,
      })
      store.updateUpload(sid, taskId, { loaded: total, total, status: "done" })
    } catch (e) {
      if (isAbortError(e)) {
        store.updateUpload(sid, taskId, { status: "cancelled" })
      } else {
        const err = e as Error
        store.updateUpload(sid, taskId, { status: "error", error: err.message })
        message.error(t("sftp.message.uploadFailed", { error: err.message }))
      }
    } finally {
      stopPolling()
    }
  }
  for (const d of upDirs) {
    await onLocalDirUpload(d)
  }
  await load()
}

/** 递归收集本地目录树：dirs 为相对路径目录集，list 为文件清单
 *  （rel 相对顶层目录）。不跟随符号链接，深度上限防御自引用环。 */
async function walkLocalDir(
  localDir: string,
  prefix: string,
  dirs: Set<string>,
  list: Array<{ rel: string; path: string; size: number }>,
  depth = 0,
): Promise<void> {
  if (depth > 32) return
  const resp = await listLocalFs(localDir)
  for (const f of resp.files) {
    if (f.file_type === "symlink") continue
    const rel = prefix ? `${prefix}/${f.file_name}` : f.file_name
    if (f.file_type === "dir") {
      dirs.add(rel)
      await walkLocalDir(f.full_path, rel, dirs, list, depth + 1)
    } else {
      list.push({ rel, path: f.full_path, size: f.size_bytes ?? 0 })
    }
  }
}

/** 本地栏右键"上传此目录"：把本地目录树整体上传到远程当前目录
 *  （复用 Rust 进程内直传、上传任务列表与进度轮询） */
async function onLocalDirUpload(dirRow: SftpFile) {
  if (!props.sid || dirRow.file_type !== "dir") return
  const sid = props.sid
  const topName = dirRow.file_name
  const baseDir = currentPath.value

  // 顶层同名确认（与"上传文件夹"按钮一致）
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

  // 收集目录树（loopback 本地盘遍历）
  const dirs = new Set<string>()
  const list: Array<{ rel: string; path: string; size: number }> = []
  try {
    await walkLocalDir(dirRow.full_path, "", dirs, list)
  } catch (e) {
    message.error(t("sftp.localPane.loadFailed", { error: (e as Error).message }))
    return
  }

  // 预建目录：顶层 + 子目录按层级排序；已存在导致的失败忽略（覆盖场景下属预期）
  const allDirs = [topName, ...dirs].sort((a, b) => {
    const da = a.split("/").length
    const db = b.split("/").length
    if (da !== db) return da - db
    return a.localeCompare(b)
  })
  for (const rel of allDirs) {
    try {
      await mkdirApi(sid, joinPath(baseDir, rel))
    } catch {
      // ignore：目录已存在等，继续传文件
    }
  }

  // 串行直传 + 任务/进度
  let okCount = 0
  let failCount = 0
  for (const ent of list) {
    const remotePath = joinPath(baseDir, `${topName}/${ent.rel}`)
    const ctrl = new AbortController()
    const taskId = genId()
    const task: TransferTask = {
      id: taskId,
      sid,
      filename: remotePath,
      remoteDir: parentPath(remotePath),
      total: ent.size,
      loaded: 0,
      status: "running",
      controller: ctrl,
      startedAt: Date.now(),
    }
    store.addUpload(sid, task)
    const stopPolling = pollDirectTransferProgress("upload", sid, taskId, ent.size)
    try {
      await uploadLocalToRemote(sid, ent.path, remotePath, {
        signal: ctrl.signal,
        taskId,
      })
      store.updateUpload(sid, taskId, {
        loaded: ent.size,
        total: ent.size,
        status: "done",
      })
      okCount++
    } catch (e) {
      if (isAbortError(e)) {
        store.updateUpload(sid, taskId, { status: "cancelled" })
      } else {
        store.updateUpload(sid, taskId, {
          status: "error",
          error: (e as Error).message,
        })
      }
      failCount++
    } finally {
      stopPolling()
    }
  }
  if (failCount === 0) {
    message.success(t("sftp.message.dirUploadDone", { count: okCount }))
  } else {
    message.warning(t("sftp.message.dirUploadPartial", { ok: okCount, fail: failCount }))
  }
  await load()
}

/** Rust 进程内直传（本地<->远端）没有浏览器字节流，进度靠轮询后端
 *  计数器（按 task_id 记账）。返回停止函数，在传输的 finally 里调用。 */
function pollDirectTransferProgress(
  kind: "upload" | "download",
  sid: string,
  taskId: string,
  total: number,
): () => void {
  const timer = window.setInterval(() => {
    void transferProgress([taskId])
      .then((map) => {
        const bytes = map[taskId]
        if (typeof bytes !== "number") return
        const patch = { loaded: bytes, total: total > 0 ? total : bytes }
        if (kind === "upload") store.updateUpload(sid, taskId, patch)
        else store.updateDownload(sid, taskId, patch)
      })
      .catch(() => {
        // 轮询失败不影响传输本身
      })
  }, 300)
  return () => window.clearInterval(timer)
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

/** webkitRelativePath 形如 "myfolder/sub/a.txt"（共享类型 OsDropEntry） */
type FolderEntry = OsDropEntry

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
    } catch {
      // 目录已存在等：忽略并继续写文件（顶层同名已做过覆盖确认）
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
 * （uploadStream 的 XHR 进度 / 取消；文件夹复用 uploadFolderEntries）。
 * 双栏下按落区路由：拖到本地栏 = 复制到本地目录（LocalPane.importOsFiles），
 * 拖到其余区域 = 上传远程当前目录。 */

const dropHover = ref(false)
/** 拖拽悬停的落区：决定遮罩文案与 drop 行为 */
const dropHoverZone = ref<"" | "local" | "remote">("")

function dropZoneOf(e: DragEvent): string | undefined {
  const el = (e.target as HTMLElement | null)?.closest?.(
    "[data-drop-zone]",
  ) as HTMLElement | null
  return el?.dataset.dropZone
}

function onPanelDragOver(e: DragEvent) {
  if (!props.open || !props.sid) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = "copy"
  dropHover.value = true
  dropHoverZone.value =
    dualPane.value && dropZoneOf(e) === "local" ? "local" : "remote"
}

function onPanelDragLeave() {
  dropHover.value = false
  dropHoverZone.value = ""
}

async function onPanelDrop(e: DragEvent) {
  e.preventDefault()
  dropHover.value = false
  dropHoverZone.value = ""
  if (!props.open || !props.sid) return
  const dt = e.dataTransfer
  if (!dt || dt.items.length === 0) return

  // 双栏下拖进本地栏：复制到本地目录，而不是上传远程
  const dropToLocal = dualPane.value && dropZoneOf(e) === "local"

  const topFiles: File[] = []
  const folders: OsDropFolder[] = []

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

  if (dropToLocal) {
    await localPaneRef.value?.importOsFiles(topFiles, folders)
    return
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
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.dir }, { default: () => h(Folder) })
  }
  if (file.file_type === "symlink") {
    return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.symlink }, { default: () => h(Link) })
  }
  return h(NIcon, { size: 16, color: FILE_TYPE_COLORS.file }, { default: () => h(FileRegular) })
}

function dirFirst(a: SftpFile, b: SftpFile): number {
  const da = a.file_type === "dir" ? 0 : 1
  const db = b.file_type === "dir" ? 0 : 1
  return da - db
}

/* 列排序必须受控：sorter 若交给 NDataTable 内部做，点列头后显示顺序
   与 files.value 会错位，而 Shift 区间选择按 files.value 索引取区间，
   错位就跳选。这里持有排序状态并在点列头时重排 files.value。 */
function cmpDefault(a: SftpFile, b: SftpFile): number {
  const d = dirFirst(a, b)
  if (d !== 0) return d
  return a.file_name.toLowerCase().localeCompare(b.file_name.toLowerCase())
}

function cmpSize(a: SftpFile, b: SftpFile): number {
  const d = dirFirst(a, b)
  if (d !== 0) return d
  const sa = typeof a.size_bytes === "number" ? a.size_bytes : -1
  const sb = typeof b.size_bytes === "number" ? b.size_bytes : -1
  return sa - sb
}

function cmpMtime(a: SftpFile, b: SftpFile): number {
  const d = dirFirst(a, b)
  if (d !== 0) return d
  const ma = typeof a.mtime === "number" ? a.mtime : null
  const mb = typeof b.mtime === "number" ? b.mtime : null
  if (ma === null && mb === null) return 0
  if (ma === null) return 1
  if (mb === null) return -1
  return ma - mb
}

const sortState = ref<{
  columnKey: string | null
  order: false | "ascend" | "descend"
}>({ columnKey: null, order: false })

/** 按当前排序状态重排列表（descend 反转；无排序时回到默认目录优先+名字） */
function applySort(list: SftpFile[]): SftpFile[] {
  const cmp =
    sortState.value.columnKey === "size"
      ? cmpSize
      : sortState.value.columnKey === "mtime"
        ? cmpMtime
        : cmpDefault
  const sorted = [...list].sort(cmp)
  return sortState.value.order === "descend" ? sorted.reverse() : sorted
}

function onRemoteSort(s: DataTableSortState | DataTableSortState[]) {
  const st = Array.isArray(s) ? s[s.length - 1] : s
  if (!st) return
  sortState.value = {
    columnKey: st.columnKey != null ? String(st.columnKey) : null,
    order: st.order,
  }
  files.value = applySort(files.value)
}

/** 点击表格空白区清空选择集；点表头（排序/调列宽）不清空，避免丢选择 */
function onRemoteTableClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  if (target?.closest(".n-data-table-th, .n-data-table-tr")) return
  clearRemoteSelection()
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
    sorter: cmpDefault,
    sortOrder: sortState.value.columnKey === "file_name" ? sortState.value.order : false,
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
    sorter: cmpSize,
    sortOrder: sortState.value.columnKey === "size" ? sortState.value.order : false,
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
    sorter: cmpMtime,
    sortOrder: sortState.value.columnKey === "mtime" ? sortState.value.order : false,
    render(row) {
      return formatUnix(row.mtime ?? null)
    },
  },
])

const rowKey = (row: SftpFile) => row.full_path

function rowProps(row: SftpFile) {
  return {
    class: isRemoteSelected(row) ? "row-selected" : "",
    style: {
      // 双栏模式下文件/目录行可拖到本地栏（目录为整树下载），提示 grab
      cursor:
        dualPane.value &&
        (row.file_type === "file" || row.file_type === "dir")
          ? "grab"
          : "default",
    },
    onPointerdown: (e: PointerEvent) => {
      // Shift+单击的默认行为是扩展文本选择，须在 pointerdown 阶段拦掉
      // （click 阶段已经选完了）
      if (e.shiftKey) e.preventDefault()
      if (dualPane.value) remoteDrag.onRowPointerdown(row, e)
    },
    onClick: (e: MouseEvent) => {
      e.stopPropagation()
      onRemoteRowClick(row, e)
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
      // 资源管理器语义：右键未选中的行时独占选中，已选中则保持集合
      if (!isRemoteSelected(row)) selectRemoteExclusive(row)
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
  // 多选集 >1 时下载项升级为批量（右键行必然在选择集内）
  const selCount = remoteSelectedFiles.value.length
  // 下载项：文件行恒有；目录行仅双栏有（单栏无本地目标目录）
  if (target.file_type === "file" || (target.file_type === "dir" && dualPane.value)) {
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
    }
    opts.push({
      label:
        selCount > 1
          ? t("sftp.ctxMenu.downloadMulti", { count: selCount })
          : target.file_type === "dir"
            ? t("sftp.ctxMenu.downloadDir")
            : t("sftp.ctxMenu.download"),
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
  if (startupStore.aiAssistantEnabled && !props.standalone) {
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
  else if (key === "download") {
    if (dualPane.value && localDir.value) {
      // 双栏：作用于整个选择集（文件直落 + 目录整树，downloadEntries 分流）
      const items =
        remoteSelectedFiles.value.length > 1 ? remoteSelectedFiles.value : [target]
      void downloadEntries(items)
    } else {
      // 单栏：浏览器逐个下载（目录行在单栏不显示下载项）
      const multi = remoteSelectedFiles.value.filter((f) => f.file_type === "file")
      if (multi.length > 1) void onDownloadMulti(multi)
      else void onDownload(target)
    }
  } else if (key === "edit") openEditor(target)
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

/** 本地栏选中集（LocalPane select 上报，支持多选；中间条按钮据此启停） */
const localSelectedFiles = ref<SftpFile[]>([])

/* ---------- 远程侧拖拽：远程行 -> 本地栏（下载） ---------- */

const remoteDrag = useFileDrag({
  collectFiles(row) {
    // 行在选择集内则拖整个选择集（含目录行），否则拖当前行；
    // 文件/目录分流由 downloadEntries 处理（WinSCP 语义）
    return collectRemoteForTransfer(row)
  },
  onDrop(files, zone) {
    if (zone === "local" && localDir.value) {
      void downloadEntries(files)
    }
  },
})

/** 中间条 -> ：上传本地栏选中条目（文件直传/目录递归由 onLocalUpload 分流） */
function transferUp() {
  if (localSelectedFiles.value.length === 0) return
  void onLocalUpload(localSelectedFiles.value)
}

/** 中间条 <- ：下载远程选中条目到本地栏当前目录（文件直落/目录整树，
 *  分流与同名确认由 downloadEntries 处理） */
function transferDown() {
  if (remoteSelectedFiles.value.length === 0) return
  void downloadEntries(remoteSelectedFiles.value)
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

const panelStyle = computed(() => {
  // 独立窗口模式：铺满窗口，由 .standalone 类接管布局
  if (props.standalone) return {}
  return {
    width: `${width.value}px`,
    transition: resizing.value ? "none" : "transform 0.25s ease, box-shadow 0.15s ease",
    transform: props.open ? "translateX(0)" : "translateX(100%)",
  }
})

function onClose() {
  // 独立窗口模式下"关闭"语义是关掉整个窗口
  if (props.standalone) {
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
      getCurrentWindow().close(),
    )
    return
  }
  emit("update:open", false)
}

/** 弹出到独立窗口（复用当前 SSH 会话）。
 *  "移动"语义：同一会话同一视图，弹出后收起本抽屉，避免挡住终端 */
function openInStandaloneWindow() {
  if (!props.sid) return
  void openSftpInNewWindow({
    sid: props.sid,
    title: props.hostName ?? "SFTP",
    addr: props.hostAddr,
  })
  emit("update:open", false)
}
</script>

<template>
  <Teleport to="body">
    <aside
      ref="panelRef"
      class="sftp-panel"
      :class="{ open: props.open, resizing: resizing, standalone: props.standalone }"
      :style="panelStyle"
      :aria-hidden="!props.open"
      @dragover="onPanelDragOver"
      @dragleave="onPanelDragLeave"
      @drop="onPanelDrop"
    >
      <div
        v-if="!props.standalone"
        class="resize-handle"
        :title="t('common.dragToResize')"
        @pointerdown="onResizeStart"
      />

      <!-- OS 拖放文件进入面板时的提示遮罩（按落区区分：本地栏复制 / 远程上传） -->
      <div v-if="dropHover && props.sid" class="drop-overlay">
        <div class="drop-overlay-inner">
          <NIcon :size="28">
            <DownloadOutline v-if="dropHoverZone === 'local'" />
            <CloudUploadOutline v-else />
          </NIcon>
          <span>
            {{
              dropHoverZone === "local"
                ? t("sftp.localPane.dropHint", { dir: localDir })
                : t("sftp.dropHint", { path: currentPath })
            }}
          </span>
        </div>
      </div>

      <!-- 独立窗口模式下不渲染面板头：标题由 SftpWindow 标题栏承担，避免双标题 -->
      <header v-if="!props.standalone" class="panel-header">
        <span class="drawer-title">{{ drawerTitle }}</span>
        <NSpace :size="6" align="center" :wrap="false">
          <NButton
            v-if="!props.standalone"
            size="small"
            quaternary
            circle
            :disabled="!props.sid"
            :title="t('sftp.openInNewWindow')"
            @click="openInStandaloneWindow"
          >
            <template #icon>
              <NIcon><OpenOutline /></NIcon>
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
            :sid="props.sid ?? ''"
            @update:dir="persistLocalDir"
            @select="localSelectedFiles = $event"
            @transfer-up="onLocalUpload"
            @transfer-dir-up="onLocalDirUpload"
            @upload-selection="transferUp"
            @copy-started="downloadModalOpen = true"
          />
          <div v-if="dualPane" class="transfer-bar">
            <NTooltip placement="left">
              <template #trigger>
                <NButton
                  size="small"
                  secondary
                  :disabled="!props.sid || localSelectedFiles.length === 0"
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
                    !props.sid || !localDir || remoteSelectedFiles.length === 0
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
            <span v-if="dualPane" class="pane-label">{{ t("sftp.remotePaneTitle") }}</span>
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
            <NButton
              size="small"
              quaternary
              circle
              :title="t('sftp.refresh')"
              @click="refresh"
            >
              <template #icon>
                <NIcon><RefreshOutline /></NIcon>
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

          <NSpin :show="loading" class="table-wrap" @click="onRemoteTableClick">
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
              @update:sorter="onRemoteSort"
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

/* 独立窗口模式：面板铺满窗口（顶部让位给窗口标题栏），无侧边阴影 */
.sftp-panel.standalone {
  left: 0;
  right: 0;
  width: auto;
  border-left: none;
}

.sftp-panel.standalone.open {
  box-shadow: none;
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

/* 与 LocalPane 的 pane-label 同款式（双栏时两栏对称标识） */
.pane-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-muted);
  letter-spacing: 0.5px;
  flex-shrink: 0;
  white-space: nowrap;
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
