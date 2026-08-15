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
  useMessage,
} from "naive-ui"
import type {
  DropdownOption,
  DataTableColumns,
  InputInst,
} from "naive-ui"
import {
  ArrowUpOutline,
  CloudUploadOutline,
  CopyOutline,
  DocumentOutline,
  FolderOutline,
  HomeOutline,
  RefreshOutline,
} from "@vicons/ionicons5"
import { HddRegular } from "@vicons/fa"
import { useI18n } from "vue-i18n"
import { listLocalFs, listLocalFsRoots } from "@/api/local"
import type { SftpFile } from "@/types"
import { formatUnix } from "@/utils/time"
import { humanSize } from "@/utils/humanSize"
import { useFileDrag } from "@/composables/useFileDrag"

const props = defineProps<{
  /** 双栏打开时父组件持久化的本地目录（v-model:dir） */
  dir: string
}>()

const emit = defineEmits<{
  (e: "update:dir", dir: string): void
  /** 本地勾选数变化（中间条上传按钮据此启停） */
  (e: "selection-change", count: number): void
  /** 本地文件拖放到远程栏（或直接上传请求） */
  (e: "transfer-up", files: SftpFile[]): void
}>()

const { t } = useI18n()
const message = useMessage()

const files = ref<SftpFile[]>([])
const loading = ref(false)
const currentPath = ref(props.dir || "")
/** "此电脑"页：展示盘符（Windows）/ 根（Unix），非真实目录 */
const viewingRoots = ref(false)
const checkedKeys = ref<string[]>([])
const pathEditing = ref(false)
const pathDraft = ref("")
const pathInputRef = ref<InputInst | null>(null)

const ctxMenuVisible = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuTarget = ref<SftpFile | null>(null)

const selectedFiles = computed(() =>
  files.value.filter(
    (f) => f.file_type === "file" && checkedKeys.value.includes(f.full_path),
  ),
)

/* ---------- 拖拽：本地 -> 远程（上传） ---------- */

const { dragging, ghostX, ghostY, dragCount, onRowPointerdown } = useFileDrag({
  collectFiles(row) {
    if (viewingRoots.value) return []
    // 有勾选且按在勾选行上：拖全部勾选；否则拖当前单行（WinSCP 语义）
    const sel = selectedFiles.value
    if (sel.length > 0 && sel.some((f) => f.full_path === row.full_path)) {
      return sel
    }
    return row.file_type === "file" ? [row] : []
  },
  onDrop(files, zone) {
    if (zone === "remote") {
      emit("transfer-up", files)
    }
  },
})

watch(
  checkedKeys,
  () => emit("selection-change", selectedFiles.value.length),
  { deep: true },
)

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
    files.value = resp.files
    currentPath.value = resp.path
    viewingRoots.value = false
    checkedKeys.value = []
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
    files.value = resp.files
    viewingRoots.value = true
    checkedKeys.value = []
  } catch (e) {
    message.error(t("sftp.localPane.loadFailed", { error: (e as Error).message }))
  } finally {
    loading.value = false
  }
}

function refresh() {
  if (viewingRoots.value) {
    void goRoots()
  } else {
    void load()
  }
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
          ? h(FolderOutline)
          : h(DocumentOutline),
  )
}

const columns = computed<DataTableColumns<SftpFile>>(() => [
  {
    type: "selection",
    disabled(row) {
      // 目录暂不支持递归上传；"此电脑"页不可勾选
      return viewingRoots.value || row.file_type !== "file"
    },
  },
  {
    title: t("sftp.columns.name"),
    key: "file_name",
    minWidth: 150,
    resizable: true,
    ellipsis: { tooltip: true },
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
    sorter: (a, b) => {
      const d = dirFirst(a, b)
      if (d !== 0) return d
      const sa = typeof a.size_bytes === "number" ? a.size_bytes : -1
      const sb = typeof b.size_bytes === "number" ? b.size_bytes : -1
      return sa - sb
    },
    render(row) {
      if (row.file_type === "dir") return "-"
      return typeof row.size_bytes === "number" ? humanSize(row.size_bytes) : row.size || "-"
    },
  },
  {
    title: t("sftp.columns.modifyTime"),
    key: "mtime",
    width: 140,
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
    style: {
      // 可拖拽的文件行提示 grab；目录/盘符双击进入，保持箭头
      cursor: !viewingRoots.value && row.file_type === "file" ? "grab" : "default",
    },
    onPointerdown: (e: PointerEvent) => onRowPointerdown(row, e),
    onDblclick: (e: MouseEvent) => {
      e.stopPropagation()
      enterDir(row)
    },
    onContextmenu: (e: MouseEvent) => {
      e.preventDefault()
      e.stopPropagation()
      ctxMenuTarget.value = row
      ctxMenuVisible.value = false
      ctxMenuX.value = e.clientX
      ctxMenuY.value = e.clientY
      void nextTick().then(() => {
        ctxMenuVisible.value = true
      })
    },
  }
}

/* ---------- 右键菜单 ---------- */

const ctxMenuOptions = computed<DropdownOption[]>(() => {
  const row = ctxMenuTarget.value
  const opts: DropdownOption[] = [
    {
      key: "copy-path",
      label: t("sftp.ctxMenu.copyPath"),
      icon: () => h(NIcon, null, () => h(CopyOutline)),
    },
  ]
  if (row && row.file_type === "file" && !viewingRoots.value) {
    opts.push({
      key: "upload",
      label: t("sftp.localPane.ctxUpload"),
      icon: () => h(NIcon, null, () => h(CloudUploadOutline)),
    })
  }
  return opts
})

function onCtxMenuSelect(key: string) {
  ctxMenuVisible.value = false
  const row = ctxMenuTarget.value
  if (!row) return
  if (key === "copy-path") {
    void navigator.clipboard
      .writeText(row.full_path)
      .then(() => message.success(t("sftp.message.copied")))
      .catch(() => message.error(t("sftp.message.copyFailed")))
  } else if (key === "upload") {
    if (row.file_type === "file") {
      emit("transfer-up", [row])
    }
  }
}

defineExpose({
  refresh,
  getSelectedFiles: () => selectedFiles.value,
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
  // 初始（及重新挂载后）向父组件通报一次选中数，避免中间条按钮状态残留
  emit("selection-change", 0)
  void load(props.dir || undefined)
})
</script>

<template>
  <div class="local-pane" data-drop-zone="local">
    <div class="pane-title">{{ t("sftp.localPane.title") }}</div>

    <div class="path-bar">
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
    </div>

    <NSpin :show="loading" class="table-wrap">
      <NDataTable
        v-if="files.length > 0"
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
      <NEmpty
        v-else
        :description="t('sftp.localPane.empty')"
        style="margin-top: 40px"
      />
    </NSpin>

    <div class="pane-actions">
      <span class="selection-info">
        {{ t("sftp.localPane.selectionCount", { count: selectedFiles.length }) }}
      </span>
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

.pane-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-muted);
  letter-spacing: 0.5px;
  flex-shrink: 0;
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

.pane-actions {
  display: flex;
  align-items: center;
  padding-top: 6px;
  border-top: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
  min-height: 30px;
}

.selection-info {
  font-size: 12px;
  color: var(--ashell-text-muted);
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
