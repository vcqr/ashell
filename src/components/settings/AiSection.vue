<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { storeToRefs } from "pinia";
import {
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NButton,
  NSpace,
  NSwitch,
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
import { useAiConfigStore } from "@/stores/aiConfig";
import { useStartupStore } from "@/stores/startup";
import {
  sidecarTypeOptions,
  thinkingLevelOptions,
  parseModelIds,
} from "@/composables/useAiConfig";

const { t } = useI18n();
const message = useMessage();
const startupStore = useStartupStore();

// ── 引擎（sidecar）配置（共享 store）──

const aiConfig = useAiConfigStore();
const { enginesState, providers } = storeToRefs(aiConfig);
const engineSaving = computed(() => aiConfig.busy);

const currentEngine = computed(() => aiConfig.activeEngine);

const isPiEngine = computed(() => enginesState.value?.active_engine === "pi");

const currentProvider = computed(() => aiConfig.activeProvider);

const providerOptions = computed(() =>
  providers.value.map((p) => ({ label: p.name, value: p.id })),
);

const modelOptions = computed(() => {
  const list = parseModelIds(currentProvider.value?.model_ids ?? "");
  const activeId = currentEngine.value?.active_model_id;
  if (activeId && !list.includes(activeId)) {
    list.unshift(activeId);
  }
  return list.map((id) => ({ label: id, value: id }));
});

async function switchEngine(engine: string) {
  try {
    await aiConfig.switchEngine(engine);
  } catch (e) {
    message.error(t("settings.ai.saveFailed", { error: String(e) }));
  }
}

async function patchEngine(input: { provider_id?: string; active_model_id?: string; thinking_level?: string }) {
  try {
    await aiConfig.patchEngine(input);
  } catch (e) {
    message.error(t("settings.ai.saveFailed", { error: String(e) }));
  }
}

// ── Claude Code 路径 ──

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
  await Promise.all([aiConfig.load(), loadClaudePath()]);
});
</script>

<template>
  <section class="settings-section">
    <NForm label-placement="top" size="small">
      <NFormItem
        :label="t('settings.ai.enabled')"
        :feedback="t('settings.ai.enabledDesc')"
      >
        <NSwitch
          :value="startupStore.aiAssistantEnabled"
          @update:value="(v: boolean) => startupStore.setAiAssistantEnabled(v)"
        />
      </NFormItem>
    </NForm>

    <div class="settings-section-title">{{ t("settings.ai.engine.title") }}</div>
    <NForm label-placement="top" size="small">
      <NFormItem :label="t('settings.ai.engine.active')">
        <NSelect
          :value="enginesState?.active_engine ?? null"
          :options="sidecarTypeOptions"
          :loading="engineSaving"
          :disabled="!enginesState"
          @update:value="switchEngine"
        />
      </NFormItem>
      <NFormItem :label="t('settings.ai.engine.provider')">
        <NSelect
          :value="currentEngine?.provider_id ?? null"
          :options="providerOptions"
          clearable
          :loading="engineSaving"
          :disabled="!enginesState"
          :placeholder="t('settings.ai.engine.providerPlaceholder')"
          @update:value="(v: string | null) => patchEngine({ provider_id: v ?? '' })"
        />
      </NFormItem>
      <NFormItem :label="t('settings.ai.provider.activeModel')">
        <NSelect
          :value="currentEngine?.active_model_id || null"
          :options="modelOptions"
          :disabled="modelOptions.length === 0 || engineSaving"
          :placeholder="t('settings.ai.provider.activeModelPlaceholder')"
          @update:value="(v: string | null) => v && patchEngine({ active_model_id: v })"
        />
      </NFormItem>
      <NFormItem v-if="isPiEngine" :label="t('settings.ai.engine.thinkingLevel')">
        <NSelect
          :value="currentEngine?.thinking_level || 'off'"
          :options="thinkingLevelOptions"
          :disabled="engineSaving"
          @update:value="(v: string) => patchEngine({ thinking_level: v })"
        />
      </NFormItem>

      <template v-if="!isPiEngine">
        <NFormItem :label="t('settings.ai.claudePath')">
          <NInput
            v-model:value="claudePath"
            :placeholder="t('settings.ai.claudePathPlaceholder')"
            clearable
            @blur="saveClaudePath"
          />
        </NFormItem>
        <NFormItem>
          <NSpace align="center" :size="8">
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
          <template #feedback>{{ t("settings.ai.claudePathHint") }}</template>
        </NFormItem>
      </template>
    </NForm>
    <p class="field-hint">{{ t("settings.ai.engine.hint") }}</p>

    <NText v-if="!isPiEngine" depth="3" style="font-size: 12px; line-height: 1.6; margin-top: 4px">
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
