<script setup lang="ts">
import { NModal, NCard, NButton, NIcon } from "naive-ui";
import { CloseOutline } from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import AiProvidersSection from "./settings/AiProvidersSection.vue";

/**
 * 模型供应商管理弹窗（独立 AI 窗口用）。
 *
 * 主体在 AiProvidersSection：主窗口已把供应商管理收进设置弹窗，此处仅保留
 * 弹窗形态给没有设置入口的 AI 独立窗口（引导卡片「添加供应商」）。
 */
defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
}>();

const { t } = useI18n();
</script>

<template>
  <NModal :show="open" :mask-closable="false" @update:show="(v: boolean) => emit('update:open', v)">
    <NCard
      style="width: min(760px, 92vw); max-height: min(660px, 88vh)"
      :title="t('settings.ai.provider.title')"
      size="medium"
      :bordered="false"
      class="providers-card"
      role="dialog"
      aria-modal="true"
    >
      <template #header-extra>
        <NButton quaternary circle size="small" :title="t('settings.close')" @click="emit('update:open', false)">
          <template #icon><NIcon><CloseOutline /></NIcon></template>
        </NButton>
      </template>

      <AiProvidersSection />
    </NCard>
  </NModal>
</template>

<style>
.providers-card .n-card-content {
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
