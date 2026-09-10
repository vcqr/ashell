<script setup lang="ts">
import { computed } from "vue"
import { NButton, NModal } from "naive-ui"
import { useI18n } from "vue-i18n"

/**
 * SSH 主机密钥指纹确认弹窗（TOFU）。
 * 首次连接展示指纹请求信任；指纹变更时红色强警告并对比新旧指纹。
 */
export interface HostKeyInfo {
  /** addr:port */
  label: string
  keyType: string
  fingerprint: string
  /** 已保存指纹；非空即指纹变更场景 */
  previous: string | null
}

const props = defineProps<{
  show: boolean
  info: HostKeyInfo
}>()

const emit = defineEmits<{
  "update:show": [value: boolean]
  "trust": []
  "decline": []
}>()

const { t } = useI18n()

const changed = computed(() => !!props.info.previous)

// 只发事件不自行改 show：由父组件（TerminalView）统一关闭并回发响应，
// 避免右上角 X 关闭时 update:show 先清掉状态导致响应丢失
function decline() {
  emit("decline")
}

function trust() {
  emit("trust")
}
</script>

<template>
  <NModal
    :show="props.show"
    preset="card"
    :title="changed ? t('terminal.hostKeyChangedTitle') : t('terminal.hostKeyTitle')"
    class="hostkey-modal"
    style="max-width: 480px; width: calc(100vw - 32px)"
    :mask-closable="false"
    :bordered="false"
    size="small"
    @update:show="(v: boolean) => { if (!v) decline() }"
  >
    <div v-if="changed" class="hostkey-warning">
      {{ t("terminal.hostKeyChangedDesc", { label: props.info.label }) }}
    </div>
    <div v-else class="hostkey-desc">
      {{ t("terminal.hostKeyDesc", { label: props.info.label }) }}
    </div>

    <div class="hostkey-field">
      <span class="hostkey-field-label">{{ t("terminal.hostKeyKeyType") }}</span>
      <span class="hostkey-field-value">{{ props.info.keyType }}</span>
    </div>
    <div v-if="changed" class="hostkey-field">
      <span class="hostkey-field-label">{{ t("terminal.hostKeyPrevious") }}</span>
      <span class="hostkey-field-value mono old">{{ props.info.previous }}</span>
    </div>
    <div class="hostkey-field">
      <span class="hostkey-field-label">
        {{ changed ? t("terminal.hostKeyNewFingerprint") : t("terminal.hostKeyFingerprint") }}
      </span>
      <span class="hostkey-field-value mono" :class="{ danger: changed }">
        {{ props.info.fingerprint }}
      </span>
    </div>

    <template #footer>
      <div class="hostkey-actions">
        <NButton size="small" @click="decline">
          {{ t("common.cancel") }}
        </NButton>
        <NButton size="small" :type="changed ? 'error' : 'primary'" @click="trust">
          {{ changed ? t("terminal.hostKeyTrustChanged") : t("terminal.hostKeyTrust") }}
        </NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.hostkey-desc {
  font-size: 13px;
  line-height: 1.6;
  color: var(--ashell-text);
  margin-bottom: 12px;
}

.hostkey-warning {
  font-size: 13px;
  line-height: 1.6;
  color: #d03050;
  background: rgba(208, 48, 80, 0.08);
  border: 1px solid rgba(208, 48, 80, 0.3);
  border-radius: 6px;
  padding: 8px 12px;
  margin-bottom: 12px;
}

.hostkey-field {
  display: flex;
  gap: 10px;
  align-items: baseline;
  font-size: 12px;
  margin-bottom: 8px;
}

.hostkey-field-label {
  flex: 0 0 auto;
  color: var(--ashell-text-muted, #98a2b3);
}

.hostkey-field-value {
  word-break: break-all;
  color: var(--ashell-text);
}

.hostkey-field-value.mono {
  font-family: var(--ashell-mono, monospace);
}

.hostkey-field-value.old {
  text-decoration: line-through;
  opacity: 0.7;
}

.hostkey-field-value.danger {
  color: #d03050;
}

.hostkey-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
