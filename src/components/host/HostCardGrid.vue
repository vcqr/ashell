<script setup lang="ts">
import { NCheckbox, NIcon } from "naive-ui"
import { TerminalOutline } from "@vicons/ionicons5"
import { PushpinFilled } from "@vicons/antd"
import { computed } from "vue"
import type { HostNode } from "@/types"
import { useHostStore } from "@/stores/hosts"
import { useIconStore } from "@/stores/icons"
import { hostAddrTextOfNode, splitHighlightSegments } from "@/utils/hosts-view"

const props = defineProps<{
  /** 平铺主机列表（置顶在前 + 当前排序模式），key 约定与树一致（host-{id}） */
  hosts: HostNode[]
  selectedKeys: string[]
  checkedKeys: string[]
  batchMode: boolean
  filter: string
}>()

const emit = defineEmits<{
  select: [key: string]
  open: [node: HostNode]
  "open-new": [node: HostNode]
  menu: [key: string, x: number, y: number]
  "toggle-check": [key: string]
}>()

const hostStore = useHostStore()
const iconStore = useIconStore()

const DEFAULT_COLOR = "#7c5cff"

const nodes = computed(() => props.hosts)

function colorOf(node: HostNode): string {
  return node.color || DEFAULT_COLOR
}

function iconUrlOf(node: HostNode): string | null {
  return node.icon ? iconStore.urlOf(node.icon) : null
}

/** 卡片地址行:root@addr:port（serial 无用户名语义，仅地址） */
function hostAddrLine(node: HostNode): string {
  const addr = hostAddrTextOfNode(node)
  if (!addr || node.protocol === "serial") return addr
  const u = node.username?.trim()
  return u ? `${u}@${addr}` : addr
}

function isPinned(node: HostNode): boolean {
  return hostStore.isHostPinned(node.id)
}

function isChecked(node: HostNode): boolean {
  return props.checkedKeys.includes(node.key)
}

function isSelected(node: HostNode): boolean {
  return props.selectedKeys.includes(node.key)
}

function highlightSegments(label: string) {
  return splitHighlightSegments(label, props.filter.trim())
}

function onAuxclick(e: MouseEvent, node: HostNode) {
  if (e.button !== 1) return
  e.preventDefault()
  emit("open-new", node)
}

function onMousedown(e: MouseEvent) {
  // 吞掉中键默认行为（自动滚动 / Linux 中键粘贴）
  if (e.button === 1) e.preventDefault()
}
</script>

<template>
  <div class="host-card-grid" tabindex="-1">
    <div
      v-for="node in nodes"
      :key="node.key"
      class="host-card"
      :class="{ selected: isSelected(node), checked: batchMode && isChecked(node) }"
      :style="node.color ? ({ '--host-color': node.color } as Record<string, string>) : undefined"
      tabindex="0"
      role="button"
      :aria-checked="batchMode ? isChecked(node) : undefined"
      :data-node-key="node.key"
      :title="node.desc || hostAddrLine(node) || undefined"
      @click="emit('select', node.key)"
      @dblclick="emit('open', node)"
      @auxclick="onAuxclick($event, node)"
      @mousedown="onMousedown"
      @contextmenu.prevent.stop="emit('menu', node.key, $event.clientX, $event.clientY)"
    >
      <span
        class="host-avatar"
        :style="{ background: `color-mix(in srgb, ${colorOf(node)} 14%, transparent)` }"
      >
        <img v-if="iconUrlOf(node)" :src="iconUrlOf(node)!" alt="" draggable="false" />
        <NIcon v-else :size="20" :color="colorOf(node)"><TerminalOutline /></NIcon>
      </span>
      <span class="host-meta">
        <span class="host-name">
          <template v-for="(seg, i) in highlightSegments(node.label)" :key="i">
            <span v-if="seg.hit" class="node-label-hit">{{ seg.text }}</span>
            <template v-else>{{ seg.text }}</template>
          </template>
        </span>
        <span class="host-addr">
          <span class="host-addr-text">{{ hostAddrLine(node) }}</span>
          <span v-if="node.protocol && node.protocol !== 'ssh'" class="host-proto">
            {{ node.protocol }}
          </span>
        </span>
      </span>
      <NIcon
        v-if="isPinned(node) && !batchMode"
        class="host-pin"
        :size="12"
        color="#7c5cff"
      >
        <PushpinFilled />
      </NIcon>
      <NCheckbox
        v-else-if="batchMode"
        class="host-check"
        :checked="isChecked(node)"
        @update:checked="() => emit('toggle-check', node.key)"
        @click.stop
      />
    </div>
  </div>
</template>

<style scoped>
.host-card-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 8px;
  padding: 4px;
  outline: none;
}

.host-card {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 8px 9px;
  border-radius: 10px;
  background: var(--ashell-panel-bg-soft);
  border: 1px solid var(--ashell-border-soft);
  cursor: default;
  outline: none;
  /* 裁剪扫光伪元素:不裁剪时会停在左/右邻卡上方,悬停任一卡扫光会越过邻居 */
  overflow: hidden;
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease,
    transform 0.15s ease;
}

/* 悬停扫光：一层斜向高光从左划到右（reduced-motion 下禁用）。
   transition 只挂在 :hover 上——移出时无过渡、瞬时归位,避免反向扫回 */
.host-card::before {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: linear-gradient(
    115deg,
    transparent 32%,
    rgba(255, 255, 255, 0.07) 50%,
    transparent 68%
  );
  transform: translateX(-130%);
  pointer-events: none;
}

/* 悬停/选中与 TabBar 同语言：色罩以 background-image 叠加在面板底色上
   （同 tab 的 18% 色罩渲染方式），直接改 background-color 会把面板底色替换掉，
   深色主题下卡片反而变暗 */
.host-card:hover {
  /* 卡片面积大,6% 罩在大表面不明显,叠两层 ≈12% 仍走主题变量 */
  background-image:
    linear-gradient(var(--ashell-hover), var(--ashell-hover)),
    linear-gradient(var(--ashell-hover), var(--ashell-hover));
  transform: translateY(-2px);
  /* 光晕跟随主机色,与头像块/选中描边同一身份色 */
  box-shadow: 0 6px 18px
    color-mix(in srgb, var(--host-color, var(--ashell-primary)) 22%, transparent);
}

.host-card:hover::before {
  transform: translateX(130%);
  transition: transform 0.45s ease;
}

.host-card:focus-visible {
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--ashell-primary) 60%, transparent);
}

.host-card.selected {
  border-color: var(--host-color, var(--ashell-primary));
  background-image: linear-gradient(
    color-mix(in srgb, var(--host-color, var(--ashell-primary)) 18%, transparent),
    color-mix(in srgb, var(--host-color, var(--ashell-primary)) 18%, transparent)
  );
  box-shadow: inset 0 -2px 0 var(--host-color, var(--ashell-primary));
}

/* 选中 + 悬停：底部色条与光晕并存（box-shadow 不跨规则合并，需显式组合） */
.host-card.selected:hover {
  box-shadow:
    inset 0 -2px 0 var(--host-color, var(--ashell-primary)),
    0 6px 18px
      color-mix(in srgb, var(--host-color, var(--ashell-primary)) 22%, transparent);
}

.host-card.checked {
  border-color: var(--ashell-primary);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--ashell-primary) 40%, transparent);
}

/* 头像块：用户色淡底承载自定义图标 / 兜底终端图标 */
.host-avatar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 8px;
  overflow: hidden;
  transition: transform 0.18s ease;
}

.host-card:hover .host-avatar {
  transform: scale(1.1);
}

@media (prefers-reduced-motion: reduce) {
  .host-card,
  .host-card::before,
  .host-avatar {
    transition: none;
  }
  .host-card:hover {
    transform: none;
  }
  .host-card:hover::before {
    display: none;
  }
  .host-card:hover .host-avatar {
    transform: none;
  }
}

.host-avatar img {
  width: 22px;
  height: 22px;
  object-fit: contain;
  pointer-events: none;
}

.host-meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.host-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--ashell-text-strong);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.host-addr {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  font-size: 12px;
  color: var(--ashell-text-muted);
}

.host-addr-text {
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.host-proto {
  flex-shrink: 0;
  font-size: 10px;
  line-height: 1;
  padding: 2px 4px;
  border-radius: 4px;
  color: var(--ashell-text-muted);
  background: var(--ashell-hover);
}

.host-pin {
  position: absolute;
  top: 6px;
  right: 7px;
}

.host-check {
  position: absolute;
  top: 6px;
  right: 7px;
}
</style>
