<script setup lang="ts">
import { ref, watch, watchEffect, onBeforeUnmount } from "vue"
import { NButton, NIcon, NTooltip } from "naive-ui"
import { CloseOutline, FolderOpenOutline } from "@vicons/ionicons5"
import { PushpinFilled, PushpinOutlined } from "@vicons/antd"
import { useI18n } from "vue-i18n"
import LocalPane from "@/components/sftp/LocalPane.vue"
import { useLocalFilesPin } from "@/composables/useLocalFilesPin"
import { useStartupStore } from "@/stores/startup"
import type { OsDropFolder } from "@/types"

const props = defineProps<{
  open: boolean
  /** 激活本地终端上报的 cwd（OSC 9;9）；变化时抽屉跟随跳转 */
  cwd: string
  /** 跟随的本地 tab 标题（仅展示） */
  tabTitle?: string
}>()

const emit = defineEmits<{
  "update:open": [value: boolean]
  /** 右键"在终端中打开"：向绑定的本地终端发送 cd 命令 */
  "open-in-terminal": [path: string]
}>()

const { t } = useI18n()

/** 固定（停靠）态：主内容区让位并排显示，切 tab / 开其他面板不自动收起 */
const pinned = useLocalFilesPin()
/** 目录跟随开关（设置 → 启动）：开启时抽屉跟随当前终端 cwd，关闭时只手动浏览 */
const startupStore = useStartupStore()

/* ---------- 抽屉宽度拖拽（与 SftpDrawer 同款：pointer 拖动 + localStorage 持久化） ---------- */

const WIDTH_KEY = "ashell:local-files-width"
// 最小宽度 = 表格三列总宽（150+84+170=404）+ 面板左右内边距 28：
// 保证任何宽度下表格都放得下，不出现横向滚动
const MIN_WIDTH = 432
const DEFAULT_WIDTH = 460

/** 拖动上限取视口宽度的 90%，避免抽屉完全盖住主界面 */
function getMaxWidth(): number {
  return Math.round(window.innerWidth * 0.9)
}

function loadWidth(): number {
  const raw =
    typeof localStorage !== "undefined" ? localStorage.getItem(WIDTH_KEY) : null
  const n = raw ? Number(raw) : NaN
  if (!Number.isFinite(n)) return DEFAULT_WIDTH
  return Math.min(getMaxWidth(), Math.max(MIN_WIDTH, n))
}

function saveWidth(v: number) {
  try {
    localStorage.setItem(WIDTH_KEY, String(v))
  } catch {
    // ignore
  }
}

const width = ref<number>(loadWidth())
const resizing = ref(false)

function onResizeStart(e: PointerEvent) {
  e.preventDefault()
  resizing.value = true
  window.addEventListener("pointermove", onResizeMove)
  window.addEventListener("pointerup", onResizeEnd)
  window.addEventListener("pointercancel", onResizeEnd)
}

function onResizeMove(e: PointerEvent) {
  // 面板锚在右缘：宽度 = 视口宽 - 光标 X
  const next = Math.round(window.innerWidth - e.clientX)
  width.value = Math.min(getMaxWidth(), Math.max(MIN_WIDTH, next))
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

// 固定时内容区让位（App 的 .app-content right 读同一变量）；
// 变量挂 documentElement，本组件是宽度唯一持有者，在此统一维护
watchEffect(() => {
  const w = props.open && pinned.value ? `${width.value}px` : "0px"
  document.documentElement.style.setProperty("--ashell-local-files-w", w)
})

onBeforeUnmount(() => {
  document.documentElement.style.setProperty("--ashell-local-files-w", "0px")
})

/** LocalPane 实例：OS 拖入时调 importOsFiles 落盘 */
const localPaneRef = ref<InstanceType<typeof LocalPane> | null>(null)

/** 本地栏当前目录：pane 自己导航时回写（update:dir），终端 cwd 变化时跟随 */
const dir = ref("")
/** 首次打开才挂载 LocalPane：避免应用启动就发本地目录请求 */
const everOpened = ref(false)

watch(
  () => props.open,
  (open) => {
    if (open) everOpened.value = true
  },
  { immediate: true },
)

/** 路径等价比较：统一分隔符、去掉结尾分隔符、忽略大小写
 *  （shell 上报与后端 listLocalFs 归一化结果可能有 \ / 差异） */
function samePath(a: string, b: string): boolean {
  if (!a || !b) return false
  const norm = (p: string) =>
    p.replace(/\/+/g, "/").replace(/\\+/g, "\\").replace(/[\\/]$/, "").toLowerCase()
  return norm(a) === norm(b)
}

// 目录跟随：受「设置 → 启动 → 本地文件目录跟随」开关门控，且仅在抽屉打开时
// 生效——关闭期间 cwd 变化不产生任何目录列表请求，重新打开时由下方
// 同步逻辑立即跳到当前 cwd（语义：关着的抽屉不做看不见的工作）
watch(
  () => props.cwd,
  (next) => {
    if (!startupStore.cwdFollowEnabled || !props.open) return
    if (!next || !everOpened.value) return
    if (!samePath(next, dir.value)) dir.value = next
  },
)

// 打开抽屉或开启跟随开关时，立即向当前 cwd 同步一次，
// 避免首开停在主目录、或打开开关后要等下一次 cd 才生效
watch(
  [everOpened, () => startupStore.cwdFollowEnabled],
  ([opened, follow]) => {
    if (!opened || !follow) return
    const cwd = props.cwd
    if (cwd && !samePath(cwd, dir.value)) dir.value = cwd
  },
  { immediate: true },
)

function onClose() {
  emit("update:open", false)
}

/* ---------- OS 拖放落盘：从资源管理器 / Finder 拖入 = 复制到当前本地目录 ---------- */

const dropHover = ref(false)

function onDragOver(e: DragEvent) {
  if (!props.open) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = "copy"
  dropHover.value = true
}

function onDragLeave() {
  dropHover.value = false
}

/** 遍历拖入的目录为 (file, relPath) 列表（SftpDrawer 同款语义） */
async function walkDropDirectory(
  dirEntry: FileSystemDirectoryEntry,
  prefix: string,
  out: { file: File; relPath: string }[],
): Promise<void> {
  const entries = await new Promise<FileSystemEntry[]>((resolve, reject) => {
    const all: FileSystemEntry[] = []
    const reader = dirEntry.createReader()
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
        () => reject(new Error("readEntries failed")),
      )
    }
    readBatch()
  })
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

async function onDrop(e: DragEvent) {
  e.preventDefault()
  dropHover.value = false
  const dt = e.dataTransfer
  if (!props.open || !dt || dt.items.length === 0) return

  // 拖拽数据在 drop 任务结束后失效，必须同步阶段一次取齐 entry
  const dropEntries: FileSystemEntry[] = []
  for (let i = 0; i < dt.items.length; i++) {
    const entry = dt.items[i]?.webkitGetAsEntry?.()
    if (entry) dropEntries.push(entry)
  }

  const topFiles: File[] = []
  const folders: OsDropFolder[] = []
  for (const entry of dropEntries) {
    if (entry.isFile) {
      const file = await new Promise<File | null>((resolve) =>
        (entry as FileSystemFileEntry).file(resolve, () => resolve(null)),
      )
      if (file) topFiles.push(file)
    } else if (entry.isDirectory) {
      const out: { file: File; relPath: string }[] = []
      await walkDropDirectory(entry as FileSystemDirectoryEntry, entry.name, out)
      if (out.length > 0) folders.push({ name: entry.name, entries: out })
    }
  }

  if (topFiles.length === 0 && folders.length === 0) return
  await localPaneRef.value?.importOsFiles(topFiles, folders)
}
</script>

<template>
  <Teleport to="body">
    <aside
      class="local-files-panel"
      :class="{ open: props.open, resizing: resizing, pinned: pinned }"
      :style="{ width: `${width}px` }"
      :aria-hidden="!props.open"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <div
        class="resize-handle"
        :title="t('common.dragToResize')"
        @pointerdown="onResizeStart"
      />
      <header class="panel-header">
        <div class="header-title">
          <NIcon :size="16"><FolderOpenOutline /></NIcon>
          <span class="drawer-title">{{ t("terminal.activityBar.localFiles") }}</span>
          <span v-if="tabTitle" class="drawer-subtitle" :title="tabTitle">{{ tabTitle }}</span>
        </div>
        <div class="header-actions">
          <NTooltip placement="bottom" :show-arrow="false">
            <template #trigger>
              <NButton
                size="small"
                quaternary
                circle
                :type="pinned ? 'primary' : 'default'"
                @click="pinned = !pinned"
              >
                <template #icon>
                  <NIcon>
                    <component :is="pinned ? PushpinFilled : PushpinOutlined" />
                  </NIcon>
                </template>
              </NButton>
            </template>
            {{ pinned ? t("terminal.localFiles.unpin") : t("terminal.localFiles.pin") }}
          </NTooltip>
          <NButton
            size="small"
            quaternary
            circle
            :title="t('common.close')"
            @click="onClose"
          >
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </div>
      </header>

      <div v-if="everOpened" class="panel-body">
        <LocalPane
          ref="localPaneRef"
          :dir="dir"
          sid=""
          standalone
          @update:dir="(d: string) => (dir = d)"
          @open-in-terminal="(p: string) => emit('open-in-terminal', p)"
          @close="onClose"
        />
      </div>

      <!-- OS 拖入高亮遮罩 -->
      <div v-if="dropHover" class="drop-overlay">
        <NIcon :size="28"><FolderOpenOutline /></NIcon>
        <p>{{ t("sftp.localPane.dropToImport") }}</p>
      </div>
    </aside>
  </Teleport>
</template>

<style scoped>
.local-files-panel {
  position: fixed;
  top: var(--ashell-header-h);
  right: var(--ashell-activity-w, 0px);
  bottom: 0;
  background: var(--ashell-panel-bg);
  border-left: 1px solid var(--ashell-border);
  display: flex;
  flex-direction: column;
  z-index: 1000;
  /* 关闭态平移叠加活动栏宽度：面板锚定 right: var(--ashell-activity-w)，
     只平移自身宽度会残留一条活动栏宽度的面板左缘盖住活动栏 */
  transform: translateX(calc(100% + var(--ashell-activity-w, 0px)));
  transition: transform 0.25s ease, box-shadow 0.15s ease;
  user-select: text;
}

.local-files-panel.open {
  transform: translateX(0);
  box-shadow: -8px 0 24px var(--ashell-shadow);
}

/* 固定（停靠）态：与主内容并排，不再悬浮 */
.local-files-panel.pinned.open {
  box-shadow: none;
}

/* 固定态显隐不滑入：滑入期间内容区让位变量已就位，
   会看到面板盖在已让位的空白上（与 HostsDrawer 同理） */
.local-files-panel.pinned {
  transition: none;
}

/* 拖拽调宽中：关掉开合动画避免宽度跟手抖动，禁选中文本 */
.local-files-panel.resizing {
  transition: none;
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
.local-files-panel.resizing .resize-handle {
  background: rgba(124, 92, 255, 0.45);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.drawer-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  white-space: nowrap;
}

.drawer-subtitle {
  font-size: 12px;
  color: var(--ashell-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.panel-body {
  flex: 1;
  min-height: 0;
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
}

.panel-body > :deep(.local-pane) {
  flex: 1;
  min-height: 0;
}

.drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: color-mix(in srgb, var(--ashell-primary) 12%, var(--ashell-panel-bg));
  border: 1px dashed var(--ashell-primary);
  color: var(--ashell-primary);
  font-size: 13px;
  pointer-events: none;
}

.drop-overlay p {
  margin: 0;
}
</style>
