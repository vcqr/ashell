<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import {
  NButton,
  NDropdown,
  NIcon,
  NInput,
  useDialog,
  useMessage,
  useThemeVars,
  type DropdownOption,
} from 'naive-ui'
import {
  AddOutline,
  CloseOutline,
  CopyOutline,
  CreateOutline,
  DownloadOutline,
  DuplicateOutline,
  MegaphoneOutline,
  OpenOutline,
  PowerOutline,
  RefreshOutline,
  ServerOutline,
  TerminalOutline,
  TrashOutline,
} from '@vicons/ionicons5'
import { useIconStore } from '@/stores/icons'
import { useBroadcastStore } from '@/stores/broadcast'
import type { TerminalTab } from '@/types'

const props = defineProps<{
  tabs: TerminalTab[]
  activeKey: string
  /**
   * App.vue 注入的"取出指定 tab 的会话快照"回调。
   * tab 的 xterm 实例在 App.vue 持有，TabBar 自己拿不到，因此通过 prop 注入。
   * 返回 SerializeAddon.serialize() 的结果（带 ANSI 颜色），失败/不存在时返回 null。
   */
  getSessionContent?: (key: string) => string | null
}>()

const emit = defineEmits<{
  'update:active-key': [key: string]
  close: [key: string]
  new: [kind: 'host' | 'local']
  reorder: [tabs: TerminalTab[]]
  reconnect: [key: string]
  disconnect: [key: string]
  duplicate: [key: string]
  rename: [key: string, title: string]
  'open-in-new-window': [key: string]
  'close-others': [key: string]
  'close-left': [key: string]
  'close-right': [key: string]
}>()

const vars = useThemeVars()
const message = useMessage()
const dialog = useDialog()

const iconStore = useIconStore()
const broadcastStore = useBroadcastStore()
const { t } = useI18n()

/** 当前生效的广播源全局 key（依赖 props.activeKey 实时计算）。 */
const broadcastSourceKey = computed(() =>
  broadcastStore.effectiveSource(
    props.activeKey ?? null,
    broadcastStore.windowId || null,
  ),
)
/** 某个本地 tab 是否是广播源。 */
function isBroadcastSource(tabKey: string): boolean {
  return broadcastSourceKey.value === broadcastStore.globalKey(tabKey)
}
/** 某个本地 tab 是否是广播目标。 */
function isBroadcastTarget(tabKey: string): boolean {
  return broadcastStore.targetKeys.has(broadcastStore.globalKey(tabKey))
}
onMounted(() => {
  void iconStore.ensureLoaded()
})

const scrollEl = ref<HTMLDivElement | null>(null)

function onWheel(e: WheelEvent) {
  if (!scrollEl.value) return
  if (e.deltaY !== 0 && e.deltaX === 0) {
    scrollEl.value.scrollLeft += e.deltaY
    e.preventDefault()
  }
}

/* ---- pointer-based drag (HTML5 dnd is unreliable in Tauri/wry) ---- */
const DRAG_THRESHOLD = 5
const dragIndex = ref<number | null>(null)
// Insertion slot: a number in [0, tabs.length]; visualised as a vertical bar
// before the tab at this index (or after the last one if === tabs.length).
const insertIndex = ref<number | null>(null)

// Floating preview (a snapshot of the dragged tab that follows the cursor).
const preview = ref<{
  title: string
  active: boolean
  width: number
  height: number
  x: number
  y: number
  offsetX: number
  offsetY: number
  color?: string | null
} | null>(null)

let pendingIdx: number | null = null
let pendingX = 0
let pendingY = 0
let pendingPointerId: number | null = null
let pendingItemRect: DOMRect | null = null
let dragStarted = false
let suppressClick = false

function computeInsertIndex(clientX: number, clientY: number): number | null {
  const strip = scrollEl.value
  if (!strip) return null
  const items = strip.querySelectorAll<HTMLElement>('.tab-item')
  if (items.length === 0) return 0
  const stripRect = strip.getBoundingClientRect()
  // Outside the strip vertically -> no slot.
  if (clientY < stripRect.top - 24 || clientY > stripRect.bottom + 24) {
    return null
  }
  for (let i = 0; i < items.length; i++) {
    const r = items[i]!.getBoundingClientRect()
    const mid = r.left + r.width / 2
    if (clientX < mid) return i
  }
  return items.length
}

function onItemPointerDown(e: PointerEvent, idx: number) {
  if (e.button !== 0) return
  pendingIdx = idx
  pendingX = e.clientX
  pendingY = e.clientY
  pendingPointerId = e.pointerId
  pendingItemRect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  dragStarted = false
  suppressClick = false
  window.addEventListener('pointermove', onWindowPointerMove)
  window.addEventListener('pointerup', onWindowPointerUp)
  window.addEventListener('pointercancel', onWindowPointerUp)
}

function onWindowPointerMove(e: PointerEvent) {
  if (pendingIdx === null) return
  if (pendingPointerId !== null && e.pointerId !== pendingPointerId) return
  if (!dragStarted) {
    const dx = e.clientX - pendingX
    const dy = e.clientY - pendingY
    if (dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD) return
    dragStarted = true
    suppressClick = true
    dragIndex.value = pendingIdx
    document.body.classList.add('ashell-tab-dragging')
    if (pendingItemRect) {
      const tab = props.tabs[pendingIdx]
      preview.value = {
        title: tab?.title ?? '',
        active: tab ? tab.key === props.activeKey : false,
        width: pendingItemRect.width,
        height: pendingItemRect.height,
        x: e.clientX,
        y: e.clientY,
        offsetX: pendingX - pendingItemRect.left,
        offsetY: pendingY - pendingItemRect.top,
        color: tab?.color ?? null,
      }
    }
  }
  if (preview.value) {
    preview.value.x = e.clientX
    preview.value.y = e.clientY
  }
  insertIndex.value = computeInsertIndex(e.clientX, e.clientY)
}

function onWindowPointerUp(e: PointerEvent) {
  window.removeEventListener('pointermove', onWindowPointerMove)
  window.removeEventListener('pointerup', onWindowPointerUp)
  window.removeEventListener('pointercancel', onWindowPointerUp)
  document.body.classList.remove('ashell-tab-dragging')
  if (dragStarted && dragIndex.value !== null) {
    const slot = computeInsertIndex(e.clientX, e.clientY)
    const from = dragIndex.value
    if (slot !== null) {
      // Slot N means "insert before tab N"; if from < slot we drop one because removing
      // the source first shifts indices left.
      let to = slot
      if (from < slot) to -= 1
      if (to !== from && to >= 0 && to < props.tabs.length) {
        const copy = [...props.tabs]
        const [moved] = copy.splice(from, 1)
        copy.splice(to, 0, moved!)
        emit('reorder', copy)
      }
    }
  }
  pendingIdx = null
  pendingPointerId = null
  pendingItemRect = null
  dragStarted = false
  dragIndex.value = null
  insertIndex.value = null
  preview.value = null
}

function onItemClick(e: MouseEvent, key: string) {
  if (suppressClick) {
    suppressClick = false
    e.preventDefault()
    e.stopPropagation()
    return
  }
  emit('update:active-key', key)
}

/* ---- Right-click context menu ---- */
const ctxMenuShow = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuKey = ref<string | null>(null)

function onItemContextMenu(e: MouseEvent, key: string) {
  e.preventDefault()
  e.stopPropagation()
  ctxMenuKey.value = key
  ctxMenuX.value = e.clientX
  ctxMenuY.value = e.clientY
  ctxMenuShow.value = false
  requestAnimationFrame(() => {
    ctxMenuShow.value = true
  })
}

function renderMenuIcon(comp: unknown) {
  return () => h(NIcon, null, { default: () => h(comp as never) })
}

const newTabOptions = computed<DropdownOption[]>(() => [
  {
    label: t('terminal.tabBar.openHost'),
    key: 'host',
    icon: renderMenuIcon(ServerOutline),
  },
  {
    label: t('terminal.tabBar.newLocal'),
    key: 'local',
    icon: renderMenuIcon(TerminalOutline),
  },
])

function onNewSelect(action: string) {
  if (action === 'host' || action === 'local') {
    emit('new', action)
  }
}

const ctxMenuOptions = computed<DropdownOption[]>(() => {
  const key = ctxMenuKey.value
  if (!key) return []
  const idx = props.tabs.findIndex((t) => t.key === key)
  if (idx < 0) return []
  const tab = props.tabs[idx]!
  const isConnected = tab.status === 'connected' || tab.status === 'connecting'
  const total = props.tabs.length
  const hasLeft = idx > 0
  const hasRight = idx < total - 1
  const hasOthers = total > 1
  const canCopyAddr = !!tab.hostInfo?.addr

  return [
    {
      label: t('terminal.tabBar.reconnect'),
      key: 'reconnect',
      icon: renderMenuIcon(RefreshOutline),
    },
    {
      label: t('terminal.tabBar.disconnect'),
      key: 'disconnect',
      icon: renderMenuIcon(PowerOutline),
      disabled: !isConnected,
    },
    { type: 'divider', key: 'd1' },
    {
      label: t('terminal.tabBar.duplicate'),
      key: 'duplicate',
      icon: renderMenuIcon(DuplicateOutline),
      disabled: tab.hostId === undefined || tab.hostId === null,
    },
    {
      label: t('terminal.tabBar.openInNewWindow'),
      key: 'open-in-new-window',
      icon: renderMenuIcon(OpenOutline),
    },
    {
      label: t('terminal.tabBar.copySshAddr'),
      key: 'copy-addr',
      icon: renderMenuIcon(CopyOutline),
      disabled: !canCopyAddr,
    },
    {
      label: t('terminal.tabBar.rename'),
      key: 'rename',
      icon: renderMenuIcon(CreateOutline),
    },
    {
      label: t('terminal.tabBar.exportSession'),
      key: 'export-session',
      icon: renderMenuIcon(DownloadOutline),
    },
    { type: 'divider', key: 'd2' },
    {
      label: t('terminal.tabBar.closeCurrent'),
      key: 'close',
      icon: renderMenuIcon(CloseOutline),
    },
    {
      label: t('terminal.tabBar.closeRight'),
      key: 'close-right',
      icon: renderMenuIcon(TrashOutline),
      disabled: !hasRight,
    },
    {
      label: t('terminal.tabBar.closeLeft'),
      key: 'close-left',
      icon: renderMenuIcon(TrashOutline),
      disabled: !hasLeft,
    },
    {
      label: t('terminal.tabBar.closeOthers'),
      key: 'close-others',
      icon: renderMenuIcon(TrashOutline),
      disabled: !hasOthers,
    },
  ]
})

async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    // fallback
  }
  try {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.left = '-9999px'
    document.body.appendChild(ta)
    ta.select()
    const ok = document.execCommand('copy')
    document.body.removeChild(ta)
    return ok
  } catch {
    return false
  }
}

function onCtxSelect(action: string) {
  ctxMenuShow.value = false
  const key = ctxMenuKey.value
  if (!key) return
  const tab = props.tabs.find((t) => t.key === key)
  if (!tab) return
  switch (action) {
    case 'reconnect':
      emit('reconnect', key)
      break
    case 'disconnect':
      emit('disconnect', key)
      break
    case 'duplicate':
      emit('duplicate', key)
      break
    case 'open-in-new-window':
      emit('open-in-new-window', key)
      break
    case 'copy-addr': {
      const info = tab.hostInfo
      if (!info?.addr) return
      const user = info.username ? `${info.username}@` : ''
      const port = info.port && info.port !== '22' ? ` -p ${info.port}` : ''
      const text = `ssh ${user}${info.addr}${port}`.trim()
      void copyToClipboard(text).then((ok) => {
        if (ok) message.success(t('terminal.tabBar.copiedToClipboard'))
        else message.error(t('terminal.tabBar.copyFailed'))
      })
      break
    }
    case 'rename':
      startRename(key, tab.title)
      break
    case 'export-session':
      void exportSession(key, tab.title)
      break
    case 'close':
      confirmCloseSingle(key)
      break
    case 'close-others':
      confirmCloseBulk('close-others', key)
      break
    case 'close-left':
      confirmCloseBulk('close-left', key)
      break
    case 'close-right':
      confirmCloseBulk('close-right', key)
      break
  }
}

/* ---- Close confirmation ---- */
function confirmCloseSingle(key: string) {
  const tab = props.tabs.find((t) => t.key === key)
  // 仅在会话进行中（已连接或正在握手）时弹确认；closed/error 直接关。
  // 是否激活不影响判断——后台 tab 同样可能在跑长任务。
  const live = tab?.status === 'connected' || tab?.status === 'connecting'
  if (!live) {
    emit('close', key)
    return
  }
  const name = tab?.title ?? ''
  dialog.warning({
    title: t('terminal.tabBar.closeConfirmTitle'),
    content: t('terminal.tabBar.closeCurrentConfirm', { name }),
    positiveText: t('common.close'),
    negativeText: t('common.cancel'),
    onPositiveClick: () => emit('close', key),
  })
}

function confirmCloseBulk(
  action: 'close-others' | 'close-left' | 'close-right',
  key: string,
) {
  const idx = props.tabs.findIndex((t) => t.key === key)
  if (idx < 0) return
  let count: number
  let content: string
  if (action === 'close-others') {
    count = props.tabs.length - 1
    content = t('terminal.tabBar.closeOthersConfirm', { count })
  } else if (action === 'close-left') {
    count = idx
    content = t('terminal.tabBar.closeLeftConfirm', { count })
  } else {
    count = props.tabs.length - idx - 1
    content = t('terminal.tabBar.closeRightConfirm', { count })
  }
  if (count <= 0) return
  const onPositive = () => {
    if (action === 'close-others') emit('close-others', key)
    else if (action === 'close-left') emit('close-left', key)
    else emit('close-right', key)
  }
  dialog.warning({
    title: t('terminal.tabBar.closeConfirmTitle'),
    content,
    positiveText: t('common.close'),
    negativeText: t('common.cancel'),
    onPositiveClick: onPositive,
  })
}

/* ---- Inline rename ---- */
const renamingKey = ref<string | null>(null)
const renameDraft = ref('')

async function exportSession(key: string, title: string) {
  if (!props.getSessionContent) {
    message.error(t('terminal.tabBar.cannotGetSession'))
    return
  }
  const content = props.getSessionContent(key)
  if (!content) {
    message.warning(t('terminal.tabBar.sessionEmpty'))
    return
  }
  // 默认文件名：tab 标题 + 时间戳。文件系统不安全字符替换为 _。
  const safeTitle = (title || 'session').replace(/[\\/:*?"<>|]/g, '_').slice(0, 80)
  const ts = new Date().toISOString().replace(/[-:T]/g, '').replace(/\..+$/, '')
  const defaultFilename = `${safeTitle}-${ts}.txt`
  try {
    const saved = await invoke<string | null>('save_text_file', {
      defaultFilename,
      content,
    })
    if (saved) {
      message.success(t('terminal.tabBar.savedTo', { path: saved }))
    }
    // saved === null 表示用户取消，不提示
  } catch (e) {
    message.error(t('terminal.tabBar.exportFailed', { error: String(e) }))
  }
}

function startRename(key: string, current: string) {
  renamingKey.value = key
  renameDraft.value = current
  void nextTick(() => {
    const el = document.querySelector<HTMLInputElement>(
      '.tab-item.is-renaming input',
    )
    el?.focus()
    el?.select()
  })
}

function commitRename() {
  const key = renamingKey.value
  if (!key) return
  const next = renameDraft.value.trim()
  if (next) {
    const tab = props.tabs.find((t) => t.key === key)
    if (tab && tab.title !== next) {
      emit('rename', key, next)
    }
  }
  renamingKey.value = null
}

function cancelRename() {
  renamingKey.value = null
}

function onRenameKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    commitRename()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    cancelRename()
  }
}

onBeforeUnmount(() => {
  window.removeEventListener('pointermove', onWindowPointerMove)
  window.removeEventListener('pointerup', onWindowPointerUp)
  window.removeEventListener('pointercancel', onWindowPointerUp)
  document.body.classList.remove('ashell-tab-dragging')
})
</script>

<template>
  <div
    class="tab-bar"
    data-tauri-drag-region="false"
    :style="{ '--hover-bg': vars?.hoverColor ?? 'var(--ashell-hover)' }"
  >
    <div ref="scrollEl" class="tab-strip" @wheel="onWheel">
      <template v-for="(tab, idx) in tabs" :key="tab.key">
        <div
          class="insert-marker"
          :class="{ visible: dragIndex !== null && insertIndex === idx }"
          aria-hidden="true"
        />
        <div
          class="tab-item"
          :class="{
            active: tab.key === activeKey,
            'is-dragging': dragIndex === idx,
            'is-renaming': renamingKey === tab.key,
            'bc-source':
              broadcastStore.enabled &&
              broadcastStore.targetKeys.size > 0 &&
              isBroadcastSource(tab.key),
            'bc-target':
              broadcastStore.enabled && isBroadcastTarget(tab.key),
          }"
          :style="
            tab.color
              ? ({
                  '--tab-color': tab.color,
                  '--tab-color-bg': `color-mix(in srgb, ${tab.color} 18%, transparent)`,
                } as Record<string, string>)
              : undefined
          "
          @pointerdown="onItemPointerDown($event, idx)"
          @click="onItemClick($event, tab.key)"
          @contextmenu="onItemContextMenu($event, tab.key)"
          @dblclick.stop="startRename(tab.key, tab.title)"
        >
          <img
            v-if="tab.icon && iconStore.urlOf(tab.icon)"
            :src="iconStore.urlOf(tab.icon) ?? ''"
            class="tab-icon"
            alt=""
          />
          <NInput
            v-if="renamingKey === tab.key"
            v-model:value="renameDraft"
            size="tiny"
            class="tab-rename-input"
            @pointerdown.stop
            @click.stop
            @blur="commitRename"
            @keydown="onRenameKeydown"
          />
          <span v-else class="tab-label">{{ tab.title }}</span>
          <NIcon
            v-if="
              broadcastStore.enabled &&
              broadcastStore.targetKeys.size > 0 &&
              (isBroadcastSource(tab.key) || isBroadcastTarget(tab.key))
            "
            :size="12"
            class="tab-broadcast-icon"
            :class="{ source: isBroadcastSource(tab.key) }"
          >
            <MegaphoneOutline />
          </NIcon>
          <NButton
            text
            size="tiny"
            class="tab-close"
            @pointerdown.stop
            @click.stop="confirmCloseSingle(tab.key)"
          >
            <template #icon>
              <NIcon :size="14"><CloseOutline /></NIcon>
            </template>
          </NButton>
        </div>
      </template>
      <div
        class="insert-marker"
        :class="{
          visible: dragIndex !== null && insertIndex === tabs.length,
        }"
        aria-hidden="true"
      />
    </div>

    <NDropdown
      placement="bottom-end"
      trigger="click"
      :options="newTabOptions"
      @select="onNewSelect"
    >
      <NButton text size="small" class="add-tab-btn">
        <template #icon>
          <NIcon :size="18"><AddOutline /></NIcon>
        </template>
      </NButton>
    </NDropdown>

    <NDropdown
      placement="bottom-start"
      trigger="manual"
      :x="ctxMenuX"
      :y="ctxMenuY"
      :options="ctxMenuOptions"
      :show="ctxMenuShow"
      @select="onCtxSelect"
      @clickoutside="ctxMenuShow = false"
    />

    <Teleport to="body">
      <div
        v-if="preview"
        class="drag-preview"
        :class="{ active: preview.active }"
        :style="{
          width: preview.width + 'px',
          height: preview.height + 'px',
          transform: `translate(${preview.x - preview.offsetX}px, ${preview.y - preview.offsetY}px)`,
          ...(preview.color
            ? {
                '--tab-color': preview.color,
                '--tab-color-bg': `color-mix(in srgb, ${preview.color} 18%, transparent)`,
              }
            : {}),
        }"
      >
        <span class="tab-label">{{ preview.title }}</span>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.tab-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 0;
  width: 100%;
  min-width: 0;
}

.tab-strip {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0;
  overflow-x: auto;
  overflow-y: hidden;
  flex-wrap: nowrap;
  scrollbar-width: none;
}

.tab-strip::-webkit-scrollbar {
  display: none;
}

.tab-item {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 12px;
  margin: 0 2px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  color: var(--ashell-text-muted);
  background: transparent;
  cursor: pointer;
  user-select: none;
  transition: background 0.15s ease, color 0.15s ease, opacity 0.15s ease,
    transform 0.15s ease;
  position: relative;
  flex: 0 0 auto;
  max-width: 200px;
  min-width: 90px;
  white-space: nowrap;
  touch-action: none;
}

.tab-item:hover {
  background: var(--hover-bg, var(--ashell-hover));
  color: var(--ashell-text);
}

.tab-item.active {
  color: var(--ashell-text-strong);
  background: var(--tab-color-bg, var(--ashell-active));
  box-shadow: inset 0 -2px 0 var(--tab-color, var(--ashell-primary));
}

.tab-item.is-dragging {
  opacity: 0.35;
  filter: grayscale(0.4);
  transform: scale(0.97);
  background: var(--ashell-hover);
}

.tab-item.is-dragging .tab-close {
  opacity: 0;
}

/* Slot between two tabs. Always present (so a thin separator can render);
 * becomes a glowing insertion bar while dragging.
 * Wrapper handles spacing/hit-testing; ::after draws the visible line. */
.insert-marker {
  flex: 0 0 auto;
  width: 6px;
  height: 26px;
  position: relative;
  pointer-events: none;
}

.insert-marker::after {
  content: '';
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%) scaleY(0.62);
  width: 1px;
  height: 16px;
  border-radius: 2px;
  background: var(--ashell-border, rgba(127, 127, 127, 0.28));
  box-shadow: none;
  transition: width 0.12s ease, height 0.12s ease, background 0.12s ease,
    box-shadow 0.12s ease, opacity 0.12s ease, transform 0.12s ease;
}

/* Hide separator at the strip ends and next to hover/active/dragging tabs. */
.insert-marker:first-child::after,
.insert-marker:last-child::after,
.tab-item:hover + .insert-marker::after,
.tab-item.active + .insert-marker::after,
.tab-item.is-dragging + .insert-marker::after,
.insert-marker:has(+ .tab-item:hover)::after,
.insert-marker:has(+ .tab-item.active)::after,
.insert-marker:has(+ .tab-item.is-dragging)::after {
  opacity: 0;
}

/* Active insertion bar during drag. */
.insert-marker.visible::after {
  opacity: 1 !important;
  width: 3px;
  height: 26px;
  background: var(--ashell-primary);
  box-shadow: 0 0 8px var(--ashell-primary);
  transform: translate(-50%, -50%) scaleY(1);
}

.tab-label {
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 1;
  min-width: 0;
}

.tab-rename-input {
  flex: 1;
  min-width: 60px;
}

.tab-rename-input :deep(.n-input__input-el) {
  font-size: 13px;
  height: 22px;
}

.tab-icon {
  width: 16px;
  height: 16px;
  border-radius: 3px;
  object-fit: contain;
  flex-shrink: 0;
}

.tab-close {
  opacity: 0;
  transition: opacity 0.15s;
  flex-shrink: 0;
  color: var(--ashell-text-subtle);
}

.tab-item:hover .tab-close,
.tab-item.active .tab-close {
  opacity: 1;
}

.tab-close:hover {
  color: var(--ashell-text-strong) !important;
}

.add-tab-btn {
  color: var(--ashell-text-subtle);
  margin-left: 4px;
  flex-shrink: 0;
}

.add-tab-btn:hover {
  color: var(--ashell-text-strong);
}

/* ===== 广播状态指示 ===== */
.tab-item.bc-target {
  /* 目标 tab：橙色虚线轮廓提示"正在接收外部输入" */
  outline: 1px dashed #f59e0b;
  outline-offset: -2px;
}
.tab-item.bc-source {
  /* 源 tab：橙色实线，更明显，提醒用户当前键盘输入会被复制出去 */
  outline: 1px solid #f59e0b;
  outline-offset: -2px;
}
.tab-broadcast-icon {
  margin-left: 4px;
  color: #f59e0b;
  flex-shrink: 0;
  opacity: 0.85;
}
.tab-broadcast-icon.source {
  /* 源用稍亮的描边色，并加微弱呼吸感 */
  opacity: 1;
  animation: tab-bc-pulse 1.6s ease-in-out infinite;
}
@keyframes tab-bc-pulse {
  0%, 100% { opacity: 0.6; }
  50% { opacity: 1; }
}
</style>

<style>
/* Global: floating drag preview lives on <body> via Teleport. */
.drag-preview {
  position: fixed;
  top: 0;
  left: 0;
  display: flex;
  align-items: center;
  padding: 0 12px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto,
    sans-serif;
  background: var(--tab-color-bg, var(--ashell-active, rgba(124, 92, 255, 0.18)));
  color: var(--ashell-text-strong, #fff);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.32),
    0 0 0 1px var(--tab-color, var(--ashell-primary, #7c5cff));
  pointer-events: none;
  z-index: 9999;
  opacity: 0.95;
  transform-origin: top left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  will-change: transform;
}

.drag-preview.active {
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4),
    0 0 0 2px var(--tab-color, var(--ashell-primary, #7c5cff));
}

.drag-preview .tab-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

body.ashell-tab-dragging,
body.ashell-tab-dragging * {
  cursor: grabbing !important;
}
</style>
