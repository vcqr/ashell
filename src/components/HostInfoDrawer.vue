<script setup lang="ts">
import {
  RiCloseLine,
  RiExternalLinkLine,
  RiRefreshLine,
} from "@remixicon/vue"
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

import { useI18n } from "vue-i18n"
import { getSysInfo } from "@/api/sysinfo"
import { ApiError } from "@/api/client"
import type { DiskUsage, ProcStat, SysInfo } from "@/types"
import { humanSize } from "@/utils/humanSize"
import { humanRate } from "@/utils/humanRate"
import { humanUptime } from "@/utils/humanUptime"
import { useDrawerWidth } from "@/composables/useDrawerWidth"
import { usePanelSwitchClosed } from "@/composables/usePanels"
import { useIconStore } from "@/stores/icons"
import NetSparkline from "@/components/host/NetSparkline.vue"
import { isTauri } from "@/utils/platform"
import { openHostInfoInNewWindow } from "@/utils/newWindow"

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
  /** 独立窗口模式：铺满窗口、无宽度拖拽，面板头由窗口标题栏接管 */
  standalone?: boolean
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

let timer: ReturnType<typeof setTimeout> | null = null
const POLL_INTERVAL = 1500
/** 连续失败后的退避轮询间隔：瞬时故障自动恢复，面板不永久停摆 */
const ERROR_POLL_INTERVAL = 5000
let consecutiveErrors = 0

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
/** swap 行是否显示：数据未到时先占位，确认未启用后整行隐藏 */
const showSwapRow = computed(() => !info.value || info.value.swap_total_kb > 0)

/** 内存行右侧明细：用量 + 规格（类型/频率来自 dmidecode，条数非 0 时一并列出） */
const memDetail = computed(() => {
  if (!info.value) return "-"
  const spec: string[] = []
  if (info.value.mem_type) {
    spec.push(
      info.value.mem_type + (info.value.mem_speed ? ` ${info.value.mem_speed}` : ""),
    )
  }
  if (info.value.dimm_count > 0) {
    spec.push(t("hostInfo.labels.dimmCount", { count: info.value.dimm_count }))
  }
  return `${formatKB(info.value.mem_used_kb)} / ${formatKB(info.value.mem_total_kb)}${
    spec.length > 0 ? ` · ${spec.join(" · ")}` : ""
  }`
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

/* ---------- 温度 / 显卡 ---------- */
const displayTemps = computed(() => (info.value?.temps ?? []).slice(0, 12))
const gpus = computed(() => info.value?.gpus ?? [])

/** 温度分级配色：≥80°C 高温、≥60°C 偏高、其余正常 */
function tempClass(c: number): string {
  if (c >= 80) return "temp-hot"
  if (c >= 60) return "temp-warm"
  return "temp-cool"
}

/** MiB 转紧凑容量：≥1 GiB 用一位小数的 GB（19.8 GB / 24 GB），避免撑爆值列 */
function formatMB(mb: number): string {
  if (mb >= 1024) {
    return `${Math.round((mb / 1024) * 10) / 10} GB`
  }
  return `${Math.round(mb)} MB`
}

/** 瓦数：整数不带小数，否则保留一位（93.6 W / 450 W） */
function formatWatts(w: number): string {
  const r = Math.round(w * 10) / 10
  return Number.isInteger(r) ? String(r) : r.toFixed(1)
}

/** 占用条阈值配色：≥80% 红、≥60% 琥珀、其余基础色（风扇等非告警指标不用） */
function gaugeColor(v: number, base: string): string {
  if (v >= 80) return "#ef4444"
  if (v >= 60) return "#f59e0b"
  return base
}

/** 磁盘占用百分比（容量为 0 时按 0 处理） */
function diskPercentOf(d: DiskUsage): number {
  if (d.total_bytes <= 0) return 0
  return clamp((d.used_bytes / d.total_bytes) * 100)
}

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
  // 上一次请求在途时忽略重复触发：轮询与手动刷新互斥，复合采集脚本较重不并发跑
  if (refreshing.value) return
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
    consecutiveErrors = 0
  } catch (e) {
    const msg = e instanceof ApiError ? e.message : (e as Error).message
    error.value = msg
    consecutiveErrors++
  } finally {
    loading.value = false
    refreshing.value = false
    // 成功回到常规节奏，失败退避慢速重试；面板已关/sid 已清则不再排程
    if (props.open && props.sid) {
      scheduleNext(consecutiveErrors > 0 ? ERROR_POLL_INTERVAL : POLL_INTERVAL)
    }
  }
}

function scheduleNext(delay: number) {
  if (timer !== null) clearTimeout(timer)
  timer = setTimeout(() => {
    timer = null
    void refresh()
  }, delay)
}

function startPolling() {
  stopPolling()
  if (!props.sid) return
  consecutiveErrors = 0
  // 立即拉一次；后续排程由 refresh 的 finally 链式驱动
  void refresh()
}

function stopPolling() {
  if (timer !== null) {
    clearTimeout(timer)
    timer = null
  }
}

function resetSeries() {
  nicSamples.value = new Map()
  lastSampleAt.value = 0
  selectedNic.value = ALL_KEY
}

function onClose() {
  // 独立窗口模式下"关闭"语义是关掉整个窗口
  if (props.standalone) {
    if (isTauri) {
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
        getCurrentWindow().close(),
      )
    } else {
      window.close()
    }
    return
  }
  emit("update:open", false)
}

/** 弹出到独立窗口（复用当前 SSH 会话）。
 *  "移动"语义：与 SFTP 面板一致，弹出后收起本抽屉，避免挡住终端 */
function openInStandaloneWindow() {
  if (!props.sid) return
  void openHostInfoInNewWindow({
    sid: props.sid,
    title: props.hostName,
    addr: props.hostInfo?.addr,
  })
  emit("update:open", false)
}

/** 供独立窗口标题栏的刷新按钮调用（HostInfoWindow 经 ref 使用） */
function manualRefresh() {
  void refresh()
}

defineExpose({ manualRefresh })

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
const { width, resizing, onResizeStart } = useDrawerWidth({
  storageKey: "ashell:hostinfo-width",
  minWidth: 380,
  defaultWidth: 480,
})

// 被互斥切换关闭：跳过离场滑动动画，避免旧面板滑出与新面板滑入交叠出重影
const switchClosed = usePanelSwitchClosed("hostInfo")

const panelStyle = computed(() => {
  // 独立窗口模式：铺满窗口（顶部让位给窗口标题栏），无滑入动画
  if (props.standalone) {
    return { width: "auto", transition: "none" }
  }
  return {
    width: `${width.value}px`,
    transition:
      resizing.value || switchClosed.value
        ? "none"
        : "transform 0.25s ease, box-shadow 0.15s ease",
    // 关闭态平移叠加活动栏宽度，理由同 AiAssistant（锚定 right: 活动栏宽）
    transform: props.open
      ? "translateX(0)"
      : "translateX(calc(100% + var(--ashell-activity-w, 0px)))",
  }
})
</script>

<template>
  <Teleport to="body">
    <aside
      class="hostinfo-panel"
      :class="{ open: props.open, resizing: resizing, standalone: props.standalone }"
      :style="panelStyle"
      :aria-hidden="!props.open"
    >
      <div
        v-if="!props.standalone"
        class="resize-handle"
        :title="t('hostInfo.dragToResize')"
        @pointerdown="onResizeStart"
      />

      <!-- 独立窗口模式下不渲染面板头：标题由 HostInfoWindow 标题栏承担，避免双标题 -->
      <header v-if="!props.standalone" class="panel-header">
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
            :disabled="!props.sid"
            :title="t('hostInfo.openInNewWindow')"
            @click="openInStandaloneWindow"
          >
            <template #icon>
              <NIcon><RiExternalLinkLine /></NIcon>
            </template>
          </NButton>
          <NButton
            size="small"
            quaternary
            circle
            :loading="refreshing"
            :title="t('hostInfo.refresh')"
            @click="manualRefresh"
          >
            <template #icon>
              <NIcon><RiRefreshLine /></NIcon>
            </template>
          </NButton>
          <NButton size="small" quaternary circle :title="t('hostInfo.close')" @click="onClose">
            <template #icon>
              <NIcon><RiCloseLine /></NIcon>
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
              <div class="error-alert-body">
                <span class="error-alert-text">{{ error }}</span>
                <NButton size="tiny" type="error" secondary :loading="refreshing" @click="manualRefresh">
                  {{ t("hostInfo.errorRetry") }}
                </NButton>
              </div>
              <div class="error-alert-hint">{{ t("hostInfo.errorHint") }}</div>
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
                <NDescriptionsItem
                  v-if="info?.cpu_model"
                  :label="t('hostInfo.labels.cpuModel')"
                  :span="2"
                >
                  <span class="info-value mono" :title="info.cpu_model">
                    {{ info.cpu_model }}
                  </span>
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
                    :color="gaugeColor(cpuPercent, '#7c5cff')"
                    rail-color="var(--ashell-hover)"
                  />
                </div>

                <div v-if="displayTemps.length > 0" class="temp-block">
                  <span
                    v-for="tp in displayTemps"
                    :key="tp.label"
                    class="temp-chip"
                    :class="tempClass(tp.temp_c)"
                    :title="tp.label"
                  >
                    {{ tp.label }}
                    <b>{{ tp.temp_c.toFixed(0) }}°C</b>
                  </span>
                </div>

                <div class="metric-row">
                  <div class="metric-row-head">
                    <span class="metric-row-label">{{ t("hostInfo.labels.memory") }}</span>
                    <span class="metric-row-value">
                      <span class="metric-row-pct">
                        {{ memPercent.toFixed(1) }}%
                      </span>
                      <span class="metric-row-detail">
                        {{ memDetail }}
                      </span>
                    </span>
                  </div>
                  <NProgress
                    type="line"
                    :percentage="memPercent"
                    :show-indicator="false"
                    :height="8"
                    :border-radius="4"
                    :color="gaugeColor(memPercent, '#4ade80')"
                    rail-color="var(--ashell-hover)"
                  />
                </div>

                <div v-if="showSwapRow" class="metric-row">
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
                    :color="gaugeColor(swapPercent, '#f59e0b')"
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
                              )}${rootDisk.model ? ` · ${rootDisk.model}` : ""}`
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
                    :color="gaugeColor(rootDiskPercent, '#4a8cff')"
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
                        <template v-if="d.model">· {{ d.model }}</template>
                      </span>
                    </div>
                    <NProgress
                      type="line"
                      :percentage="diskPercentOf(d)"
                      :show-indicator="false"
                      :height="6"
                      :color="gaugeColor(diskPercentOf(d), '#4a8cff')"
                    />
                  </div>
                </NCollapseItem>
              </NCollapse>
            </section>

            <section v-if="gpus.length > 0" class="card">
              <div class="card-title">{{ t("hostInfo.gpu") }}</div>
              <div
                v-for="(g, i) in gpus"
                :key="`${g.vendor}-${i}`"
                class="gpu-item"
              >
                <div class="gpu-head">
                  <span class="gpu-name" :title="g.name">
                    <span v-if="gpus.length > 1" class="gpu-index mono">
                      #{{ g.index }}
                    </span>
                    {{ g.name }}
                  </span>
                  <span class="gpu-head-tags">
                    <span
                      v-if="g.health && g.health !== 'OK'"
                      class="gpu-health"
                      :class="g.health === 'Error' ? 'health-error' : 'health-warn'"
                    >
                      {{ g.health }}
                    </span>
                    <span v-if="g.perf_state" class="gpu-chip mono">
                      {{ g.perf_state }}
                    </span>
                    <span v-if="g.pcie" class="gpu-chip mono">{{ g.pcie }}</span>
                    <span
                      v-if="g.temp_c > 0"
                      class="gpu-temp"
                      :class="tempClass(g.temp_c)"
                    >
                      {{ g.temp_c.toFixed(0) }}°C
                    </span>
                  </span>
                </div>
                <div class="gpu-row">
                  <span class="gpu-row-label">{{ t("hostInfo.gpuUtil") }}</span>
                  <div class="gpu-row-bar">
                    <NProgress
                      type="line"
                      :percentage="clamp(g.util_percent)"
                      :show-indicator="false"
                      :height="6"
                      :color="gaugeColor(g.util_percent, '#7c5cff')"
                      rail-color="var(--ashell-hover)"
                    />
                  </div>
                  <span class="gpu-row-value mono">
                    {{ g.util_percent.toFixed(0) }}%
                  </span>
                </div>
                <div v-if="g.mem_total_mb > 0" class="gpu-row">
                  <span class="gpu-row-label">{{ t("hostInfo.gpuVram") }}</span>
                  <div class="gpu-row-bar">
                    <NProgress
                      type="line"
                      :percentage="
                        clamp((g.mem_used_mb / g.mem_total_mb) * 100)
                      "
                      :show-indicator="false"
                      :height="6"
                      :color="
                        gaugeColor(
                          (g.mem_used_mb / g.mem_total_mb) * 100,
                          '#4ade80',
                        )
                      "
                      rail-color="var(--ashell-hover)"
                    />
                  </div>
                  <span class="gpu-row-value mono">
                    {{ formatMB(g.mem_used_mb) }} /
                    {{ formatMB(g.mem_total_mb) }}
                  </span>
                </div>
                <div v-if="g.power_watts != null" class="gpu-row">
                  <span class="gpu-row-label">{{ t("hostInfo.gpuPower") }}</span>
                  <div class="gpu-row-bar">
                    <NProgress
                      v-if="g.power_limit_watts != null"
                      type="line"
                      :percentage="
                        clamp((g.power_watts / g.power_limit_watts) * 100)
                      "
                      :show-indicator="false"
                      :height="6"
                      :color="
                        gaugeColor(
                          (g.power_watts / g.power_limit_watts) * 100,
                          '#f59e0b',
                        )
                      "
                      rail-color="var(--ashell-hover)"
                    />
                  </div>
                  <span class="gpu-row-value mono">
                    {{ formatWatts(g.power_watts) }} W<template
                      v-if="g.power_limit_watts != null"
                    >
                      / {{ formatWatts(g.power_limit_watts) }} W</template
                    >
                  </span>
                </div>
                <div v-if="g.fan_percent != null" class="gpu-row">
                  <span class="gpu-row-label">{{ t("hostInfo.gpuFan") }}</span>
                  <div class="gpu-row-bar">
                    <NProgress
                      type="line"
                      :percentage="clamp(g.fan_percent)"
                      :show-indicator="false"
                      :height="6"
                      color="#4a8cff"
                      rail-color="var(--ashell-hover)"
                    />
                  </div>
                  <span class="gpu-row-value mono">
                    {{ Math.round(g.fan_percent) }}%<template v-if="g.fan_rpm">
                      · {{ g.fan_rpm }} RPM</template
                    >
                  </span>
                </div>
              </div>
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

/* 独立窗口模式：面板铺满窗口（顶部让位给窗口标题栏），无侧边阴影 */
.hostinfo-panel.standalone {
  left: 0;
  right: 0;
  width: auto;
  border-left: none;
}

.hostinfo-panel.standalone.open {
  box-shadow: none;
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

.error-alert-body {
  display: flex;
  align-items: center;
  gap: 10px;
}

.error-alert-text {
  flex: 1;
  min-width: 0;
  word-break: break-all;
}

.error-alert-hint {
  margin-top: 4px;
  font-size: 12px;
  opacity: 0.75;
  line-height: 1.5;
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

/* 温度芯片 */
.temp-block {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: -4px;
}

.temp-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 100%;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  line-height: 1.6;
  border: 1px solid var(--ashell-border-soft);
  color: var(--ashell-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.temp-chip b {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.temp-chip.temp-cool b {
  color: var(--ashell-text);
}

.temp-chip.temp-warm {
  border-color: rgba(245, 158, 11, 0.45);
  color: #f59e0b;
}

.temp-chip.temp-warm b {
  color: #f59e0b;
}

.temp-chip.temp-hot {
  border-color: rgba(239, 68, 68, 0.55);
  background: rgba(239, 68, 68, 0.12);
  color: #ef4444;
}

.temp-chip.temp-hot b {
  color: #ef4444;
}

/* 显卡卡片 */
.gpu-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
}

.gpu-item + .gpu-item {
  margin-top: 8px;
}

.gpu-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.gpu-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.gpu-index {
  display: inline-block;
  margin-right: 5px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--ashell-hover);
  color: var(--ashell-text-muted);
  font-size: 10px;
  font-weight: 600;
  line-height: 1.6;
  vertical-align: 1px;
}

.gpu-temp {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  padding: 1px 7px;
  border-radius: 999px;
}

.gpu-temp.temp-cool {
  color: var(--ashell-text-muted);
}

.gpu-temp.temp-warm {
  color: #f59e0b;
}

.gpu-temp.temp-hot {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.12);
}

.gpu-head-tags {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

/* 昇腾健康态徽标：仅 Warning / Error 时出现 */
.gpu-health {
  font-size: 10px;
  font-weight: 600;
  padding: 1px 7px;
  border-radius: 999px;
}

.gpu-health.health-warn {
  color: #f59e0b;
  background: rgba(245, 158, 11, 0.12);
}

.gpu-health.health-error {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.12);
}

/* 头部属性徽章：性能状态 / PCIe 链路等非计量文本 */
.gpu-chip {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--ashell-text-muted);
  background: var(--ashell-hover);
  padding: 1px 6px;
  border-radius: 999px;
  font-variant-numeric: tabular-nums;
}

.gpu-row {
  display: grid;
  grid-template-columns: 42px 1fr 110px;
  align-items: center;
  gap: 8px;
}

.gpu-row-label {
  font-size: 11px;
  color: var(--ashell-text-muted);
  flex-shrink: 0;
}

.gpu-row-bar {
  min-width: 0;
}

.gpu-row-value {
  font-size: 11px;
  color: var(--ashell-text);
  font-variant-numeric: tabular-nums;
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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
