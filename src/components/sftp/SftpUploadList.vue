<script setup lang="ts">
import { computed } from "vue"
import { NButton, NEmpty, NIcon, NProgress, NTag } from "naive-ui"
import { useI18n } from "vue-i18n"
import { CloseCircleOutline } from "@vicons/ionicons5"
import type { TransferStatus, TransferTask } from "@/types"
import { humanSize } from "@/utils/humanSize"
import { progressColor } from "@/utils/progressColor"

interface Props {
  tasks: TransferTask[]
}

const props = defineProps<Props>()
const emit = defineEmits<{
  cancel: [id: string]
}>()

const { t } = useI18n()
const list = computed(() => props.tasks)

function percent(t: TransferTask): number {
  if (!t.total || t.total <= 0) return t.status === "done" ? 100 : 0
  return Math.min(100, Math.floor((t.loaded / t.total) * 100))
}

/** running / pending 时按百分比阶梯着色；其它状态交给 NProgress 的 status 处理 */
function barColor(t: TransferTask): string | undefined {
  if (t.status === "running" || t.status === "pending") {
    return progressColor(percent(t))
  }
  return undefined
}

function statusType(s: TransferStatus): "default" | "info" | "success" | "warning" | "error" {
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

function statusLabel(s: TransferStatus): string {
  switch (s) {
    case "pending":
      return t("common.status.pending")
    case "running":
      return t("common.status.uploading")
    case "done":
      return t("common.status.completed")
    case "error":
      return t("common.status.failed")
    case "cancelled":
      return t("common.status.cancelled")
    default:
      return s
  }
}
</script>

<template>
  <div class="transfer-list">
    <NEmpty v-if="list.length === 0" :description="t('sftp.uploadList.empty')" />
    <div v-for="task in list" :key="task.id" class="transfer-item">
      <div class="row">
        <div class="name" :title="task.filename">{{ task.filename }}</div>
        <NTag size="small" :type="statusType(task.status)" :bordered="false">
          {{ statusLabel(task.status) }}
        </NTag>
      </div>
      <NProgress
        :percentage="percent(task)"
        :status="task.status === 'error' ? 'error' : task.status === 'done' ? 'success' : 'default'"
        :color="barColor(task)"
        :height="6"
        :show-indicator="false"
      />
      <div class="meta">
        <span>
          {{ humanSize(task.loaded) }} / {{ humanSize(task.total) }}
          <span class="pct">{{ percent(task) }}%</span>
        </span>
        <NButton
          v-if="task.status === 'pending' || task.status === 'running'"
          size="tiny"
          quaternary
          type="error"
          @click="emit('cancel', task.id)"
        >
          <template #icon>
            <NIcon><CloseCircleOutline /></NIcon>
          </template>
          {{ t("common.cancel") }}
        </NButton>
      </div>
      <div v-if="task.error" class="err">{{ task.error }}</div>
    </div>
  </div>
</template>

<style scoped>
.transfer-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 4px 2px;
  max-height: 60vh;
  overflow-y: auto;
}

.transfer-item {
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
  padding: 8px 10px;
  background: var(--ashell-bg);
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  color: var(--ashell-text-strong);
  font-family: var(--n-font-family-mono);
}

.meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 6px;
  font-size: 12px;
  color: var(--ashell-text-subtle);
}

.pct {
  margin-left: 8px;
  color: var(--ashell-text-strong);
  font-variant-numeric: tabular-nums;
}

.err {
  margin-top: 6px;
  color: #e88080;
  font-size: 12px;
  word-break: break-all;
}
</style>
