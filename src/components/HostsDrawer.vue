<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"
import { NModal, NCard, useMessage } from "naive-ui"
import { useI18n } from "vue-i18n"
import HostTree from "@/components/HostTree.vue"
import HostForm from "@/components/HostForm.vue"
import { useHostStore } from "@/stores/hosts"
import { useApiStore } from "@/stores/api"
import type { Host, HostCreate, HostNode, HostUpdate } from "@/types"

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  "update:open": [value: boolean]
  "open-host": [node: HostNode, forceNew?: boolean]
}>()

const { t } = useI18n()
const store = useHostStore()
const apiStore = useApiStore()
const message = useMessage()

const formOpen = ref(false)
const formMode = ref<"create" | "edit">("create")
const formInitial = ref<Host | null>(null)
const formDefaultGid = ref<number>(0)
const formSubmitting = ref(false)

const initialized = ref(false)

async function ensureLoaded() {
  if (initialized.value) return
  if (!apiStore.ready) {
    await apiStore.init()
  }
  if (!apiStore.ready) {
    message.error(t("hosts.message.backendNotReady", { error: apiStore.error ?? "unknown" }))
    return
  }
  try {
    await store.refresh()
    initialized.value = true
  } catch (e) {
    message.error(t("hosts.message.loadFailed", { error: String(e) }))
  }
}

onMounted(() => {
  if (props.open) ensureLoaded()
})

watch(
  () => props.open,
  (v) => {
    if (v) ensureLoaded()
  },
)

function onCreateHost(parentGid: number) {
  formMode.value = "create"
  formInitial.value = null
  formDefaultGid.value = parentGid
  formOpen.value = true
}

function onEditHost(host: Host) {
  formMode.value = "edit"
  formInitial.value = host
  formDefaultGid.value = host.gid
  formOpen.value = true
}

async function onFormSubmit(data: HostCreate | HostUpdate) {
  formSubmitting.value = true
  try {
    if (formMode.value === "create") {
      await store.addHost(data as HostCreate)
      message.success(t("hosts.message.created"))
    } else {
      const target = formInitial.value
      if (!target) return
      await store.editHost(target.id, data as HostUpdate)
      message.success(t("hosts.message.saved"))
    }
    formOpen.value = false
  } catch (e) {
    message.error(t("hosts.message.saveFailed", { error: String(e) }))
  } finally {
    formSubmitting.value = false
  }
}

function onFormCancel() {
  if (formSubmitting.value) return
  formOpen.value = false
}

/* ---- resize via right-edge drag handle ---- */
const MIN_WIDTH = 240
const DEFAULT_WIDTH = 320
// Drag cap: 90% of viewport width so the drawer never fully covers the main view
function getMaxWidth(): number {
  return Math.round(window.innerWidth * 0.9)
}
const WIDTH_KEY = "ashell:hosts-width"

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
  // Panel anchored to the left edge; width = cursor X.
  const next = Math.round(e.clientX)
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

const panelStyle = computed(() => ({
  width: `${width.value}px`,
  transition: resizing.value ? "none" : "transform 0.25s ease, box-shadow 0.15s ease",
  transform: props.open ? "translateX(0)" : "translateX(-100%)",
}))

function onClose() {
  emit("update:open", false)
}
</script>

<template>
  <Teleport to="body">
    <aside
      class="hosts-panel"
      :class="{ open: props.open, resizing: resizing }"
      :style="panelStyle"
      :aria-hidden="!props.open"
    >
      <div class="hosts-body">
        <HostTree
          @close="onClose"
          @open-host="(n: HostNode, forceNew?: boolean) => emit('open-host', n, forceNew)"
          @create-host="onCreateHost"
          @edit-host="onEditHost"
        />
      </div>
      <div
        class="resize-handle"
        :title="t('hosts.drawer.dragToResize')"
        @pointerdown="onResizeStart"
      />
    </aside>
  </Teleport>

  <NModal v-model:show="formOpen" :mask-closable="!formSubmitting">
    <NCard
      style="width: 760px; max-width: 92vw"
      :title="formMode === 'create' ? t('hosts.drawer.newHost') : t('hosts.drawer.editHost')"
      size="small"
      :bordered="false"
      role="dialog"
      aria-modal="true"
    >
      <HostForm
        :mode="formMode"
        :initial="formInitial"
        :default-gid="formDefaultGid"
        @submit="onFormSubmit"
        @cancel="onFormCancel"
      />
    </NCard>
  </NModal>
</template>

<style scoped>
.hosts-panel {
  position: fixed;
  top: var(--ashell-header-h);
  left: 0;
  bottom: 0;
  background: var(--ashell-panel-bg);
  border-right: 1px solid var(--ashell-border);
  display: flex;
  flex-direction: column;
  z-index: 1000;
  user-select: text;
}

.hosts-panel.open {
  box-shadow: 8px 0 24px var(--ashell-shadow);
}

.hosts-panel.resizing {
  user-select: none;
}

.hosts-body {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.resize-handle {
  position: absolute;
  top: 0;
  right: -3px;
  width: 6px;
  height: 100%;
  cursor: col-resize;
  z-index: 1;
  background: transparent;
  transition: background 0.15s ease;
}

.resize-handle:hover,
.hosts-panel.resizing .resize-handle {
  background: rgba(124, 92, 255, 0.45);
}
</style>
