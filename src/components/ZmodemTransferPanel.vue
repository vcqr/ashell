<script setup lang="ts">
import { RiCloseLine, RiDownloadLine, RiUploadLine } from "@remixicon/vue"
import { computed, type Ref } from "vue"
import { NButton, NIcon, NProgress, NTag, NTooltip } from "naive-ui"
import { useI18n } from "vue-i18n"

import type { ZmodemTransferItem } from "@/composables/useTerminalZmodem"
import { humanSize } from "@/utils/humanSize"
import { progressColor } from "@/utils/progressColor"
import { useTransferSpeed } from "@/composables/useTransferSpeed"

interface Props {
  items: ZmodemTransferItem[]
  /** 会话进行中(还有可取消的传输) */
  busy: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  cancel: []
  clear: []
}>()

const { t } = useI18n()

/** 速度采样按 string id 工作,这里做一层映射视图 */
const speedView = computed(() =>
  props.items.map((it) => ({ id: String(it.id), status: it.status, loaded: it.loaded })),
) as Ref<{ id: string; status: string; loaded: number }[]>
const { speeds } = useTransferSpeed(speedView)

const hasRunning = computed(() => props.items.some((it) => it.status === "running"))

function percent(it: ZmodemTransferItem): number {
  if (!it.size || it.size <= 0) return it.status === "done" ? 100 : 0
  return Math.min(100, Math.floor((it.loaded / it.size) * 100))
}

function sizeText(it: ZmodemTransferItem): string {
  return it.size > 0
    ? `${humanSize(it.loaded)} / ${humanSize(it.size)}`
    : humanSize(it.loaded)
}

function speedText(it: ZmodemTransferItem): string {
  const bps = speeds.value[String(it.id)]
  if (!bps || bps <= 0) return ""
  return `${humanSize(bps)}/s`
}

function statusType(s: ZmodemTransferItem["status"]): "default" | "info" | "success" | "warning" | "error" {
  switch (s) {
    case "running":
      return "info"
    case "done":
      return "success"
    case "error":
      return "error"
    case "cancelled":
      return "warning"
    default:
      return "default"
  }
}

function statusLabel(it: ZmodemTransferItem): string {
  const key = it.direction === "upload" ? "uploading" : "downloading"
  switch (it.status) {
    case "running":
      return t(`terminal.zmodem.${key}`)
    case "done":
      return t("common.status.completed")
    case "error":
      return t("common.status.failed")
    case "cancelled":
      return t("common.status.cancelled")
    default:
      return it.status
  }
}

function barColor(it: ZmodemTransferItem): string | undefined {
  if (it.status === "running") return progressColor(percent(it))
  return undefined
}
</script>

<template>
  <Transition name="zmodem-fade">
    <div v-if="items.length > 0" class="zmodem-panel">
      <div class="panel-head">
        <span class="panel-title">{{ t('terminal.zmodem.panelTitle') }}</span>
        <NTooltip v-if="hasRunning && busy" trigger="hover" placement="top">
          <template #trigger>
            <NButton size="tiny" quaternary type="error" @click="emit('cancel')">
              {{ t('terminal.zmodem.cancelAll') }}
            </NButton>
          </template>
          {{ t('terminal.zmodem.cancelAllTip') }}
        </NTooltip>
        <NButton v-else size="tiny" quaternary @click="emit('clear')">
          <template #icon>
            <NIcon :size="12"><RiCloseLine /></NIcon>
          </template>
          {{ t('terminal.zmodem.close') }}
        </NButton>
      </div>
      <div class="panel-list">
        <div v-for="it in items" :key="it.id" class="transfer-item">
          <div class="row">
            <NIcon :size="13" class="dir-icon" :class="it.direction">
              <RiUploadLine v-if="it.direction === 'upload'" />
              <RiDownloadLine v-else />
            </NIcon>
            <div class="name" :title="it.name">{{ it.name }}</div>
            <NTag size="small" :type="statusType(it.status)" :bordered="false">
              {{ statusLabel(it) }}
            </NTag>
          </div>
          <NProgress
            :percentage="percent(it)"
            :status="it.status === 'error' ? 'error' : it.status === 'done' ? 'success' : 'default'"
            :color="barColor(it)"
            :height="5"
            :show-indicator="false"
          />
          <div class="meta">
            <span>
              {{ sizeText(it) }}
              <span class="pct">{{ percent(it) }}%</span>
            </span>
            <span v-if="it.status === 'running' && speedText(it)" class="speed">{{ speedText(it) }}</span>
          </div>
          <div v-if="it.error" class="err">{{ it.error }}</div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.zmodem-panel {
  position: absolute;
  right: 14px;
  bottom: 14px;
  z-index: 30;
  width: 320px;
  max-height: 42vh;
  display: flex;
  flex-direction: column;
  background: var(--ashell-bg);
  border: 1px solid var(--ashell-border-soft);
  border-radius: 10px;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18);
  overflow: hidden;
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  border-bottom: 1px solid var(--ashell-border-soft);
}

.panel-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-strong);
}

.panel-list {
  padding: 8px 10px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.transfer-item {
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
  padding: 6px 8px;
  background: var(--ashell-bg);
}

.row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 5px;
}

.dir-icon {
  flex: none;
}

.dir-icon.upload {
  color: var(--ashell-success);
}

.dir-icon.download {
  color: var(--ashell-primary, #7c66dc);
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--ashell-text-strong);
  font-family: var(--n-font-family-mono, monospace);
}

.meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 4px;
  font-size: 11px;
  color: var(--ashell-text-subtle);
  font-variant-numeric: tabular-nums;
}

.speed {
  color: var(--ashell-text-muted);
}

.pct {
  margin-left: 6px;
  color: var(--ashell-text-strong);
}

.err {
  margin-top: 4px;
  color: var(--ashell-danger);
  font-size: 11px;
  word-break: break-all;
}

.zmodem-fade-enter-active,
.zmodem-fade-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}

.zmodem-fade-enter-from,
.zmodem-fade-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
</style>
