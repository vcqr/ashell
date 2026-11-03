<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue"
import { NInput } from "naive-ui"
import { useI18n } from "vue-i18n"
import { useHostStore } from "@/stores/hosts"
import { useApiStore } from "@/stores/api"
import { useRecentHostIds } from "@/composables/useRecentHosts"
import { splitHighlightSegments } from "@/utils/hosts-view"
import { isMac } from "@/stores/keybindings"
import type { HostNode, HostWithGroup } from "@/types"

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
const recentIds = useRecentHostIds()

const initialized = ref(false)
const loadError = ref<{ key: string; error: string } | null>(null)
const query = ref("")
const selectedIndex = ref(0)
const inputRef = ref<InstanceType<typeof NInput> | null>(null)
const listRef = ref<HTMLDivElement | null>(null)

async function ensureLoaded() {
  if (initialized.value) return
  if (!apiStore.ready) {
    await apiStore.init()
  }
  if (!apiStore.ready) {
    loadError.value = { key: "backendNotReady", error: apiStore.error ?? "unknown" }
    return
  }
  try {
    await store.refresh()
    initialized.value = true
    loadError.value = null
  } catch (e) {
    loadError.value = { key: "loadFailed", error: String(e) }
  }
}

watch(
  () => props.open,
  (v) => {
    if (!v) return
    query.value = ""
    selectedIndex.value = 0
    void ensureLoaded()
    void nextTick(() => inputRef.value?.focus())
  },
)

// 搜索词变化时选中项回到顶部
watch(query, () => {
  selectedIndex.value = 0
  if (listRef.value) listRef.value.scrollTop = 0
})

// ---------------------------------------------------------------------------
// 渲染模型：header 段只作展示不参与选中；host 行带扁平选中序号 idx
// ---------------------------------------------------------------------------

type PaletteRow =
  | { kind: "header"; label: string; idx: -1 }
  | { kind: "host"; host: HostWithGroup; idx: number }

const hostsSortedByName = computed(() =>
  store.hosts.slice().sort((a, b) => a.name.localeCompare(b.name)),
)

function matchHost(h: HostWithGroup, q: string): boolean {
  return (
    h.name.toLowerCase().includes(q) ||
    h.addr.toLowerCase().includes(q) ||
    h.username.toLowerCase().includes(q)
  )
}

const displayRows = computed<PaletteRow[]>(() => {
  const q = query.value.trim().toLowerCase()
  if (q) {
    return hostsSortedByName.value
      .filter((h) => matchHost(h, q))
      .map((host, idx) => ({ kind: "host" as const, host, idx }))
  }
  // 无搜索词：最近连接（MRU 序，仅取仍存在的主机）在前，其余按名称
  const byId = new Map(store.hosts.map((h) => [h.id, h] as const))
  const recent = recentIds.value
    .map((id) => byId.get(id))
    .filter((h): h is HostWithGroup => !!h)
  const recentIdSet = new Set(recent.map((h) => h.id))
  const rest = hostsSortedByName.value.filter((h) => !recentIdSet.has(h.id))
  const rows: PaletteRow[] = []
  let idx = 0
  if (recent.length > 0) {
    rows.push({ kind: "header", label: t("hosts.palette.recentSection"), idx: -1 })
    for (const host of recent) rows.push({ kind: "host", host, idx: idx++ })
  }
  if (rest.length > 0) {
    rows.push({
      kind: "header",
      label: t(recent.length > 0 ? "hosts.palette.otherSection" : "hosts.palette.allSection"),
      idx: -1,
    })
    for (const host of rest) rows.push({ kind: "host", host, idx: idx++ })
  }
  return rows
})

const hostCount = computed(() => displayRows.value.filter((r) => r.kind === "host").length)

// ---------------------------------------------------------------------------
// 键盘导航与连接
// ---------------------------------------------------------------------------

function scrollToSelected() {
  void nextTick(() => {
    listRef.value
      ?.querySelector<HTMLElement>(`[data-idx="${selectedIndex.value}"]`)
      ?.scrollIntoView({ block: "nearest" })
  })
}

function moveSelection(delta: number) {
  const n = hostCount.value
  if (!n) return
  selectedIndex.value = (selectedIndex.value + delta + n) % n
  scrollToSelected()
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault()
    moveSelection(1)
  } else if (e.key === "ArrowUp") {
    e.preventDefault()
    moveSelection(-1)
  } else if (e.key === "Enter") {
    e.preventDefault()
    const row = displayRows.value.find(
      (r) => r.kind === "host" && r.idx === selectedIndex.value,
    )
    if (row && row.kind === "host") connect(row.host, e.ctrlKey || e.metaKey)
  } else if (e.key === "Escape") {
    e.preventDefault()
    close()
  }
}

function addrText(h: HostWithGroup): string {
  if (h.protocol === "serial") return h.addr
  return h.port !== "22" ? `${h.addr}:${h.port}` : h.addr
}

function toNode(h: HostWithGroup): HostNode {
  return {
    key: `host-${h.id}`,
    label: h.name,
    type: "host",
    id: h.id,
    gid: h.gid,
    host: h.addr,
    port: h.port,
    username: h.username,
    icon: h.icon ?? null,
    color: h.color ?? null,
    desc: h.desc ?? null,
    protocol: h.protocol ?? "ssh",
  }
}

function connect(host: HostWithGroup, forceNew: boolean) {
  emit("open-host", toNode(host), forceNew)
  close()
}

function onRowMousedown(host: HostWithGroup, e: MouseEvent) {
  if (e.button !== 0) return
  connect(host, false)
}

function close() {
  emit("update:open", false)
}
</script>

<template>
  <Teleport to="body">
    <div v-if="props.open" class="qc-backdrop" @mousedown.self="close">
      <div class="qc-panel" role="dialog" aria-modal="true">
        <div class="qc-input-wrap">
          <NInput
            ref="inputRef"
            v-model:value="query"
            :placeholder="t('hosts.palette.placeholder')"
            size="large"
            clearable
            :bordered="false"
            @keydown="onInputKeydown"
          />
        </div>
        <div ref="listRef" class="qc-list">
          <div v-if="loadError" class="qc-empty">
            {{ t(`hosts.message.${loadError.key}`, { error: loadError.error }) }}
          </div>
          <div v-else-if="!store.hosts.length && store.loading" class="qc-empty">
            {{ t("hosts.palette.loading") }}
          </div>
          <div v-else-if="!store.hosts.length" class="qc-empty">
            {{ t("hosts.palette.noHosts") }}
          </div>
          <div v-else-if="!hostCount" class="qc-empty">
            {{ t("hosts.palette.noMatch") }}
          </div>
          <template v-else>
            <template v-for="row in displayRows" :key="row.kind === 'header' ? `h-${row.label}` : `host-${row.host.id}`">
              <div v-if="row.kind === 'header'" class="qc-section">{{ row.label }}</div>
              <div
                v-else
                class="qc-row"
                :class="{ selected: row.idx === selectedIndex }"
                :data-idx="row.idx"
                @mousemove="selectedIndex = row.idx"
                @mousedown.prevent="onRowMousedown(row.host, $event)"
              >
                <span
                  class="qc-dot"
                  :style="{ background: row.host.color || 'var(--ashell-primary)' }"
                />
                <div class="qc-main">
                  <div class="qc-name">
                    <span
                      v-for="(seg, si) in splitHighlightSegments(row.host.name, query)"
                      :key="si"
                      :class="{ hit: seg.hit }"
                    >{{ seg.text }}</span>
                    <span v-if="row.host.protocol && row.host.protocol !== 'ssh'" class="qc-proto">
                      {{ row.host.protocol }}
                    </span>
                  </div>
                  <div class="qc-sub">
                    {{ row.host.username }}<template v-if="row.host.group_name"> · {{ row.host.group_name }}</template>
                  </div>
                </div>
                <span class="qc-addr">
                  <span
                    v-for="(seg, si) in splitHighlightSegments(addrText(row.host), query)"
                    :key="si"
                    :class="{ hit: seg.hit }"
                  >{{ seg.text }}</span>
                </span>
              </div>
            </template>
          </template>
        </div>
        <div class="qc-footer">
          <span class="qc-hint"><kbd>↑</kbd><kbd>↓</kbd>{{ t("hosts.palette.hintSelect") }}</span>
          <span class="qc-hint"><kbd>Enter</kbd>{{ t("hosts.palette.hintConnect") }}</span>
          <span class="qc-hint">
            <kbd>{{ isMac ? "⌘" : "Ctrl" }}</kbd><kbd>Enter</kbd>{{ t("hosts.palette.hintNewTab") }}
          </span>
          <span class="qc-hint"><kbd>Esc</kbd>{{ t("hosts.palette.hintClose") }}</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qc-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.35);
  z-index: 1600;
}

.qc-panel {
  position: absolute;
  top: 12vh;
  left: 50%;
  transform: translateX(-50%);
  width: 600px;
  max-width: 92vw;
  background: var(--ashell-panel-bg);
  border: 1px solid var(--ashell-border);
  border-radius: 10px;
  box-shadow: 0 16px 48px var(--ashell-shadow);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.qc-input-wrap {
  border-bottom: 1px solid var(--ashell-border-soft);
  padding: 4px 8px;
}

.qc-list {
  max-height: 46vh;
  overflow-y: auto;
  padding: 6px;
}

.qc-empty {
  padding: 28px 16px;
  text-align: center;
  color: var(--ashell-text-muted);
  font-size: 13px;
}

.qc-section {
  padding: 8px 10px 4px;
  font-size: 11px;
  color: var(--ashell-text-subtle);
  user-select: none;
}

.qc-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
  user-select: none;
}

.qc-row.selected {
  background: var(--ashell-active);
}

.qc-dot {
  flex: 0 0 auto;
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

.qc-main {
  flex: 1 1 auto;
  min-width: 0;
}

.qc-name {
  font-size: 13px;
  color: var(--ashell-text-strong);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qc-proto {
  margin-left: 6px;
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid var(--ashell-border);
  color: var(--ashell-text-muted);
  text-transform: uppercase;
  vertical-align: 1px;
}

.qc-sub {
  margin-top: 2px;
  font-size: 11px;
  color: var(--ashell-text-subtle);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qc-addr {
  flex: 0 0 auto;
  font-size: 12px;
  color: var(--ashell-text-muted);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  max-width: 45%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qc-name .hit,
.qc-addr .hit {
  color: var(--ashell-accent);
  font-weight: 600;
}

.qc-footer {
  display: flex;
  gap: 16px;
  padding: 8px 12px;
  border-top: 1px solid var(--ashell-border-soft);
  font-size: 11px;
  color: var(--ashell-text-muted);
  user-select: none;
}

.qc-hint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}

.qc-footer kbd {
  font-family: inherit;
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid var(--ashell-border);
  background: var(--ashell-panel-bg-soft);
  color: var(--ashell-text-muted);
}
</style>
