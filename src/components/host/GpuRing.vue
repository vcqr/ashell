<script setup lang="ts">
import { computed } from "vue"

interface Props {
  /** 0 - 100，超界自动收敛 */
  percent: number
  /** 环心短标签，如 GPU / VRAM */
  label: string
  /** 正常区间（<60%）的基础色 */
  baseColor?: string
  /** 环外径（px） */
  size?: number
}

const props = withDefaults(defineProps<Props>(), {
  baseColor: "#7c5cff",
  size: 56,
})

/** 分级配色：≥80% 红（过载告警）、≥60% 琥珀（偏高） */
const WARM_COLOR = "#f59e0b"
const HOT_COLOR = "#ef4444"

const color = computed(() => {
  if (props.percent >= 80) return HOT_COLOR
  if (props.percent >= 60) return WARM_COLOR
  return props.baseColor
})

const STROKE = 5
const radius = computed(() => (props.size - STROKE) / 2)
const circumference = computed(() => 2 * Math.PI * radius.value)
const dashOffset = computed(
  () =>
    circumference.value *
    (1 - Math.min(100, Math.max(0, props.percent)) / 100),
)
</script>

<template>
  <div class="gpu-ring" :style="{ width: `${size}px`, height: `${size}px` }">
    <svg
      :width="size"
      :height="size"
      :viewBox="`0 0 ${size} ${size}`"
      class="gpu-ring-svg"
    >
      <circle
        class="gpu-ring-rail"
        :cx="size / 2"
        :cy="size / 2"
        :r="radius"
        :stroke-width="STROKE"
      />
      <circle
        class="gpu-ring-value"
        :cx="size / 2"
        :cy="size / 2"
        :r="radius"
        :stroke-width="STROKE"
        :stroke="color"
        :stroke-dasharray="circumference"
        :stroke-dashoffset="dashOffset"
      />
    </svg>
    <div class="gpu-ring-center">
      <span class="gpu-ring-pct" :style="{ color }">
        {{ Math.round(Math.min(100, Math.max(0, percent))) }}%
      </span>
      <span class="gpu-ring-label">{{ label }}</span>
    </div>
  </div>
</template>

<style scoped>
.gpu-ring {
  position: relative;
  flex-shrink: 0;
}

.gpu-ring-svg {
  display: block;
  transform: rotate(-90deg);
}

.gpu-ring-rail {
  fill: none;
  stroke: var(--ashell-hover, rgba(128, 128, 128, 0.18));
}

.gpu-ring-value {
  fill: none;
  stroke-linecap: round;
  /* 相邻两次轮询（1.5s）之间平滑过渡，颜色跟着阈值切换渐变 */
  transition:
    stroke-dashoffset 0.6s ease,
    stroke 0.6s ease;
}

.gpu-ring-center {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1px;
  pointer-events: none;
}

.gpu-ring-pct {
  font-family: var(--n-font-family-mono, monospace);
  font-size: 13px;
  font-weight: 700;
  line-height: 1;
  font-variant-numeric: tabular-nums;
  transition: color 0.6s ease;
}

.gpu-ring-label {
  font-size: 8px;
  line-height: 1;
  letter-spacing: 0.5px;
  color: var(--ashell-text-subtle, rgba(120, 120, 120, 0.7));
  text-transform: uppercase;
}
</style>
