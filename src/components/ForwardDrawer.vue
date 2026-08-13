<script setup lang="ts">
import { computed, h, onBeforeUnmount, ref, watch } from "vue"
import {
  NButton,
  NCard,
  NDataTable,
  NDynamicInput,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NInputNumber,
  NModal,
  NPopconfirm,
  NSelect,
  NSpace,
  NTag,
  NTooltip,
  useMessage,
  type DataTableColumns,
} from "naive-ui"
import {
  AddOutline,
  CloseOutline,
  RefreshOutline,
  TrashOutline,
} from "@vicons/ionicons5"
import { useI18n } from "vue-i18n"
import {
  createForward,
  deleteForward,
  listForwards,
  ApiError,
} from "@/api"
import type { ForwardKind, ForwardRule } from "@/types"
import { humanSize } from "@/utils/humanSize"

// 抑制未用的导入警告
void NDynamicInput

const props = defineProps<{
  open: boolean
  /** 当前激活终端 tab 的 SSH session id */
  sid: string | null
  hostName?: string
}>()

const emit = defineEmits<{
  "update:open": [value: boolean]
}>()

const { t } = useI18n()
const message = useMessage()

const rules = ref<ForwardRule[]>([])
const loading = ref(false)
const formOpen = ref(false)
const submitting = ref(false)

interface DraftForm {
  kind: ForwardKind
  bindAddr: string
  bindPort: number
  destHost: string
  destPort: number
}

const draft = ref<DraftForm>(emptyDraft())

function emptyDraft(): DraftForm {
  return {
    kind: "local",
    bindAddr: "127.0.0.1",
    bindPort: 8080,
    destHost: "",
    destPort: 80,
  }
}

const kindOptions = computed<Array<{ label: string; value: ForwardKind }>>(() => [
  { label: t("forward.kind.local"), value: "local" },
  { label: t("forward.kind.remote"), value: "remote" },
  { label: t("forward.kind.dynamic"), value: "dynamic" },
])

const drawerTitle = computed(() => {
  const host = props.hostName ?? ""
  return host ? t("forward.titleWithHost", { host }) : t("forward.title")
})

const showDest = computed(() => draft.value.kind !== "dynamic")

function kindLabel(k: ForwardKind): string {
  return k === "local" ? "-L" : k === "remote" ? "-R" : "-D"
}

function kindTagType(k: ForwardKind): "success" | "info" | "warning" {
  return k === "local" ? "success" : k === "remote" ? "warning" : "info"
}

const columns = computed<DataTableColumns<ForwardRule>>(() => [
  {
    title: t("forward.columns.kind"),
    key: "kind",
    width: 70,
    render: (r) =>
      h(
        NTag,
        { size: "small", type: kindTagType(r.kind), bordered: false },
        { default: () => kindLabel(r.kind) },
      ),
  },
  {
    title: t("forward.columns.bind"),
    key: "bind",
    render: (r) => `${r.bindAddr}:${r.bindPort}`,
  },
  {
    title: t("forward.columns.target"),
    key: "dest",
    render: (r) =>
      r.kind === "dynamic"
        ? t("forward.kind.dynamicLabel")
        : `${r.destHost ?? "?"}:${r.destPort ?? "?"}`,
  },
  {
    title: t("forward.columns.status"),
    key: "status",
    width: 90,
    render: (r) => {
      const hasErr = !!r.err
      const tagType: "success" | "error" | "warning" =
        r.status === "running"
          ? hasErr
            ? "warning"
            : "success"
          : "error"
      const tag = h(
        NTag,
        { size: "small", type: tagType, bordered: false },
        { default: () => (hasErr ? `${r.status}*` : r.status) },
      )
      if (!hasErr) return tag
      return h(
        NTooltip,
        { placement: "top", style: { maxWidth: "320px" } },
        {
          trigger: () => tag,
          default: () => t("forward.message.lastError", { error: r.err ?? "" }),
        },
      )
    },
  },
  {
    title: t("forward.columns.traffic"),
    key: "bytes",
    width: 130,
    render: (r) => `↓${humanSize(r.rxBytes)} ↑${humanSize(r.txBytes)}`,
  },
  {
    title: t("forward.columns.actions"),
    key: "actions",
    width: 70,
    render: (r) =>
      h(
        NPopconfirm,
        { onPositiveClick: () => onDelete(r) },
        {
          trigger: () =>
            h(
              NButton,
              { size: "tiny", quaternary: true, circle: true, type: "error" },
              { icon: () => h(NIcon, null, { default: () => h(TrashOutline) }) },
            ),
          default: () => t("forward.deleteConfirm"),
        },
      ),
  },
])

async function refresh() {
  if (!props.sid) {
    rules.value = []
    return
  }
  loading.value = true
  try {
    rules.value = await listForwards(props.sid)
  } catch (e) {
    if (e instanceof ApiError) {
      message.error(t("forward.message.loadFailed", { error: e.message }))
    } else {
      console.error("[forward] list:", e)
    }
  } finally {
    loading.value = false
  }
}

function openForm() {
  draft.value = emptyDraft()
  formOpen.value = true
}

async function submitForm() {
  const sid = props.sid
  if (!sid) return
  if (submitting.value) return
  const d = draft.value
  if (d.bindPort < 0 || d.bindPort > 65535) {
    message.warning(t("forward.message.bindPortRange"))
    return
  }
  if (d.kind !== "dynamic") {
    if (!d.destHost.trim()) {
      message.warning(t("forward.message.targetHostRequired"))
      return
    }
    if (d.destPort < 1 || d.destPort > 65535) {
      message.warning(t("forward.message.targetPortRange"))
      return
    }
  }
  submitting.value = true
  try {
    await createForward({
      sid,
      kind: d.kind,
      bindAddr: d.bindAddr.trim() || "127.0.0.1",
      bindPort: d.bindPort,
      destHost: d.kind === "dynamic" ? null : d.destHost.trim(),
      destPort: d.kind === "dynamic" ? null : d.destPort,
    })
    formOpen.value = false
    message.success(t("forward.message.started"))
    await refresh()
  } catch (e) {
    const msg = e instanceof ApiError ? e.message : String(e)
    message.error(t("forward.message.createFailed", { error: msg }))
  } finally {
    submitting.value = false
  }
}

async function onDelete(r: ForwardRule) {
  if (!props.sid) return
  try {
    await deleteForward(props.sid, r.id)
    message.success(t("forward.message.deleted"))
    await refresh()
  } catch (e) {
    const msg = e instanceof ApiError ? e.message : String(e)
    message.error(t("forward.message.deleteFailed", { error: msg }))
  }
}

/* ---------- 5s 轮询刷新流量 / 状态 ---------- */
let pollTimer: number | null = null

function startPoll() {
  stopPoll()
  pollTimer = window.setInterval(() => {
    if (props.open && props.sid) void refresh()
  }, 5000)
}

function stopPoll() {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer)
    pollTimer = null
  }
}

watch(
  () => [props.open, props.sid] as const,
  ([open, sid]) => {
    if (open && sid) {
      void refresh()
      startPoll()
    } else {
      stopPoll()
    }
  },
  { immediate: true },
)

/* ---------- 拖拽改变面板宽度 ---------- */
const MIN_WIDTH = 460
const DEFAULT_WIDTH = 720
// 拖动上限取视口宽度的 90%，避免抽屉完全盖住主界面
function getMaxWidth(): number {
  return Math.round(window.innerWidth * 0.9)
}
const WIDTH_KEY = "ashell:forward-width"

const width = ref<number>(loadWidth())
const resizing = ref(false)

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

function onResizeStart(e: PointerEvent) {
  e.preventDefault()
  resizing.value = true
  window.addEventListener("pointermove", onResizeMove)
  window.addEventListener("pointerup", onResizeEnd)
  window.addEventListener("pointercancel", onResizeEnd)
}

function onResizeMove(e: PointerEvent) {
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

onBeforeUnmount(() => {
  onResizeEnd()
  stopPoll()
})

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
      class="forward-panel"
      :class="{ open: props.open, resizing: resizing }"
      :style="panelStyle"
      :aria-hidden="!props.open"
    >
      <div
        class="resize-handle"
        :title="t('forward.dragToResize')"
        @pointerdown="onResizeStart"
      />

      <header class="panel-header">
        <span class="drawer-title">{{ drawerTitle }}</span>
        <NSpace :size="6" align="center" :wrap="false">
          <NButton size="small" type="primary" @click="openForm">
            <template #icon>
              <NIcon><AddOutline /></NIcon>
            </template>
            {{ t("forward.newButton") }}
          </NButton>
          <NButton
            size="small"
            quaternary
            circle
            :title="t('forward.refresh')"
            :loading="loading"
            @click="refresh"
          >
            <template #icon>
              <NIcon><RefreshOutline /></NIcon>
            </template>
          </NButton>
          <NButton size="small" quaternary circle :title="t('forward.close')" @click="onClose">
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </NSpace>
      </header>

      <div class="panel-body">
        <NDataTable
          size="small"
          :columns="columns"
          :data="rules"
          :bordered="false"
          :single-line="false"
          :row-key="(r: ForwardRule) => r.id"
          :loading="loading"
          flex-height
          style="height: 100%"
        />
      </div>
    </aside>

    <NModal v-model:show="formOpen">
      <NCard
        style="width: min(480px, 90vw)"
        :title="t('forward.newRule')"
        size="small"
        :bordered="false"
        role="dialog"
        aria-modal="true"
      >
        <NForm label-placement="top" :model="draft">
          <NFormItem :label="t('forward.form.kind')">
            <NSelect v-model:value="draft.kind" :options="kindOptions" />
          </NFormItem>
          <NFormItem
            :label="
              draft.kind === 'remote'
                ? t('forward.form.remoteBindAddr')
                : t('forward.form.localBindAddr')
            "
          >
            <NInput
              v-model:value="draft.bindAddr"
              :placeholder="
                draft.kind === 'remote' ? t('forward.form.bindAddrPlaceholder') : t('forward.form.bindAddrPlaceholderLocal')
              "
            />
          </NFormItem>
          <NFormItem
            :label="
              draft.kind === 'remote' ? t('forward.form.remoteBindPort') : t('forward.form.localBindPort')
            "
          >
            <NInputNumber
              v-model:value="draft.bindPort"
              :min="0"
              :max="65535"
              style="width: 100%"
            />
          </NFormItem>
          <template v-if="showDest">
            <NFormItem
              :label="
                draft.kind === 'local'
                  ? t('forward.form.remoteTargetHost')
                  : t('forward.form.localTargetHost')
              "
            >
              <NInput v-model:value="draft.destHost" :placeholder="t('forward.form.targetHostPlaceholder')" />
            </NFormItem>
            <NFormItem :label="t('forward.form.targetPort')">
              <NInputNumber
                v-model:value="draft.destPort"
                :min="1"
                :max="65535"
                style="width: 100%"
              />
            </NFormItem>
          </template>
          <div v-if="draft.kind === 'dynamic'" class="hint">
            {{ t("forward.form.dynamicHint") }}
          </div>
          <div v-else-if="draft.kind === 'remote'" class="hint">
            {{ t("forward.form.remoteHint") }}
          </div>
        </NForm>
        <template #footer>
          <NSpace justify="end">
            <NButton :disabled="submitting" @click="formOpen = false">
              {{ t("forward.cancel") }}
            </NButton>
            <NButton type="primary" :loading="submitting" @click="submitForm">
              {{ t("forward.start") }}
            </NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>
  </Teleport>
</template>

<style scoped>
.forward-panel {
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

.forward-panel.open {
  box-shadow: -8px 0 24px var(--ashell-shadow);
}

.forward-panel.resizing {
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
.forward-panel.resizing .resize-handle {
  background: rgba(124, 92, 255, 0.18);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.drawer-title {
  font-size: 14px;
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
  padding: 10px 12px;
}

.hint {
  margin-top: 4px;
  font-size: 12px;
  color: var(--ashell-text-subtle);
  line-height: 1.5;
}
</style>
