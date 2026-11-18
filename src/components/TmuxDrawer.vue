<script setup lang="ts">
import {
  RiCloseLine,
  RiDeleteBinLine,
  RiRefreshLine,
} from "@remixicon/vue"
import { computed, h, onBeforeUnmount, ref, watch } from "vue"
import {
  NAlert,
  NButton,
  NDataTable,
  NEmpty,
  NIcon,
  NPopconfirm,
  NSpace,
  NTag,
  useMessage,
  type DataTableColumns,
} from "naive-ui"

import { useI18n } from "vue-i18n"
import { killTmuxSession, listTmuxSessions, ApiError } from "@/api"
import type { TmuxSessionInfo } from "@/types"
import { useDrawerWidth } from "@/composables/useDrawerWidth"
import { usePanelSwitchClosed } from "@/composables/usePanels"

const props = defineProps<{
  open: boolean
  /** 当前激活终端 tab 的 SSH session id */
  sid: string | null
  /** 关联主机 id（后端据其 bastion 标记拒绝堡垒机会话） */
  hostId: number | null
  hostName?: string
}>()

const emit = defineEmits<{
  "update:open": [value: boolean]
  /** 接入指定会话：由父组件向当前终端注入 switch-client/attach 命令 */
  attach: [name: string]
}>()

const { t } = useI18n()
const message = useMessage()

const sessions = ref<TmuxSessionInfo[]>([])
const loading = ref(false)
/** 加载失败原因（内联 NAlert 展示：远端未装 tmux 属常态而非异常） */
const loadError = ref("")
const killing = ref(false)

const drawerTitle = computed(() =>
  props.hostName
    ? t("tmux.titleWithHost", { host: props.hostName })
    : t("tmux.title"),
)

function formatTime(unixSecs: number): string {
  const d = new Date(unixSecs * 1000)
  if (Number.isNaN(d.getTime())) return String(unixSecs)
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

const columns = computed<DataTableColumns<TmuxSessionInfo>>(() => [
  {
    title: t("tmux.columns.name"),
    key: "name",
    ellipsis: { tooltip: true },
    render: (r) => r.name,
  },
  {
    title: t("tmux.columns.windows"),
    key: "windows",
    width: 70,
    render: (r) => String(r.windows),
  },
  {
    title: t("tmux.columns.attached"),
    key: "attached",
    width: 70,
    render: (r) =>
      h(
        NTag,
        { size: "small", bordered: false, type: r.attached > 0 ? "success" : "default" },
        { default: () => String(r.attached) },
      ),
  },
  {
    title: t("tmux.columns.created"),
    key: "created",
    width: 140,
    render: (r) => formatTime(r.created),
  },
  {
    title: t("tmux.columns.actions"),
    key: "actions",
    width: 92,
    render: (r) =>
      h(NSpace, { size: 4, wrap: false }, {
        default: () => [
          h(
            NButton,
            {
              size: "tiny",
              type: "primary",
              onClick: () => onAttach(r.name),
            },
            { default: () => t("tmux.attach") },
          ),
          h(
            NPopconfirm,
            { onPositiveClick: () => onKill(r) },
            {
              trigger: () =>
                h(
                  NButton,
                  { size: "tiny", quaternary: true, circle: true, type: "error" },
                  { icon: () => h(NIcon, null, { default: () => h(RiDeleteBinLine) }) },
                ),
              default: () => t("tmux.killConfirm", { name: r.name }),
            },
          ),
        ],
      }),
  },
])

function onAttach(name: string) {
  emit("attach", name)
  // 切换由终端侧异步完成，列表短暂延迟刷新一次让 attached 数跟上
  window.setTimeout(() => void refresh(), 1200)
}

async function refresh() {
  if (!props.sid || props.hostId == null) {
    sessions.value = []
    loadError.value = ""
    return
  }
  loading.value = true
  loadError.value = ""
  try {
    sessions.value = await listTmuxSessions(props.sid, props.hostId)
  } catch (e) {
    sessions.value = []
    loadError.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function onKill(r: TmuxSessionInfo) {
  if (!props.sid || props.hostId == null || killing.value) return
  killing.value = true
  try {
    await killTmuxSession(props.sid, r.name, props.hostId)
    message.success(t("tmux.message.killed", { name: r.name }))
    await refresh()
  } catch (e) {
    const msg = e instanceof ApiError ? e.message : String(e)
    message.error(t("tmux.message.killFailed", { error: msg }))
  } finally {
    killing.value = false
  }
}

function onClose() {
  emit("update:open", false)
}

/* ---------- 打开 / 切换会话时刷新（不轮询：列表打开时手动刷新） ---------- */
watch(
  () => [props.open, props.sid] as const,
  ([open, sid]) => {
    if (open && sid) void refresh()
  },
  { immediate: true },
)

/* ---------- 拖拽改变面板宽度 ---------- */
const { width, resizing, onResizeStart, onResizeEnd } = useDrawerWidth({
  storageKey: "ashell:tmux-width",
  minWidth: 420,
  defaultWidth: 520,
})

onBeforeUnmount(() => {
  onResizeEnd()
})

// 被互斥切换关闭：跳过离场滑动动画，避免旧面板滑出与新面板滑入交叠出重影
const switchClosed = usePanelSwitchClosed("tmux")

const panelStyle = computed(() => ({
  width: `${width.value}px`,
  transition:
    resizing.value || switchClosed.value
      ? "none"
      : "transform 0.25s ease, box-shadow 0.15s ease",
  // 关闭态平移叠加活动栏宽度，理由同 AiAssistant（锚定 right: 活动栏宽）
  transform: props.open
    ? "translateX(0)"
    : "translateX(calc(100% + var(--ashell-activity-w, 0px)))",
}))
</script>

<template>
  <Teleport to="body">
    <aside
      class="tmux-panel"
      :class="{ open: props.open, resizing: resizing }"
      :style="panelStyle"
      :aria-hidden="!props.open"
    >
      <div
        class="resize-handle"
        :title="t('tmux.dragToResize')"
        @pointerdown="onResizeStart"
      />

      <header class="panel-header">
        <span class="drawer-title">{{ drawerTitle }}</span>
        <NSpace :size="6" align="center" :wrap="false">
          <NButton
            size="small"
            quaternary
            circle
            :title="t('tmux.refresh')"
            :loading="loading"
            @click="refresh"
          >
            <template #icon>
              <NIcon><RiRefreshLine /></NIcon>
            </template>
          </NButton>
          <NButton size="small" quaternary circle :title="t('tmux.close')" @click="onClose">
            <template #icon>
              <NIcon><RiCloseLine /></NIcon>
            </template>
          </NButton>
        </NSpace>
      </header>

      <div class="panel-body">
        <div v-if="!props.sid" class="empty-wrap">
          <NEmpty :description="t('tmux.needSession')" />
        </div>
        <template v-else>
          <NAlert
            v-if="loadError"
            type="warning"
            :title="t('tmux.message.loadFailed')"
            class="load-error"
          >
            {{ loadError }}
          </NAlert>
          <div v-else-if="sessions.length === 0 && !loading" class="empty-wrap">
            <NEmpty :description="t('tmux.empty')" />
          </div>
          <NDataTable
            v-else
            size="small"
            :columns="columns"
            :data="sessions"
            :bordered="false"
            :single-line="false"
            :row-key="(r: TmuxSessionInfo) => r.name"
            :loading="loading"
            flex-height
            style="height: 100%"
          />
        </template>
      </div>
    </aside>
  </Teleport>
</template>

<style scoped>
.tmux-panel {
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

.tmux-panel.open {
  box-shadow: -8px 0 24px var(--ashell-shadow);
  /* 浮层抽屉永远盖在固定（停靠）面板之上（固定面板豁免互斥会并存） */
  z-index: 1010;
}

.tmux-panel.resizing {
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
.tmux-panel.resizing .resize-handle {
  background: rgba(109, 90, 232, 0.18);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  /* 与各抽屉统一的紧凑头部规格（同 SftpDrawer） */
  padding: 6px 12px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.drawer-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-right: 12px;
}

.panel-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 10px 12px;
}

.empty-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}

.load-error {
  flex-shrink: 0;
  margin-bottom: 8px;
  max-height: 40%;
  overflow: auto;
}
</style>
