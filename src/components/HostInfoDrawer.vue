<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  NAlert,
  NButton,
  NCollapse,
  NCollapseItem,
  NDescriptions,
  NDescriptionsItem,
  NEmpty,
  NIcon,
  NProgress,
  NSelect,
  NSpace,
  NSpin,
  NTabPane,
  NTabs,
} from "naive-ui"
import type { SelectOption } from "naive-ui"
import { CloseOutline, RefreshOutline } from "@vicons/ionicons5"
import { useI18n } from "vue-i18n"
import { getSysInfo } from "@/api/sysinfo"
import { ApiError } from "@/api/client"
import type { ProcStat, SysInfo } from "@/types"
import { humanSize } from "@/utils/humanSize"
import { humanRate } from "@/utils/humanRate"
import { humanUptime } from "@/utils/humanUptime"
import { useIconStore } from "@/stores/icons"
import NetSparkline from "@/components/host/NetSparkline.vue"

interface Props {
  open: boolean
  sid: string | null
  hostName?: string
  /** host.icon 文件名（来自 ~/.ashell/icons），有值时在标题前显示缩略图 */
  hostIcon?: string | null
  hostInfo?: {
    addr: string
    port: string
    username: string
  }
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
}>()

const { t } = useI18n()
const iconStore = useIconStore()
onMounted(() => {
  void iconStore.ensureLoaded()
})

const hostIconUrl = computed(() => iconStore.urlOf(props.hostIcon))

const loading = ref(false)
const refreshing = ref(false)
const error = ref<string | null>(null)
const info = ref<SysInfo | null>(null)

/** 网络速率历史（B/s），最多 60 个点 */
const SERIES_CAP = 60

/** 单个网卡 / 合计的采样状态 */
interface NicSample {
  rxSeries: number[]
  txSeries: number[]
  lastRx: number | null
  lastTx: number | null
  currentRx: number
  currentTx: number
}

/** key 为网卡名；特殊 key "__all__" 表示合计 */
const ALL_KEY = "__all__"
const nicSamples = ref<Map<string, NicSample>>(new Map())
/** 上次采样时刻（所有网卡共用，因为同一次 refresh 出来的） */
const lastSampleAt = ref<number>(0)
/** 当前选中网卡 key */
const selectedNic = ref<string>(ALL_KEY)

let timer: ReturnType<typeof setInterval> | null = null
const POLL_INTERVAL = 1500

const drawerTitle = computed(() => {
  const host = props.hostName ?? t("hostInfo.title")
  const addr = props.hostInfo?.addr?.trim()
  return addr ? `${host} (${addr})` : host
})

const connectionLabel = computed(() => {
  if (!props.hostInfo) return "-"
  const { addr, port } = props.hostInfo
  if (!addr) return "-"
  return port ? `${addr}:${port}` : addr
})

const usernameLabel = computed(() => props.hostInfo?.username ?? "-")

const memPercent = computed(() => {
  if (!info.value || info.value.mem_total_kb <= 0) return 0
  return clamp((info.value.mem_used_kb / info.value.mem_total_kb) * 100)
})
const swapPercent = computed(() => {
  if (!info.value || info.value.swap_total_kb <= 0) return 0
  return clamp((info.value.swap_used_kb / info.value.swap_total_kb) * 100)
})
const cpuPercent = computed(() =>
  info.value ? clamp(info.value.cpu_percent) : 0,
)

const rootDisk = computed(() => {
  if (!info.value || info.value.disks.length === 0) return null
  return info.value.disks.find((d) => d.mount === "/") ?? info.value.disks[0]!
})

const rootDiskPercent = computed(() => {
  const d = rootDisk.value
  if (!d || d.total_bytes <= 0) return 0
  return clamp((d.used_bytes / d.total_bytes) * 100)
})

const otherDisks = computed(() => {
  if (!info.value) return []
  const root = rootDisk.value
  return info.value.disks.filter((d) => d !== root)
})

/* ---------- 网卡相关 ---------- */
const nicOptions = computed<SelectOption[]>(() => {
  const opts: SelectOption[] = [
    {
      label: t("hostInfo.labels.allNics", { count: (info.value?.nics ?? []).length }),
      value: ALL_KEY,
    },
  ]
  for (const n of info.value?.nics ?? []) {
    opts.push({ label: n.name, value: n.name })
  }
  return opts
})

const currentSample = computed<NicSample | null>(() => {
  return nicSamples.value.get(selectedNic.value) ?? null
})

const currentRxRate = computed(() => currentSample.value?.currentRx ?? 0)
const currentTxRate = computed(() => currentSample.value?.currentTx ?? 0)
const currentRxSeries = computed(() => currentSample.value?.rxSeries ?? [])
const currentTxSeries = computed(() => currentSample.value?.txSeries ?? [])

/* ---------- Top 进程 ---------- */
const topTab = ref<"cpu" | "mem">("cpu")
const topList = computed<ProcStat[]>(() => {
  if (!info.value) return []
  return topTab.value === "cpu" ? info.value.top_cpu : info.value.top_mem
})

/** 当前显示模式：是 procps 路径还是 BusyBox /proc 路径 */
const isProcMode = computed(() => {
  const list = [...(info.value?.top_cpu ?? []), ...(info.value?.top_mem ?? [])]
  return list.some(
    (p) => p.cpu_time_secs !== undefined || p.mem_rss_kb !== undefined,
  )
})

const topValueHeader = computed(() => {
  if (topTab.value === "cpu") return isProcMode.value ? "TIME" : "CPU%"
  return isProcMode.value ? "RSS" : "MEM%"
})

function formatCpuSecs(secs: number): string {
  const s = Math.max(0, Math.floor(secs))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const ss = s % 60
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(ss).padStart(2, "0")}`
  return `${m}:${String(ss).padStart(2, "0")}`
}

function formatTopValue(p: ProcStat): string {
  if (topTab.value === "cpu") {
    if (p.cpu_time_secs !== undefined) return formatCpuSecs(p.cpu_time_secs)
    return `${p.cpu_percent.toFixed(1)}`
  }
  if (p.mem_rss_kb !== undefined) return humanSize(p.mem_rss_kb * 1024)
  return `${p.mem_percent.toFixed(1)}`
}

function clamp(v: number): number {
  if (!Number.isFinite(v)) return 0
  if (v < 0) return 0
  if (v > 100) return 100
  return v
}

function formatKB(kb: number): string {
  return humanSize(kb * 1024)
}

function pushSeries(arr: number[], v: number) {
  arr.push(v)
  if (arr.length > SERIES_CAP) arr.shift()
}

function ensureSample(key: string): NicSample {
  let s = nicSamples.value.get(key)
  if (!s) {
    s = {
      rxSeries: [],
      txSeries: [],
      lastRx: null,
      lastTx: null,
      currentRx: 0,
      currentTx: 0,
    }
    nicSamples.value.set(key, s)
  }
  return s
}

function updateSample(
  s: NicSample,
  rxBytes: number,
  txBytes: number,
  dt: number,
) {
  if (s.lastRx !== null && s.lastTx !== null && dt > 0) {
    const rx = Math.max(0, (rxBytes - s.lastRx) / dt)
    const tx = Math.max(0, (txBytes - s.lastTx) / dt)
    s.currentRx = rx
    s.currentTx = tx
    pushSeries(s.rxSeries, rx)
    pushSeries(s.txSeries, tx)
  }
  s.lastRx = rxBytes
  s.lastTx = txBytes
}

async function refresh() {
  if (!props.sid) return
  const sid = props.sid
  if (info.value === null) loading.value = true
  refreshing.value = true
  try {
    const data = await getSysInfo(sid)
    if (sid !== props.sid) return // 期间 sid 已切换，丢弃
    info.value = data
    error.value = null

    const now = Date.now()
    const dt = lastSampleAt.value > 0 ? (now - lastSampleAt.value) / 1000 : 0

    // 合计
    const allSample = ensureSample(ALL_KEY)
    updateSample(allSample, data.net_rx_bytes, data.net_tx_bytes, dt)

    // 单卡
    const seenNames = new Set<string>([ALL_KEY])
    for (const n of data.nics) {
      seenNames.add(n.name)
      const s = ensureSample(n.name)
      updateSample(s, n.rx_bytes, n.tx_bytes, dt)
    }
    // 清理掉不再存在的网卡历史，避免下拉残留
    for (const k of [...nicSamples.value.keys()]) {
      if (!seenNames.has(k)) nicSamples.value.delete(k)
    }
    // 若当前选中的网卡已消失，回退到合计
    if (!seenNames.has(selectedNic.value)) {
      selectedNic.value = ALL_KEY
    }

    lastSampleAt.value = now
  } catch (e) {
    const msg = e instanceof ApiError ? e.message : (e as Error).message
    error.value = msg
    stopPolling()
  } finally {
    loading.value = false
    refreshing.value = false
  }
}

function startPolling() {
  stopPolling()
  if (!props.sid) return
  void refresh()
  timer = setInterval(() => {
    void refresh()
  }, POLL_INTERVAL)
}

function stopPolling() {
  if (timer !== null) {
    clearInterval(timer)
    timer = null
  }
}

function resetSeries() {
  nicSamples.value = new Map()
  lastSampleAt.value = 0
  selectedNic.value = ALL_KEY
}

function manualRefresh() {
  void refresh()
}

function onClose() {
  emit("update:open", false)
}

watch(
  () => [props.open, props.sid] as const,
  ([open, sid], _old) => {
    if (open && sid) {
      // sid 变化时清掉旧序列
      resetSeries()
      info.value = null
      error.value = null
      startPolling()
    } else {
      stopPolling()
    }
  },
  { immediate: true },
)

onBeforeUnmount(stopPolling)

/* ---------- 拖拽改变面板宽度 ---------- */
const MIN_WIDTH = 380
const DEFAULT_WIDTH = 480
// 拖动上限取视口宽度的 90%，避免抽屉完全盖住主界面
function getMaxWidth(): number {
  return Math.round(window.innerWidth * 0.9)
}
const WIDTH_KEY = "ashell:hostinfo-width"

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

onBeforeUnmount(onResizeEnd)

const panelStyle = computed(() => ({
  width: `${width.value}px`,
  transition: resizing.value ? "none" : "transform 0.25s ease, box-shadow 0.15s ease",
  transform: props.open ? "translateX(0)" : "translateX(100%)",
}))
</script>

<template>
  <Teleport to="body">
    <aside
      class="hostinfo-panel"
      :class="{ open: props.open, resizing: resizing }"
      :style="panelStyle"
      :aria-hidden="!props.open"
    >
      <div
        class="resize-handle"
        :title="t('hostInfo.dragToResize')"
        @pointerdown="onResizeStart"
      />

      <header class="panel-header">
        <div class="drawer-title-wrap">
          <img
            v-if="hostIconUrl"
            :src="hostIconUrl"
            class="drawer-title-icon"
            alt=""
          />
          <span class="drawer-title">{{ drawerTitle }}</span>
        </div>
        <NSpace :size="6" align="center" :wrap="false">
          <NButton
            size="small"
            quaternary
            circle
            :loading="refreshing && info !== null"
            :title="t('hostInfo.refresh')"
            @click="manualRefresh"
          >
            <template #icon>
              <NIcon><RefreshOutline /></NIcon>
            </template>
          </NButton>
          <NButton size="small" quaternary circle :title="t('hostInfo.close')" @click="onClose">
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </NSpace>
      </header>

      <div class="panel-body">
        <div v-if="!props.sid" class="empty-wrap">
          <NEmpty :description="t('hostInfo.needSession')" />
        </div>

        <NSpin v-else :show="loading" class="content-spin">
          <div class="content-scroll">
            <NAlert
              v-if="error"
              type="error"
              :show-icon="false"
              class="error-alert"
            >
              {{ error }}
            </NAlert>

            <section class="card">
              <div class="card-title">{{ t("hostInfo.basicInfo") }}</div>
              <NDescriptions
                :column="2"
                size="small"
                label-placement="left"
                bordered
                :label-style="{
                  color: 'var(--ashell-text-muted)',
                  fontSize: '13px',
                }"
                :content-style="{ fontSize: '13px' }"
                class="basic-desc"
              >
                <NDescriptionsItem :label="t('hostInfo.labels.connIp')">
                  <span class="info-value mono">{{ connectionLabel }}</span>
                </NDescriptionsItem>
                <NDescriptionsItem :label="t('hostInfo.labels.loginUser')">
                  <span class="info-value mono">{{ usernameLabel }}</span>
                </NDescriptionsItem>
                <NDescriptionsItem :label="t('hostInfo.labels.hostname')">
                  <span class="info-value">{{ info?.hostname || "-" }}</span>
                </NDescriptionsItem>
                <NDescriptionsItem :label="t('hostInfo.labels.arch')">
                  <span class="info-value mono">{{ info?.arch || "-" }}</span>
                </NDescriptionsItem>
                <NDescriptionsItem :label="t('hostInfo.labels.os')" :span="2">
                  <span class="info-value">{{ info?.os_pretty || "-" }}</span>
                </NDescriptionsItem>
                <NDescriptionsItem :label="t('hostInfo.labels.kernel')" :span="2">
                  <span class="info-value mono">{{ info?.kernel || "-" }}</span>
                </NDescriptionsItem>
                <NDescriptionsItem :label="t('hostInfo.labels.uptime')" :span="2">
                  <span class="info-value">
                    {{ info ? humanUptime(info.uptime_secs, t) : "-" }}
                  </span>
                </NDescriptionsItem>
              </NDescriptions>
            </section>

            <section class="card">
              <div class="card-title">{{ t("hostInfo.resources") }}</div>
              <div class="metric-list">
                <div class="metric-row">
                  <div class="metric-row-head">
                    <span class="metric-row-label">CPU</span>
                    <span class="metric-row-value">
                      <span class="metric-row-pct">
                        {{ cpuPercent.toFixed(1) }}%
                      </span>
                      <span class="metric-row-detail">
                        {{ info ? t("hostInfo.labels.cores", { count: info.cpu_cores }) : "-" }}
                      </span>
                    </span>
                  </div>
                  <NProgress
                    type="line"
                    :percentage="cpuPercent"
                    :show-indicator="false"
                    :height="8"
                    :border-radius="4"
                    color="#7c5cff"
                    rail-color="var(--ashell-hover)"
                  />
                </div>

                <div class="metric-row">
                  <div class="metric-row-head">
                    <span class="metric-row-label">{{ t("hostInfo.labels.memory") }}</span>
                    <span class="metric-row-value">
                      <span class="metric-row-pct">
                        {{ memPercent.toFixed(1) }}%
                      </span>
                      <span class="metric-row-detail">
                        {{
                          info
                            ? `${formatKB(info.mem_used_kb)} / ${formatKB(
                                info.mem_total_kb,
                              )}`
                            : "-"
                        }}
                      </span>
                    </span>
                  </div>
                  <NProgress
                    type="line"
                    :percentage="memPercent"
                    :show-indicator="false"
                    :height="8"
                    :border-radius="4"
                    color="#4ade80"
                    rail-color="var(--ashell-hover)"
                  />
                </div>

                <div class="metric-row">
                  <div class="metric-row-head">
                    <span class="metric-row-label">{{ t("hostInfo.labels.swap") }}</span>
                    <span class="metric-row-value">
                      <span class="metric-row-pct">
                        {{ swapPercent.toFixed(1) }}%
                      </span>
                      <span class="metric-row-detail">
                        <template v-if="info && info.swap_total_kb > 0">
                          {{ formatKB(info.swap_used_kb) }} /
                          {{ formatKB(info.swap_total_kb) }}
                        </template>
                        <template v-else>{{ t("hostInfo.labels.swapDisabled") }}</template>
                      </span>
                    </span>
                  </div>
                  <NProgress
                    type="line"
                    :percentage="swapPercent"
                    :show-indicator="false"
                    :height="8"
                    :border-radius="4"
                    color="#f59e0b"
                    rail-color="var(--ashell-hover)"
                  />
                </div>

                <div class="metric-row">
                  <div class="metric-row-head">
                    <span class="metric-row-label">
                      {{ t("hostInfo.disk", { count: rootDisk?.mount ?? "/" }) }}
                    </span>
                    <span class="metric-row-value">
                      <span class="metric-row-pct">
                        {{ rootDiskPercent.toFixed(1) }}%
                      </span>
                      <span class="metric-row-detail">
                        {{
                          rootDisk
                            ? `${humanSize(rootDisk.used_bytes)} / ${humanSize(
                                rootDisk.total_bytes,
                              )}`
                            : "-"
                        }}
                      </span>
                    </span>
                  </div>
                  <NProgress
                    type="line"
                    :percentage="rootDiskPercent"
                    :show-indicator="false"
                    :height="8"
                    :border-radius="4"
                    color="#4a8cff"
                    rail-color="var(--ashell-hover)"
                  />
                </div>
              </div>

              <NCollapse v-if="otherDisks.length > 0" class="other-disks">
                <NCollapseItem
                  :title="t('hostInfo.otherMounts', { count: otherDisks.length })"
                  name="more"
                >
                  <div v-for="d in otherDisks" :key="d.mount" class="disk-row">
                    <div class="disk-row-head">
                      <span class="disk-mount mono">{{ d.mount }}</span>
                      <span class="disk-detail">
                        {{ humanSize(d.used_bytes) }} /
                        {{ humanSize(d.total_bytes) }}
                      </span>
                    </div>
                    <NProgress
                      type="line"
                      :percentage="
                        d.total_bytes > 0
                          ? clamp((d.used_bytes / d.total_bytes) * 100)
                          : 0
                      "
                      :show-indicator="false"
                      :height="6"
                      color="#4a8cff"
                    />
                  </div>
                </NCollapseItem>
              </NCollapse>
            </section>

            <section class="card">
              <div class="net-head">
                <div class="card-title net-title">{{ t("hostInfo.network") }}</div>
                <NSelect
                  v-model:value="selectedNic"
                  size="tiny"
                  :options="nicOptions"
                  class="nic-select"
                />
              </div>
              <div class="net-stats">
                <span class="net-stat">
                  <span class="net-dot rx" />
                  ↓ {{ humanRate(currentRxRate) }}
                </span>
                <span class="net-stat">
                  <span class="net-dot tx" />
                  ↑ {{ humanRate(currentTxRate) }}
                </span>
              </div>
              <NetSparkline
                :rx-series="currentRxSeries"
                :tx-series="currentTxSeries"
                :height="96"
              />
              <div v-if="currentRxSeries.length === 0" class="net-hint">
                {{ t("hostInfo.collecting") }}
              </div>
            </section>

            <section class="card proc-card">
              <div class="proc-head-bar">
                <div class="card-title proc-title">{{ t("hostInfo.topProcesses") }}</div>
                <NTabs
                  v-model:value="topTab"
                  type="segment"
                  size="small"
                  animated
                  class="proc-tabs"
                >
                  <NTabPane name="cpu" tab="CPU" />
                  <NTabPane name="mem" :tab="t('hostInfo.tabs.memory')" />
                </NTabs>
              </div>
              <div class="proc-table">
                <div class="proc-row proc-row-head">
                  <span class="col-pid">PID</span>
                  <span class="col-user">USER</span>
                  <span class="col-pct">{{ topValueHeader }}</span>
                  <span class="col-cmd">COMMAND</span>
                </div>
                <div
                  v-for="p in topList"
                  :key="`${topTab}-${p.pid}`"
                  class="proc-row"
                >
                  <span class="col-pid mono">{{ p.pid }}</span>
                  <span class="col-user mono" :title="p.user">{{ p.user }}</span>
                  <span class="col-pct mono">{{ formatTopValue(p) }}</span>
                  <span class="col-cmd mono" :title="p.command">
                    {{ p.command }}
                  </span>
                </div>
                <div v-if="topList.length === 0" class="proc-empty">
                  {{ t("hostInfo.noData") }}
                </div>
              </div>
            </section>
          </div>
        </NSpin>
      </div>
    </aside>
  </Teleport>
</template>

<style scoped>
.hostinfo-panel {
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

.hostinfo-panel.open {
  box-shadow: -8px 0 24px var(--ashell-shadow);
}

.hostinfo-panel.resizing {
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
.hostinfo-panel.resizing .resize-handle {
  background: rgba(124, 92, 255, 0.45);
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

.drawer-title-wrap {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.drawer-title-icon {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  object-fit: contain;
  flex-shrink: 0;
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

.panel-body {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
}

.empty-wrap {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1 1 auto;
  min-height: 240px;
}

.content-spin {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.content-spin :deep(.n-spin-container),
.content-spin :deep(.n-spin-content) {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.content-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.error-alert {
  margin-bottom: 4px;
}

.card {
  background: var(--ashell-panel-bg-soft);
  border: 1px solid var(--ashell-border-soft);
  border-radius: 10px;
  padding: 12px 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.card-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
}

/* 基本信息字段统一字号 */
.info-value {
  font-size: 13px;
  color: var(--ashell-text);
  word-break: break-all;
}
.info-value.mono {
  font-family: var(--n-font-family-mono);
}

/* 基本信息双列紧凑样式 */
.basic-desc :deep(.n-descriptions-table-wrapper) {
  border-radius: 6px;
}
.basic-desc :deep(th.n-descriptions-table-header),
.basic-desc :deep(td.n-descriptions-table-content) {
  padding: 6px 10px !important;
}
.basic-desc :deep(th.n-descriptions-table-header) {
  white-space: nowrap;
  width: 1%;
}

.metric-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.metric-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.metric-row-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}

.metric-row-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--ashell-text);
  flex-shrink: 0;
}

.metric-row-value {
  display: inline-flex;
  align-items: baseline;
  gap: 8px;
  flex-shrink: 0;
  min-width: 0;
  text-align: right;
}

.metric-row-pct {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  font-variant-numeric: tabular-nums;
}

.metric-row-detail {
  font-size: 11px;
  color: var(--ashell-text-muted);
  font-variant-numeric: tabular-nums;
}

.other-disks {
  margin-top: 4px;
}

.other-disks :deep(.n-collapse-item__header-main) {
  font-size: 12px;
  color: var(--ashell-text-muted);
}

.disk-row {
  padding: 6px 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.disk-row + .disk-row {
  border-top: 1px dashed var(--ashell-border-soft);
}

.disk-row-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.disk-mount {
  color: var(--ashell-text-strong);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1 1 auto;
  min-width: 0;
}

.disk-detail {
  color: var(--ashell-text-muted);
  flex-shrink: 0;
}

.net-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.net-title {
  flex: 0 0 auto;
}

.nic-select {
  flex: 0 1 180px;
  min-width: 120px;
}

.net-stats {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--ashell-text);
}

.net-stat {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.net-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}
.net-dot.rx {
  background: #7c5cff;
}
.net-dot.tx {
  background: #4a8cff;
}

.net-hint {
  font-size: 11px;
  color: var(--ashell-text-subtle);
  text-align: center;
  margin-top: 4px;
}

/* Top 进程表 */
.proc-card {
  gap: 4px;
  padding-bottom: 10px;
}

.proc-head-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.proc-title {
  flex: 0 0 auto;
  white-space: nowrap;
}

.proc-tabs {
  flex: 0 0 auto;
  width: 140px;
}

.proc-tabs :deep(.n-tabs-tab) {
  --n-tab-padding: 0 10px !important;
  padding: 2px 10px !important;
  min-height: 22px !important;
  font-size: 12px !important;
}

.proc-tabs :deep(.n-tabs-rail) {
  padding: 2px !important;
}

.proc-tabs :deep(.n-tabs-nav) {
  margin-bottom: 0 !important;
}

.proc-tabs :deep(.n-tab-pane) {
  padding: 0 !important;
}

.proc-table {
  display: flex;
  flex-direction: column;
}

.proc-row {
  display: grid;
  grid-template-columns: 52px 72px 60px 1fr;
  gap: 8px;
  align-items: center;
  padding: 5px 6px;
  font-size: 12px;
  line-height: 1.5;
  border-radius: 3px;
}

.proc-row.proc-row-head {
  font-size: 10px;
  color: var(--ashell-text-muted);
  font-weight: 500;
  letter-spacing: 0.04em;
  padding: 2px 6px 3px;
}

.proc-row:not(.proc-row-head):nth-child(even) {
  background: var(--ashell-hover, rgba(120, 120, 120, 0.05));
}

.proc-row .col-pid,
.proc-row .col-pct {
  font-variant-numeric: tabular-nums;
  text-align: right;
}

.proc-row .col-user {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ashell-text);
}

.proc-row .col-cmd {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ashell-text);
}

.proc-empty {
  padding: 8px 0;
  text-align: center;
  font-size: 12px;
  color: var(--ashell-text-subtle);
}

.mono {
  font-family: var(--n-font-family-mono);
}
</style>
