<script setup lang="ts">
import { computed, h, nextTick, onMounted, ref, watch } from "vue"
import {
  NButton,
  NDataTable,
  NDropdown,
  NEmpty,
  NIcon,
  NInput,
  NSpin,
  useDialog,
  useMessage,
} from "naive-ui"
import type {
  DataTableColumns,
  DataTableSortState,
  DropdownOption,
  InputInst,
} from "naive-ui"
import {
  ArrowUpOutline,
  CheckmarkDoneOutline,
  CloudUploadOutline,
  CopyOutline,
  CreateOutline,
  CutOutline,
  EyeOffOutline,
  EyeOutline,
  FolderOpenOutline,
  HomeOutline,
  OpenOutline,
  RefreshOutline,
  TrashOutline,
  TrashSharp,
} from "@vicons/ionicons5"
import { FileRegular, Folder, HddRegular } from "@vicons/fa"
import { useI18n } from "vue-i18n"
import {
  copyLocalFs,
  createLocalFile,
  listLocalFs,
  listLocalFsRoots,
  mkdirLocalFs,
  moveLocalFs,
  openLocalFs,
  removeLocalFs,
  renameLocalFs,
  revealLocalFs,
  saveLocalFile,
  trashLocalFs,
} from "@/api/local"
import MkdirDialog from "@/components/sftp/MkdirDialog.vue"
import RenameDialog from "@/components/sftp/RenameDialog.vue"
import { isAbortError } from "@/api/sftp"
import { useSftpStore } from "@/stores/sftp"
import type { OsDropFolder, SftpFile, TransferTask } from "@/types"
import { formatUnix } from "@/utils/time"
import { humanSize } from "@/utils/humanSize"
import { useFileDrag } from "@/composables/useFileDrag"
import { useMultiSelect } from "@/composables/useMultiSelect"

const props = defineProps<{
  /** 双栏打开时父组件持久化的本地目录（v-model:dir） */
  dir: string
  /** SFTP 会话 id：复制任务挂到该会话的下载列表 */
  sid: string
}>()

const emit = defineEmits<{
  (e: "update:dir", dir: string): void
  /** 选中集变化（支持 Ctrl/Shift 多选，中间条上传按钮据此启停） */
  (e: "select", files: SftpFile[]): void
  /** 本地文件拖放到远程栏（或直接上传请求） */
  (e: "transfer-up", files: SftpFile[]): void
  /** 右键目录"上传此目录"：由父组件递归收集并整体上传到远程当前目录 */
  (e: "transfer-dir-up", dir: SftpFile): void
  /** 右键"上传选中项"（多选）：复用父组件 -> 按钮的分流逻辑 */
  (e: "upload-selection"): void
  /** OS 拖放复制任务开始（父组件据此自动打开下载列表弹窗显示进度） */
  (e: "copy-started"): void
}>()

const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()
const store = useSftpStore()

const files = ref<SftpFile[]>([])
const loading = ref(false)
const currentPath = ref(props.dir || "")
/** "此电脑"页：展示盘符（Windows）/ 根（Unix），非真实目录 */
const viewingRoots = ref(false)

/** 隐藏文件（dotfile）显示开关：默认隐藏，与 Finder/资源管理器一致 */
const showHidden = ref(false)
/** 表格实际渲染的列表（多选基于该列表） */
const displayFiles = computed(() =>
  showHidden.value ? files.value : files.value.filter((f) => !f.file_name.startsWith(".")),
)
const pathEditing = ref(false)
const pathDraft = ref("")
const pathInputRef = ref<InputInst | null>(null)

const ctxMenuVisible = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuTarget = ref<SftpFile | null>(null)

/* ---------- 多选（与远程栏共用 useMultiSelect 语义） ---------- */

const {
  selectedFiles,
  isSelected,
  selectExclusive,
  onRowClick: onRowSelect,
  collectForTransfer,
  clearSelection,
  selectAll,
} = useMultiSelect(displayFiles)

/** 本地栏剪贴板（内部复制/剪切 -> 粘贴；cut 粘贴 = 移动，跨盘自动回退复制+删除） */
const localClipboard = ref<{ op: "copy" | "cut"; files: SftpFile[] } | null>(null)

/** 新建文件夹 / 新建文件弹窗（复用远程栏同款组件） */
const mkdirOpen = ref(false)
const mkdirMode = ref<"mkdir" | "touch">("mkdir")
const renameOpen = ref(false)
const renameTarget = ref<SftpFile | null>(null)

/* ---------- 拖拽：本地 -> 远程（上传） ---------- */

const { dragging, ghostX, ghostY, dragCount, onRowPointerdown } = useFileDrag({
  collectFiles(row) {
    if (viewingRoots.value) return []
    // 集合含目录行，文件/目录分流由父组件 onLocalUpload 处理（WinSCP 语义）
    return collectForTransfer(row)
  },
  onDrop(files, zone) {
    if (zone === "remote") {
      emit("transfer-up", files)
    }
  },
})

/** 盘符根（D:\ / C:/）判定：可继续上溯到"此电脑"页 */
function isDriveRoot(p: string): boolean {
  return /^[a-zA-Z]:[\\/]?$/.test(p.trim())
}

function canGoUp(): boolean {
  if (viewingRoots.value) return false
  const p = currentPath.value.replace(/[\\/]+$/, "")
  if (!p || p === "/") return false
  if (isDriveRoot(p)) return true
  return !/^[a-zA-Z]:$/.test(p)
}

/** 本地路径的父目录：同时接受 \ 与 / 分隔符，保留路径原有分隔符风格
 *  （utils/pathJoin 的 parentPath 会把 D:\a 规范化成 D:/a，显示会漂移） */
function localParentPath(p: string): string {
  const idx = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"))
  if (idx < 0) return p
  const parent = p.slice(0, idx)
  if (!parent) return "/"
  // 截到只剩盘符（D:）时补回反斜杠，得到盘符根 D:\
  if (/^[a-zA-Z]:$/.test(parent)) return `${parent}\\`
  return parent
}

async function load(path?: string) {
  loading.value = true
  try {
    const resp = await listLocalFs(path ?? currentPath.value)
    files.value = applySort(resp.files)
    currentPath.value = resp.path
    viewingRoots.value = false
    clearSelection()
    emit("select", [])
    emit("update:dir", resp.path)
  } catch (e) {
    message.error(t("sftp.localPane.loadFailed", { error: (e as Error).message }))
    files.value = []
  } finally {
    loading.value = false
  }
}

/** "此电脑"：Windows 盘符 / Unix 根 */
async function goRoots() {
  loading.value = true
  try {
    const resp = await listLocalFsRoots()
    files.value = applySort(resp.files)
    viewingRoots.value = true
    clearSelection()
    emit("select", [])
  } catch (e) {
    message.error(t("sftp.localPane.loadFailed", { error: (e as Error).message }))
  } finally {
    loading.value = false
  }
}

function refresh(): Promise<void> {
  return viewingRoots.value ? goRoots() : load()
}

function goUp() {
  if (!canGoUp()) return
  if (isDriveRoot(currentPath.value.trim())) {
    void goRoots()
    return
  }
  void load(localParentPath(currentPath.value))
}

function goHome() {
  void load(undefined)
}

function enterDir(file: SftpFile) {
  if (file.file_type === "dir") {
    void load(file.full_path)
  }
}

/* ---------- 地址栏编辑（对齐远程栏交互） ---------- */

const displayPath = computed(() =>
  viewingRoots.value ? t("sftp.localPane.thisPc") : currentPath.value,
)

function startPathEdit() {
  pathDraft.value = viewingRoots.value ? "" : currentPath.value
  pathEditing.value = true
  void nextTick().then(() => {
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
  const next = pathDraft.value.trim()
  pathEditing.value = false
  pathDraft.value = ""
  if (!next) return
  if (next !== currentPath.value) {
    void load(next)
  }
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

/* ---------- 表格 ---------- */

/** 目录优先（与远程栏排序行为对齐） */
function dirFirst(a: SftpFile, b: SftpFile): number {
  const da = a.file_type === "dir" ? 0 : 1
  const db = b.file_type === "dir" ? 0 : 1
  return da - db
}

/* 列排序必须受控（同远程栏）：sorter 交给 NDataTable 内部做会导致显示
   顺序与 files.value 错位，Shift 区间选择按 files.value 索引取区间会跳选 */
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

function onLocalSort(s: DataTableSortState | DataTableSortState[]) {
  const st = Array.isArray(s) ? s[s.length - 1] : s
  if (!st) return
  sortState.value = {
    columnKey: st.columnKey != null ? String(st.columnKey) : null,
    order: st.order,
  }
  files.value = applySort(files.value)
}

/** 点击表格空白区清空选择集；点表头（排序/调列宽）不清空，避免丢选择 */
function onTableAreaClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  if (target?.closest(".n-data-table-th, .n-data-table-tr")) return
  clearSelection()
  emit("select", [])
}

function fileIcon(row: SftpFile) {
  return h(
    NIcon,
    {
      size: 16,
      class: "file-icon",
      style: {
        color:
          row.file_type === "dir"
            ? "var(--ashell-accent, #7c5cff)"
            : undefined,
      },
    },
    () =>
      viewingRoots.value
        ? h(HddRegular)
        : row.file_type === "dir"
          ? h(Folder)
          : h(FileRegular),
  )
}

const columns = computed<DataTableColumns<SftpFile>>(() => [
  {
    title: t("sftp.columns.name"),
    key: "file_name",
    minWidth: 150,
    resizable: true,
    ellipsis: { tooltip: true },
    sorter: cmpDefault,
    sortOrder: sortState.value.columnKey === "file_name" ? sortState.value.order : false,
    render(row) {
      return h("div", { class: "name-cell" }, [
        fileIcon(row),
        h("span", { class: "name-text", title: row.full_path }, [
          row.file_name,
          row.file_type === "symlink" && row.link_path ? ` -> ${row.link_path}` : "",
        ]),
      ])
    },
  },
  {
    title: t("sftp.columns.size"),
    key: "size",
    width: 84,
    resizable: true,
    sorter: cmpSize,
    sortOrder: sortState.value.columnKey === "size" ? sortState.value.order : false,
    render(row) {
      if (row.file_type === "dir") return "-"
      return typeof row.size_bytes === "number" ? humanSize(row.size_bytes) : row.size || "-"
    },
  },
  {
    title: t("sftp.columns.modifyTime"),
    key: "mtime",
    // 与远程栏的 170 对齐：140 会放不下完整时间导致换行
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
    class: [
      isSelected(row) ? "row-selected" : "",
      isCutRowLocal(row) ? "row-cut" : "",
    ]
      .filter(Boolean)
      .join(" "),
    style: {
      // 文件/目录行都可拖到远程栏（目录为递归上传），提示 grab
      cursor:
        !viewingRoots.value &&
        (row.file_type === "file" || row.file_type === "dir")
          ? "grab"
          : "default",
    },
    onPointerdown: (e: PointerEvent) => {
      // Shift+单击的默认行为是扩展文本选择，须在 pointerdown 阶段拦掉
      // （click 阶段已经选完了）
      if (e.shiftKey) e.preventDefault()
      onRowPointerdown(row, e)
    },
    onClick: (e: MouseEvent) => {
      e.stopPropagation()
      onRowSelect(row, e)
      ctxMenuVisible.value = false
      emit("select", selectedFiles.value)
    },
    onDblclick: (e: MouseEvent) => {
      e.stopPropagation()
      if (row.file_type === "dir") {
        void enterDir(row)
      } else if (row.file_type === "file" && !viewingRoots.value) {
        // 与远程栏双击语义对齐：双击文件 = 传到对侧（上传到远程当前目录）
        emit("transfer-up", [row])
      }
    },
    onContextmenu: (e: MouseEvent) => {
      e.preventDefault()
      e.stopPropagation()
      // 资源管理器语义：右键未选中的行时独占选中，已选中则保持集合
      if (!isSelected(row)) selectExclusive(row)
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
  void nextTick().then(() => {
    ctxMenuVisible.value = true
  })
}

/** 空白处右键：弹本地栏自己的菜单，并阻止冒泡到 SftpDrawer 的
 *  .sftp-body（否则会弹出远程"新建"菜单，且操作全部作用于远程目录） */
function onBlankContextMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  openCtxMenu(e, null)
}

const ctxMenuOptions = computed<DropdownOption[]>(() => {
  const row = ctxMenuTarget.value
  if (!row) {
    // 空白区：新建/粘贴/全选（非"此电脑"页）+ 刷新 / 回此电脑
    const opts: DropdownOption[] = []
    if (!viewingRoots.value) {
      opts.push(
        {
          key: "local-mkdir",
          label: t("sftp.ctxMenu.newFolder"),
          icon: () => h(NIcon, null, () => h(Folder)),
        },
        {
          key: "local-touch",
          label: t("sftp.ctxMenu.newFile"),
          icon: () => h(NIcon, null, () => h(FileRegular)),
        },
      )
      if (localClipboard.value) {
        const clip = localClipboard.value
        opts.push({
          key: "local-paste",
          label:
            clip.files.length > 1
              ? t("sftp.ctxMenu.pasteMulti", { count: clip.files.length })
              : t("sftp.ctxMenu.paste"),
          icon: () => h(NIcon, null, () => h(CopyOutline)),
        })
      }
      opts.push(
        { type: "divider", key: "d-local-blank-1" },
        {
          key: "local-select-all",
          label: t("sftp.ctxMenu.selectAll"),
          icon: () => h(NIcon, null, () => h(CheckmarkDoneOutline)),
        },
        { type: "divider", key: "d-local-blank-2" },
      )
    }
    opts.push({
      key: "local-refresh",
      label: t("sftp.ctxMenu.refresh"),
      icon: () => h(NIcon, null, () => h(RefreshOutline)),
    })
    if (!viewingRoots.value) {
      opts.push({
        key: "local-roots",
        label: t("sftp.localPane.goRoots"),
        icon: () => h(NIcon, null, () => h(HddRegular)),
      })
    }
    return opts
  }
  const selCount = selectedFiles.value.length
  const opts: DropdownOption[] = [
    {
      key: "copy-path",
      label: t("sftp.ctxMenu.copyPath"),
      icon: () => h(NIcon, null, () => h(CopyOutline)),
    },
  ]
  // 多选（右键行必然在选择集内）：合并为批量上传项；单选按行类型单项
  if (selCount > 1) {
    opts.push({
      key: "upload-selection",
      label: t("sftp.localPane.ctxUploadMulti", { count: selCount }),
      icon: () => h(NIcon, null, () => h(CloudUploadOutline)),
    })
  } else if (row.file_type === "file" && !viewingRoots.value) {
    opts.push({
      key: "upload",
      label: t("sftp.localPane.ctxUpload"),
      icon: () => h(NIcon, null, () => h(CloudUploadOutline)),
    })
  } else if (row.file_type === "dir" && !viewingRoots.value) {
    opts.push({
      key: "upload-dir",
      label: t("sftp.localPane.ctxUploadDir"),
      icon: () => h(NIcon, null, () => h(CloudUploadOutline)),
    })
  }
  if (!viewingRoots.value) {
    opts.push(
      { type: "divider", key: "d-local-clip" },
      {
        key: "clipboard-copy",
        label:
          selCount > 1
            ? t("sftp.ctxMenu.copyMulti", { count: selCount })
            : t("sftp.ctxMenu.copy"),
        icon: () => h(NIcon, null, () => h(CopyOutline)),
      },
      {
        key: "clipboard-cut",
        label:
          selCount > 1
            ? t("sftp.ctxMenu.cutMulti", { count: selCount })
            : t("sftp.ctxMenu.cut"),
        icon: () => h(NIcon, null, () => h(CutOutline)),
      },
      {
        key: "local-rename",
        label: t("sftp.ctxMenu.rename"),
        icon: () => h(NIcon, null, () => h(CreateOutline)),
      },
    )
    opts.push(
      { type: "divider", key: "d-local-trash" },
      {
        key: "trash",
        label: t("sftp.localPane.ctxTrash"),
        icon: () => h(NIcon, null, () => h(TrashOutline)),
      },
      {
        key: "delete",
        label: t("sftp.ctxMenu.delete"),
        icon: () => h(NIcon, null, () => h(TrashSharp)),
      },
    )
    opts.push(
      { type: "divider", key: "d-local-os" },
      {
        key: "local-reveal",
        label: t("sftp.localPane.ctxReveal"),
        icon: () => h(NIcon, null, () => h(FolderOpenOutline)),
      },
      {
        key: "local-open",
        label: t("sftp.localPane.ctxOpen"),
        icon: () => h(NIcon, null, () => h(OpenOutline)),
      },
    )
  }
  return opts
})

function onCtxMenuSelect(key: string) {
  ctxMenuVisible.value = false
  const row = ctxMenuTarget.value
  if (!row) {
    if (key === "local-refresh") void refresh()
    else if (key === "local-roots") void goRoots()
    else if (key === "local-mkdir") openLocalMkdir()
    else if (key === "local-touch") openLocalTouch()
    else if (key === "local-paste") void pasteLocal()
    else if (key === "local-select-all") selectAll()
    return
  }
  if (key === "copy-path") {
    void navigator.clipboard
      .writeText(row.full_path)
      .then(() => message.success(t("sftp.message.copied")))
      .catch(() => message.error(t("sftp.message.copyFailed")))
  } else if (key === "upload") {
    if (row.file_type === "file") {
      emit("transfer-up", [row])
    }
  } else if (key === "upload-dir") {
    if (row.file_type === "dir") {
      emit("transfer-dir-up", row)
    }
  } else if (key === "upload-selection") {
    emit("upload-selection")
  } else if (key === "clipboard-copy") {
    setLocalClipboard("copy")
  } else if (key === "clipboard-cut") {
    setLocalClipboard("cut")
  } else if (key === "local-rename") {
    openLocalRename(row)
  } else if (key === "local-reveal") {
    void revealInFileManager(row)
  } else if (key === "local-open") {
    void openWithDefaultApp(row)
  } else if (key === "trash") {
    confirmTrashLocal(row)
  } else if (key === "delete") {
    confirmRemoveLocal(row)
  }
}

/* ---------- 删除本地文件（回收站 / 永久，与远程栏汇总语义一致） ---------- */

function fileTypeName(f: SftpFile): string {
  return f.file_type === "dir"
    ? t("sftp.dialog.typeFolder")
    : t("sftp.dialog.typeFile")
}

/** 逐条执行删除类操作（回收站/永久），汇总成功/失败后清空选择并刷新。 */
async function deleteLocalEntries(
  targets: SftpFile[],
  op: (path: string) => Promise<unknown>,
  successMsg: string,
) {
  if (targets.length === 0) return
  let ok = 0
  let fail = 0
  let firstError = ""
  for (const f of targets) {
    try {
      await op(f.full_path)
      ok++
    } catch (e) {
      fail++
      if (!firstError) firstError = (e as Error).message
    }
  }
  if (fail === 0) {
    message.success(successMsg)
  } else if (targets.length === 1) {
    message.error(t("sftp.message.deleteFailed", { error: firstError }))
  } else {
    message.warning(t("sftp.message.deletePartial", { ok, fail }))
  }
  clearSelection()
  await load()
}

/** 选择目标集合：右键行必在选择集内（openCtxMenu 前已补选） */
function removeTargets(file: SftpFile): SftpFile[] {
  return isSelected(file) ? selectedFiles.value : [file]
}

function confirmTrashLocal(file: SftpFile) {
  const targets = removeTargets(file)
  const content =
    targets.length > 1
      ? t("sftp.localPane.trashConfirmMulti", { count: targets.length })
      : t("sftp.localPane.trashConfirm", {
          type: fileTypeName(file),
          name: file.file_name,
        })
  dialog.info({
    title: t("sftp.localPane.ctxTrash"),
    content,
    positiveText: t("sftp.localPane.ctxTrash"),
    negativeText: t("common.cancel"),
    onPositiveClick: () => {
      void deleteLocalEntries(
        targets,
        (p) => trashLocalFs([p]),
        t("sftp.localPane.trashed"),
      )
    },
  })
}

function confirmRemoveLocal(file: SftpFile) {
  // 本地永久删除不经回收站：强警示文案
  const targets = removeTargets(file)
  const content =
    targets.length > 1
      ? t("sftp.localPane.deleteConfirmMulti", { count: targets.length })
      : t("sftp.localPane.deleteConfirm", {
          type: fileTypeName(file),
          name: file.file_name,
        })
  dialog.warning({
    title: t("sftp.dialog.deleteTitle"),
    content,
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: () => {
      void deleteLocalEntries(
        targets,
        (p) => removeLocalFs([p]),
        t("sftp.message.deleted"),
      )
    },
  })
}

/* ---------- 复制 / 剪切（移动）/ 粘贴、新建、重命名、系统打开 ---------- */

/** 复制/剪切到内部剪贴板：作用于右键行所在的选择集（批量） */
function setLocalClipboard(op: "copy" | "cut") {
  const target = ctxMenuTarget.value
  if (!target) return
  const items = isSelected(target) ? selectedFiles.value : [target]
  localClipboard.value = { op, files: [...items] }
  message.info(
    op === "copy"
      ? t("sftp.message.copiedToClipboard", { count: items.length })
      : t("sftp.message.cutToClipboard", { count: items.length }),
  )
}

function isCutRowLocal(row: SftpFile): boolean {
  const clip = localClipboard.value
  return (
    clip?.op === "cut" && clip.files.some((f) => f.full_path === row.full_path)
  )
}

/** 粘贴到当前本地目录：copy = 递归复制；cut = 移动（跨盘自动回退复制+删除）。
 *  目标存在同名时弹一次覆盖确认；逐条执行、部分失败给汇总。 */
async function pasteLocal() {
  const clip = localClipboard.value
  if (!clip || clip.files.length === 0 || viewingRoots.value || !currentPath.value) {
    return
  }
  const dstDir = currentPath.value

  const run = async () => {
    let ok = 0
    let fail = 0
    let firstError = ""
    for (const f of clip.files) {
      try {
        if (clip.op === "copy") {
          await copyLocalFs(f.full_path, dstDir)
        } else {
          await moveLocalFs(f.full_path, dstDir)
        }
        ok++
      } catch (e) {
        fail++
        if (!firstError) firstError = (e as Error).message
      }
    }
    if (fail === 0) {
      message.success(t("sftp.message.pasted"))
    } else if (clip.files.length === 1) {
      message.error(t("sftp.message.pasteFailed", { error: firstError }))
    } else {
      message.warning(t("sftp.message.pastePartial", { ok, fail }))
    }
    if (clip.op === "cut") localClipboard.value = null
    clearSelection()
    await load()
  }

  const names = new Set(clip.files.map((f) => f.file_name))
  const conflicts = displayFiles.value.filter((f) => names.has(f.file_name))
  if (conflicts.length > 0) {
    dialog.warning({
      title: t("sftp.dialog.pasteOverwriteTitle"),
      content:
        conflicts.length === 1
          ? t("sftp.dialog.pasteOverwrite", { name: conflicts[0]!.file_name })
          : t("sftp.dialog.pasteOverwriteMulti", { count: conflicts.length }),
      positiveText: t("common.confirm"),
      negativeText: t("common.cancel"),
      onPositiveClick: () => {
        void run()
      },
    })
  } else {
    void run()
  }
}

/* ---------- 新建 / 重命名（复用远程栏弹窗组件，对接本地 API） ---------- */

function openLocalMkdir() {
  mkdirMode.value = "mkdir"
  mkdirOpen.value = true
}

function openLocalTouch() {
  mkdirMode.value = "touch"
  mkdirOpen.value = true
}

async function onLocalMkdirSubmit(name: string) {
  if (viewingRoots.value || !currentPath.value) return
  const target = localJoin(currentPath.value, name)
  try {
    if (mkdirMode.value === "mkdir") {
      await mkdirLocalFs(target)
      message.success(t("sftp.message.folderCreated"))
    } else {
      await createLocalFile(target)
      message.success(t("sftp.message.fileCreated"))
    }
    mkdirOpen.value = false
    await load()
  } catch (e) {
    message.error(t("sftp.message.createFailed", { error: (e as Error).message }))
  }
}

function openLocalRename(row: SftpFile) {
  renameTarget.value = row
  renameOpen.value = true
}

async function onLocalRenameSubmit(newName: string) {
  const target = renameTarget.value
  if (!target) return
  const to = localJoin(localParentPath(target.full_path), newName)
  try {
    await renameLocalFs(target.full_path, to)
    renameOpen.value = false
    message.success(t("sftp.message.renameSuccess"))
    await load()
  } catch (e) {
    message.error(t("sftp.message.renameFailed", { error: (e as Error).message }))
  }
}

/* ---------- 系统集成：文件管理器定位 / 默认程序打开 ---------- */

async function revealInFileManager(row: SftpFile) {
  try {
    await revealLocalFs(row.full_path)
  } catch (e) {
    message.error(t("sftp.localPane.revealFailed", { error: (e as Error).message }))
  }
}

async function openWithDefaultApp(row: SftpFile) {
  try {
    await openLocalFs(row.full_path)
  } catch (e) {
    message.error(t("sftp.localPane.openFailed", { error: (e as Error).message }))
  }
}

/* ---------- OS 拖放落入本地栏：复制到当前本地目录 ---------- */

function genId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

/** 展示用路径拼接：跟随当前目录的分隔符风格（Windows 反斜杠 / Unix 正斜杠） */
function localJoin(dir: string, rel: string): string {
  const sep = dir.includes("\\") ? "\\" : "/"
  return `${dir.replace(/[\\/]+$/, "")}${sep}${rel.replace(/\//g, sep)}`
}

/** 把从资源管理器 / Finder 拖入的文件复制到本地栏当前目录。
 *  支持顶层文件与文件夹（按原层级写入，父目录自动创建）；
 *  顶层同名（文件或目录）只询问一次覆盖确认。
 *  每个文件生成一条下载列表任务：XHR 进度 + 可取消，与远程下载同一套 UI。 */
async function importOsFiles(topFiles: File[], folders: OsDropFolder[]) {
  if (viewingRoots.value || !currentPath.value) {
    message.warning(t("sftp.localPane.dropNeedsDir"))
    return
  }
  if (topFiles.length === 0 && folders.length === 0) return

  const names = new Set([
    ...topFiles.map((f) => f.name.toLowerCase()),
    ...folders.map((f) => f.name.toLowerCase()),
  ])
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

  const dir = currentPath.value
  const sid = props.sid
  if (!sid) return
  emit("copy-started")
  let okCount = 0
  let failCount = 0
  const writeOne = async (file: File, rel: string) => {
    const ctrl = new AbortController()
    const taskId = genId()
    const task: TransferTask = {
      id: taskId,
      sid,
      filename: localJoin(dir, rel),
      total: file.size,
      loaded: 0,
      status: "running",
      controller: ctrl,
      startedAt: Date.now(),
    }
    store.addDownload(sid, task)
    try {
      await saveLocalFile(dir, rel, file, {
        signal: ctrl.signal,
        onProgress: (loaded, total) => {
          store.updateDownload(sid, taskId, {
            loaded,
            total: total > 0 ? total : file.size,
          })
        },
      })
      store.updateDownload(sid, taskId, {
        loaded: file.size,
        total: file.size,
        status: "done",
      })
      okCount++
    } catch (e) {
      if (isAbortError(e)) {
        store.updateDownload(sid, taskId, { status: "cancelled" })
      } else {
        const err = e as Error
        store.updateDownload(sid, taskId, {
          status: "error",
          error: err.message,
        })
      }
      failCount++
    }
  }
  for (const f of topFiles) await writeOne(f, f.name)
  for (const folder of folders) {
    for (const ent of folder.entries) await writeOne(ent.file, ent.relPath)
  }

  await refresh()
  if (okCount + failCount === 0) return
  if (failCount === 0) {
    message.success(t("sftp.localPane.importDone", { count: okCount }))
  } else {
    message.warning(
      t("sftp.localPane.importPartial", { ok: okCount, fail: failCount }),
    )
  }
}

defineExpose({
  refresh,
  importOsFiles,
  /** 当前已加载的本地目录条目快照（父组件下载同名检查用） */
  getLocalFiles: () => files.value,
  /** 全选当前列表（SftpDrawer 的 Ctrl/Cmd+A 分发） */
  selectAll,
  /** 清空选择（SftpDrawer 的 Esc 分发） */
  clearSelection,
  /** 本地栏有弹窗/菜单/路径编辑打开：Esc 留给浮层原生处理 */
  hasOverlay: () =>
    ctxMenuVisible.value || mkdirOpen.value || renameOpen.value || pathEditing.value,
})

watch(
  () => props.dir,
  (next) => {
    // 父组件恢复持久化目录时同步（仅首次 / 外部变更）
    if (next && next !== currentPath.value && !loading.value) {
      void load(next)
    }
  },
)

onMounted(() => {
  void load(props.dir || undefined)
})
</script>

<template>
  <div class="local-pane" data-drop-zone="local" @contextmenu="onBlankContextMenu">
    <div class="path-bar">
      <span class="pane-label">{{ t("sftp.localPane.title") }}</span>
      <NButton
        size="small"
        quaternary
        circle
        :title="t('sftp.localPane.goRoots')"
        :type="viewingRoots ? 'primary' : 'default'"
        @click="goRoots"
      >
        <template #icon>
          <NIcon><HddRegular /></NIcon>
        </template>
      </NButton>
      <NButton
        size="small"
        quaternary
        circle
        :title="t('sftp.localPane.goHome')"
        @click="goHome"
      >
        <template #icon>
          <NIcon><HomeOutline /></NIcon>
        </template>
      </NButton>
      <NButton
        size="small"
        quaternary
        circle
        :title="t('sftp.goUp')"
        :disabled="!canGoUp()"
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
          :placeholder="t('sftp.localPane.pathPlaceholder')"
          class="address-input"
          @keydown="onPathKeydown"
          @blur="submitPathEdit"
        />
        <div
          v-else
          class="address-display"
          :title="displayPath"
          tabindex="0"
          role="textbox"
          @click="startPathEdit"
          @keydown.enter.prevent="startPathEdit"
          @keydown.space.prevent="startPathEdit"
        >
          {{ displayPath }}
        </div>
      </div>
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
      <NButton
        size="small"
        quaternary
        circle
        :title="showHidden ? t('sftp.hideHidden') : t('sftp.showHidden')"
        @click="showHidden = !showHidden"
      >
        <template #icon>
          <NIcon>
            <EyeOffOutline v-if="showHidden" />
            <EyeOutline v-else />
          </NIcon>
        </template>
      </NButton>
    </div>

    <NSpin :show="loading" class="table-wrap" @click="onTableAreaClick">
      <NDataTable
        v-if="files.length > 0"
        size="small"
        :columns="columns"
        :data="displayFiles"
        :row-key="rowKey"
        :row-props="rowProps"
        :bordered="false"
        :single-line="false"
        flex-height
        class="file-table"
        @update:sorter="onLocalSort"
      />
      <NEmpty
        v-else
        :description="t('sftp.localPane.empty')"
        style="margin-top: 40px"
      />
    </NSpin>

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

    <!-- 拖拽跟随标签。Teleport 到 body：SftpDrawer aside 有 transform
         （开合动画），fixed 的包含块会变成它导致坐标漂移 -->
    <Teleport to="body">
      <div
        v-if="dragging"
        class="drag-ghost"
        :style="{ left: `${ghostX}px`, top: `${ghostY}px` }"
      >
        {{ t("sftp.localPane.dragGhost", { count: dragCount }) }}
      </div>
    </Teleport>

    <!-- 新建 / 重命名：复用远程栏同款弹窗，对接本地 API -->
    <MkdirDialog
      v-model:open="mkdirOpen"
      :mode="mkdirMode"
      :current-path="currentPath"
      @submit="onLocalMkdirSubmit"
    />
    <RenameDialog
      v-model:open="renameOpen"
      :old-name="renameTarget?.file_name ?? ''"
      @submit="onLocalRenameSubmit"
    />
  </div>
</template>

<style scoped>
.local-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  gap: 6px;
}

.pane-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-muted);
  letter-spacing: 0.5px;
  flex-shrink: 0;
  white-space: nowrap;
}

.path-bar {
  display: flex;
  align-items: center;
  flex-wrap: nowrap;
  gap: 6px;
  padding: 2px 0;
  min-width: 0;
  flex-shrink: 0;
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

.table-wrap {
  flex: 1 1 auto;
  min-height: 0;
}

.table-wrap :deep(.n-spin-container),
.table-wrap :deep(.n-spin-content) {
  height: 100%;
}

.file-table {
  height: 100%;
}

/* 点选行高亮（与远程栏 .row-selected 同款） */
.file-table :deep(.n-data-table-tr.row-selected .n-data-table-td) {
  background-color: var(--ashell-row-selected, rgba(99, 153, 255, 0.16));
}

/* 剪切（待粘贴移动）的行：半透明提示"待移动" */
.file-table :deep(.n-data-table-tr.row-cut .n-data-table-td) {
  opacity: 0.45;
}

.name-cell {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.name-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
</style>
