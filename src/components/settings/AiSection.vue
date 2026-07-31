<script setup lang="ts">
import { ref, onMounted } from "vue";
import {
  NForm,
  NFormItem,
  NInput,
  NButton,
  NSpace,
  NIcon,
  NText,
  NTag,
  useMessage,
} from "naive-ui";
import {
  SearchOutline,
  CheckmarkCircleOutline,
  CloseCircleOutline,
} from "@vicons/ionicons5";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

const { t } = useI18n();
const message = useMessage();

const claudePath = ref("");
const detectedPath = ref<string | null>(null);
const detecting = ref(false);
const detectionAttempted = ref(false);

async function loadClaudePath() {
  try {
    const config = await invoke<{ claudePath: string }>("read_ai_paths");
    claudePath.value = config.claudePath;
  } catch {
    // ignore
  }
}

async function detectClaude() {
  detectionAttempted.value = true;
  detecting.value = true;
  try {
    detectedPath.value = await invoke<string | null>("detect_claude_path");
  } catch {
    detectedPath.value = null;
  } finally {
    detecting.value = false;
  }
}

async function applyDetectedPath() {
  if (detectedPath.value) {
    claudePath.value = detectedPath.value;
    await saveClaudePath();
  }
}

async function saveClaudePath() {
  try {
    await invoke("write_ai_paths", {
      config: { sidecarPath: "", claudePath: claudePath.value.trim() },
    });
  } catch (e) {
    message.error(t("settings.ai.saveFailed", { error: String(e) }));
  }
}

onMounted(async () => {
  await loadClaudePath();
});
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.ai.claudePath") }}</div>
    <NInput
      v-model:value="claudePath"
      :placeholder="t('settings.ai.claudePathPlaceholder')"
      clearable
      @blur="saveClaudePath"
    />
    <NSpace align="center" :size="8" style="margin-top: 8px">
      <NButton size="small" :loading="detecting" @click="detectClaude">
        <template #icon>
          <NIcon><SearchOutline /></NIcon>
        </template>
        {{ t("settings.ai.detect") }}
      </NButton>
      <template v-if="detectedPath !== null">
        <NTag size="small" type="success" :bordered="false">
          <template #icon>
            <NIcon><CheckmarkCircleOutline /></NIcon>
          </template>
          {{ detectedPath }}
        </NTag>
        <NButton size="tiny" quaternary type="primary" @click="applyDetectedPath">
          {{ t("settings.ai.applyDetected") }}
        </NButton>
      </template>
      <template v-else-if="detectionAttempted && !detecting">
        <NTag size="small" type="warning" :bordered="false">
          <template #icon>
            <NIcon><CloseCircleOutline /></NIcon>
          </template>
          {{ t("settings.ai.notDetected") }}
        </NTag>
      </template>
    </NSpace>
    <p class="field-hint">{{ t("settings.ai.claudePathHint") }}</p>

    <NText depth="3" style="font-size: 12px; line-height: 1.6; margin-top: 12px">
      {{ t("settings.ai.configFileHint") }}
    </NText>
  </section>
</template>

<style scoped>
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.settings-section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
}

.field-hint {
  margin: -4px 0 8px;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.5;
}
</style>
