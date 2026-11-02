<script setup lang="ts">
import { ref } from "vue"
import { NButton, NCheckbox, NModal } from "naive-ui"
import { useI18n } from "vue-i18n"

/**
 * 退出确认弹窗：关闭策略为"退出应用"时，仍有活动终端连接的退出路径
 * （点关闭按钮 / Cmd+Q / Dock 退出）由后端拦截后发 tray-quit-confirm
 * 事件触发本弹窗，仅挂在主窗口。
 *
 * 只发事件不自行改 show：由父组件（App.vue）统一关闭并回发响应，
 * 与 HostKeyConfirmModal 同一约定，避免关窗时序导致响应丢失。
 */
const props = defineProps<{
  show: boolean
  /** 活动连接数（所有窗口求和），用于文案 */
  count: number
  /** 托盘是否启用：未启用时没有托盘图标，不提供"最小化到托盘"出口 */
  canHide: boolean
}>()

const emit = defineEmits<{
  "update:show": [value: boolean]
  resolve: [action: "quit" | "hide" | "cancel", remember: boolean]
}>()

const { t } = useI18n()

const remember = ref(false)

function close(action: "quit" | "hide" | "cancel") {
  emit("update:show", false)
  emit("resolve", action, remember.value)
  remember.value = false
}
</script>

<template>
  <NModal
    :show="props.show"
    preset="card"
    :title="t('app.quitConfirm.title')"
    class="quit-confirm-modal"
    style="max-width: 420px; width: calc(100vw - 32px)"
    :mask-closable="false"
    :bordered="false"
    size="small"
    @update:show="(v: boolean) => { if (!v) close('cancel') }"
  >
    <div class="quit-confirm-desc">
      {{ t("app.quitConfirm.desc", { count: props.count }) }}
    </div>
    <NCheckbox v-model:checked="remember" size="small" class="quit-confirm-remember">
      {{ t("app.quitConfirm.remember") }}
    </NCheckbox>
    <template #footer>
      <div class="quit-confirm-actions">
        <NButton quaternary @click="close('cancel')">
          {{ t("app.quitConfirm.cancel") }}
        </NButton>
        <NButton v-if="props.canHide" secondary @click="close('hide')">
          {{ t("app.quitConfirm.hide") }}
        </NButton>
        <NButton type="error" secondary @click="close('quit')">
          {{ t("app.quitConfirm.quit") }}
        </NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.quit-confirm-desc {
  color: var(--ashell-text, inherit);
  line-height: 1.7;
}

.quit-confirm-remember {
  margin-top: 14px;
}

.quit-confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
