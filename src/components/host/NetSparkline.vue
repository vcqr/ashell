<script setup lang="ts">
import { computed } from "vue"
import { humanRate } from "@/utils/humanRate"

interface Props {
  /** 接收速率序列（B/s），最新点在末尾 */
  rxSeries: number[]
  /** 发送速率序列（B/s），最新点在末尾 */
  txSeries: number[]
  /** 图高（px），默认 96 */
  height?: number
  /** 接收线颜色，默认 ashell primary */
  rxColor?: string
  /** 发送线颜色 */
  txColor?: string
}

const props = withDefaults(defineProps<Props>(), {
  height: 96,
  rxColor: "#7c5cff",
  txColor: "#4a8cff",
})

/** SVG 内部坐标系宽度（绘图区，不含 Y 轴标签） */
const VIEW_W = 200

/** 把任意正数向上对齐到 1/2/5 × 10^n 的"友好"刻度 */
function niceCeil(v: number): number {
  if (!Number.isFinite(v) || v <= 0) return 1
  const exp = Math.floor(Math.log10(v))
  const base = Math.pow(10, exp)
  const f = v / base
  let nf: number
  if (f <= 1) nf = 1
  else if (f <= 2) nf = 2
  else if (f <= 5) nf = 5
  else nf = 10
  return nf * base
}

const yMax = computed(() => {
  const all = [...props.rxSeries, ...props.txSeries]
  const m = all.reduce((acc, v) => (v > acc ? v : acc), 0)
  if (m <= 0) return 1024 // 1 KB/s 的占位刻度，避免空图全压底
  return niceCeil(m * 1.15)
})

/** 4 等分得到 5 条横线 + 5 个标签：max、3/4、1/2、1/4、0 */
const ticks = computed(() => {
  const max = yMax.value
  const out: { value: number; topPct: number; label: string }[] = []
  const STEPS = 4
  for (let i = 0; i <= STEPS; i++) {
    const v = (max * (STEPS - i)) / STEPS
    const topPct = (i / STEPS) * 100
    out.push({
      value: v,
      topPct,
      label: i === STEPS ? "0" : humanRate(v),
    })
  }
  return out
})

function buildPoints(series: number[]): string {
  if (series.length === 0) return ""
  if (series.length === 1) {
    const y = props.height - (series[0]! / yMax.value) * props.height
    return `0,${y} ${VIEW_W},${y}`
  }
  const step = VIEW_W / (series.length - 1)
  return series
    .map((v, i) => {
      const x = i * step
      const y = props.height - (v / yMax.value) * props.height
      return `${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(" ")
}

function buildArea(series: number[]): string {
  if (series.length === 0) return ""
  const step = series.length === 1 ? VIEW_W : VIEW_W / (series.length - 1)
  const pts = series
    .map((v, i) => {
      const x = i * step
      const y = props.height - (v / yMax.value) * props.height
      return `${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(" L")
  return `M0,${props.height} L${pts} L${VIEW_W},${props.height} Z`
}

const rxPoints = computed(() => buildPoints(props.rxSeries))
const txPoints = computed(() => buildPoints(props.txSeries))
const rxArea = computed(() => buildArea(props.rxSeries))
const txArea = computed(() => buildArea(props.txSeries))
</script>

<template>
  <div class="sparkline-wrap" :style="{ height: `${height}px` }">
    <!-- Y 轴标签（HTML 渲染，不被 SVG 拉伸） -->
    <div class="y-axis">
      <span
        v-for="(t, i) in ticks"
        :key="i"
        class="y-tick"
        :style="{ top: `${t.topPct}%` }"
      >
        {{ t.label }}
      </span>
    </div>

    <!-- 绘图区 SVG -->
    <svg
      class="net-sparkline"
      :viewBox="`0 0 ${VIEW_W} ${height}`"
      preserveAspectRatio="none"
    >
      <!-- 横向网格 -->
      <line
        v-for="(t, i) in ticks"
        :key="`g${i}`"
        class="grid"
        :class="{ 'grid-base': i === ticks.length - 1 }"
        x1="0"
        :y1="(t.topPct / 100) * height"
        :x2="VIEW_W"
        :y2="(t.topPct / 100) * height"
      />

      <path :d="rxArea" :fill="rxColor" fill-opacity="0.14" />
      <polyline
        :points="rxPoints"
        fill="none"
        :stroke="rxColor"
        stroke-width="1.5"
        stroke-linejoin="round"
        stroke-linecap="round"
        vector-effect="non-scaling-stroke"
      />

      <path :d="txArea" :fill="txColor" fill-opacity="0.10" />
      <polyline
        :points="txPoints"
        fill="none"
        :stroke="txColor"
        stroke-width="1.5"
        stroke-linejoin="round"
        stroke-linecap="round"
        vector-effect="non-scaling-stroke"
        stroke-dasharray="3 2"
      />
    </svg>
  </div>
</template>

<style scoped>
.sparkline-wrap {
  position: relative;
  width: 100%;
  display: flex;
}

.y-axis {
  position: relative;
  width: 48px;
  flex-shrink: 0;
  height: 100%;
}

.y-tick {
  position: absolute;
  right: 6px;
  transform: translateY(-50%);
  font-size: 10px;
  line-height: 1;
  color: var(--ashell-text-subtle, rgba(120, 120, 120, 0.7));
  font-family: var(--n-font-family-mono);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

/* 第一条（max）和最后一条（0）紧贴边缘时挪进来一点，避免被裁 */
.y-tick:first-child {
  transform: translateY(0);
}
.y-tick:last-child {
  transform: translateY(-100%);
}

.net-sparkline {
  flex: 1 1 auto;
  height: 100%;
  display: block;
}

.grid {
  stroke: var(--ashell-border-soft, rgba(255, 255, 255, 0.08));
  stroke-width: 1;
  stroke-dasharray: 2 3;
  vector-effect: non-scaling-stroke;
}
.grid-base {
  stroke-dasharray: 0;
  stroke: var(--ashell-border, rgba(255, 255, 255, 0.14));
}
</style>
