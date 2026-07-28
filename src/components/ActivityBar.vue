<script setup lang="ts">
import { h } from "vue"
import {
  NBadge,
  NIcon,
  NPopover,
  NTooltip,
  useThemeVars,
} from "naive-ui"
import {
  MegaphoneOutline,
  FolderOpenOutline,
  HardwareChipOutline,
  SwapHorizontalOutline,
  SparklesOutline,
} from "@vicons/ionicons5"
import { useI18n } from "vue-i18n"
import { useBroadcastStore } from "@/stores/broadcast"
import BroadcastPopover from "@/components/BroadcastPopover.vue"
import type { TerminalTab } from "@/types"

const { t } = useI18n()
const props = defineProps<{
  tabs: TerminalTab[]
  activeKey: string
  sftpOpen: boolean
  hostInfoOpen: boolean
  forwardOpen: boolean
  aiOpen: boolean
  hasActiveSession: boolean
  /** AI 助手可用性：比 hasActiveSession 宽，本地 PTY tab 也算可用 */
  hasAiSession: boolean
}>()

const emit = defineEmits<{
  "toggle-sftp": []
  "toggle-host-info": []
  "toggle-forward": []
  "toggle-ai": []
}>()

const broadcastStore = useBroadcastStore()
const vars = useThemeVars()

function renderIcon(comp: unknown) {
  return () => h(NIcon, { size: 20 }, { default: () => h(comp as never) })
}
</script>

<template>
  <aside
    class="activity-bar"
    :style="{
      '--ab-bg': vars?.bodyColor ?? 'var(--ashell-panel-bg)',
      '--ab-border': vars?.borderColor ?? 'var(--ashell-border)',
      '--ab-hover': vars?.hoverColor ?? 'var(--ashell-hover)',
    }"
  >
    <!-- 广播：点击展开配置 popover -->
    <NPopover
      trigger="click"
      placement="left-start"
      :show-arrow="false"
    >
      <template #trigger>
        <NTooltip placement="left" :show-arrow="false">
          <template #trigger>
            <button
              class="ab-btn"
              :class="{
                active:
                  broadcastStore.enabled &&
                  broadcastStore.targetKeys.size > 0,
              }"
              type="button"
            >
              <NBadge
                :value="broadcastStore.targetKeys.size"
                :show="
                  broadcastStore.enabled &&
                  broadcastStore.targetKeys.size > 0
                "
                :max="9"
                color="#f59e0b"
                processing
              >
                <component :is="renderIcon(MegaphoneOutline)" />
              </NBadge>
            </button>
          </template>
          {{
            broadcastStore.enabled && broadcastStore.targetKeys.size > 0
              ? t("terminal.activityBar.broadcasting", { count: broadcastStore.targetKeys.size })
              : t("terminal.activityBar.broadcast")
          }}
        </NTooltip>
      </template>
      <BroadcastPopover :tabs="tabs" :active-key="activeKey" />
    </NPopover>

    <div class="ab-divider" />

    <!-- SFTP -->
    <NTooltip placement="left" :show-arrow="false">
      <template #trigger>
        <button
          class="ab-btn"
          :class="{ active: sftpOpen }"
          type="button"
          :disabled="!hasActiveSession"
          @click="emit('toggle-sftp')"
        >
          <component :is="renderIcon(FolderOpenOutline)" />
        </button>
      </template>
      {{ hasActiveSession ? t("terminal.activityBar.sftp") : t("terminal.activityBar.sftpDisabled") }}
    </NTooltip>

    <!-- 主机信息 -->
    <NTooltip placement="left" :show-arrow="false">
      <template #trigger>
        <button
          class="ab-btn"
          :class="{ active: hostInfoOpen }"
          type="button"
          :disabled="!hasActiveSession"
          @click="emit('toggle-host-info')"
        >
          <component :is="renderIcon(HardwareChipOutline)" />
        </button>
      </template>
      {{ hasActiveSession ? t("terminal.activityBar.hostInfo") : t("terminal.activityBar.hostInfoDisabled") }}
    </NTooltip>

    <!-- 端口转发 -->
    <NTooltip placement="left" :show-arrow="false">
      <template #trigger>
        <button
          class="ab-btn"
          :class="{ active: forwardOpen }"
          type="button"
          :disabled="!hasActiveSession"
          @click="emit('toggle-forward')"
        >
          <component :is="renderIcon(SwapHorizontalOutline)" />
        </button>
      </template>
      {{ hasActiveSession ? t("terminal.activityBar.forward") : t("terminal.activityBar.forwardDisabled") }}
    </NTooltip>

    <!-- AI 助手 -->
    <NTooltip placement="left" :show-arrow="false">
      <template #trigger>
        <button
          class="ab-btn"
          :class="{ active: aiOpen }"
          type="button"
          :disabled="!hasAiSession"
          @click="emit('toggle-ai')"
        >
          <component :is="renderIcon(SparklesOutline)" />
        </button>
      </template>
      {{ hasAiSession ? t("terminal.activityBar.ai") : t("terminal.activityBar.aiDisabled") }}
    </NTooltip>
  </aside>
</template>

<style scoped>
.activity-bar {
  position: fixed;
  top: var(--ashell-header-h);
  right: 0;
  bottom: 0;
  width: 44px;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 8px 0;
  gap: 4px;
  background: var(--ab-bg);
  border-left: 1px solid var(--ab-border);
  z-index: 999;
  user-select: none;
}

.ab-btn {
  width: 36px;
  height: 36px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--ashell-text-muted);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  position: relative;
  flex-shrink: 0;
  transition: background 0.15s ease, color 0.15s ease;
}

.ab-btn:hover:not(:disabled) {
  background: var(--ab-hover);
  color: var(--ashell-text-strong);
}

.ab-btn:disabled {
  opacity: 0.38;
  cursor: not-allowed;
}

.ab-btn.active {
  color: var(--ashell-primary);
  background: color-mix(in srgb, var(--ashell-primary) 14%, transparent);
}

.ab-btn.active::before {
  content: "";
  position: absolute;
  left: -4px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 20px;
  border-radius: 0 2px 2px 0;
  background: var(--ashell-primary);
}

.ab-divider {
  width: 24px;
  height: 1px;
  background: var(--ab-border);
  margin: 2px 0;
  flex-shrink: 0;
}
</style>
