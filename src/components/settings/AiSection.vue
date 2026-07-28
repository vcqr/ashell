<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import {
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NButton,
  NSpace,
  NIcon,
  NText,
  NTag,
  NSpin,
  useMessage,
} from "naive-ui";
import {
  SearchOutline,
  CheckmarkCircleOutline,
  CloseCircleOutline,
  SaveOutline,
} from "@vicons/ionicons5";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";

const { t } = useI18n();
const message = useMessage();

// ── Claude Code path ──

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
  }
}

// ── Model config ──
//
// 配置存储在 ~/.ashell/ai/.env，由后端 read_ai_env / write_ai_env 命令读写。
// 与 AiAssistant.vue 的 Model settings 弹窗共用同一份配置，保持唯一来源。

type AiModelConfig = {
  url: string;
  key: string;
  modelIds: string;
  activeModelId: string;
  sidecarType: string;
  piProvider: string;
  piModel: string;
  piModelIds: string;
  piBaseUrl: string;
  piApiKey: string;
  piApi: string;
  piThinkingLevel: string;
};

function emptyModelConfig(): AiModelConfig {
  return {
    url: "", key: "", modelIds: "", activeModelId: "", sidecarType: "",
    piProvider: "", piModel: "", piModelIds: "", piBaseUrl: "", piApiKey: "", piApi: "", piThinkingLevel: "",
  };
}

function parseModelIds(value: string) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizeModelIds(value: string) {
  return parseModelIds(value).join(", ");
}

function resolveActiveModelId(modelIds: string, activeModelId: string) {
  const list = parseModelIds(modelIds);
  return list.includes(activeModelId) ? activeModelId : (list[0] ?? "");
}

const modelConfig = ref<AiModelConfig>(emptyModelConfig());
const draftConfig = ref<AiModelConfig>(emptyModelConfig());
const modelLoading = ref(false);
const modelSaving = ref(false);

const modelOptions = computed(() =>
  parseModelIds(draftConfig.value.modelIds).map((id) => ({
    label: id,
    value: id,
  })),
);

const sidecarTypeOptions = computed(() => [
  { label: "Claude Agent SDK", value: "claude" },
  { label: "Pi Coding Agent", value: "pi" },
]);

const piApiOptions = computed(() => [
  { label: "OpenAI Completions", value: "openai-completions" },
  { label: "Anthropic Messages", value: "anthropic-messages" },
  { label: "OpenAI Responses", value: "openai-responses" },
  { label: "Google Generative AI", value: "google-generative-ai" },
]);

const piThinkingLevelOptions = computed(() => [
  { label: "Off", value: "off" },
  { label: "Minimal", value: "minimal" },
  { label: "Low", value: "low" },
  { label: "Medium", value: "medium" },
  { label: "High", value: "high" },
  { label: "XHigh", value: "xhigh" },
  { label: "Max", value: "max" },
]);

const piModelOptions = computed(() =>
  parseModelIds(draftConfig.value.piModelIds).map((id) => ({
    label: id,
    value: id,
  })),
);

const isPiDraft = computed(() => draftConfig.value.sidecarType === "pi");

const sidecarTypeChanged = computed(
  () => draftConfig.value.sidecarType !== modelConfig.value.sidecarType,
);

function syncDraftActiveModel() {
  draftConfig.value.activeModelId = resolveActiveModelId(
    draftConfig.value.modelIds,
    draftConfig.value.activeModelId,
  );
}

function syncDraftPiModel() {
  draftConfig.value.piModel = resolveActiveModelId(
    draftConfig.value.piModelIds,
    draftConfig.value.piModel,
  );
}

async function loadModelConfig() {
  modelLoading.value = true;
  try {
    const raw = await invoke<AiModelConfig>("read_ai_env");
    const cfg: AiModelConfig = {
      url: raw.url ?? "",
      key: raw.key ?? "",
      modelIds: normalizeModelIds(raw.modelIds ?? ""),
      activeModelId: raw.activeModelId ?? "",
      sidecarType: raw.sidecarType ?? "",
      piProvider: raw.piProvider ?? "",
      piModel: raw.piModel ?? "",
      piModelIds: raw.piModelIds ?? "",
      piBaseUrl: raw.piBaseUrl ?? "",
      piApiKey: raw.piApiKey ?? "",
      piApi: raw.piApi ?? "",
      piThinkingLevel: raw.piThinkingLevel ?? "",
    };
    modelConfig.value = cfg;
    draftConfig.value = { ...cfg };
  } catch (e) {
    message.error(t("settings.ai.loadFailed", { error: String(e) }));
  } finally {
    modelLoading.value = false;
  }
}

async function saveModelConfig() {
  if (modelSaving.value) return;
  const modelIds = normalizeModelIds(draftConfig.value.modelIds);
  const piModelIds = normalizeModelIds(draftConfig.value.piModelIds);
  const next: AiModelConfig = {
    url: draftConfig.value.url.trim(),
    key: draftConfig.value.key.trim(),
    modelIds,
    activeModelId: resolveActiveModelId(modelIds, draftConfig.value.activeModelId),
    sidecarType: draftConfig.value.sidecarType.trim(),
    piProvider: draftConfig.value.piProvider.trim(),
    piModel: resolveActiveModelId(piModelIds, draftConfig.value.piModel),
    piModelIds,
    piBaseUrl: draftConfig.value.piBaseUrl.trim(),
    piApiKey: draftConfig.value.piApiKey.trim(),
    piApi: draftConfig.value.piApi.trim(),
    piThinkingLevel: draftConfig.value.piThinkingLevel.trim(),
  };

  modelSaving.value = true;
  try {
    await invoke("write_ai_paths", {
      config: {
        sidecarPath: "",
        claudePath: claudePath.value.trim(),
      },
    });
    await invoke("write_ai_env", { config: next });
    modelConfig.value = next;
    draftConfig.value = { ...next };
    message.success(t("settings.ai.modelSaved"));
  } catch (e) {
    message.error(t("settings.ai.saveFailed", { error: String(e) }));
  } finally {
    modelSaving.value = false;
  }
}

onMounted(async () => {
  await Promise.all([loadClaudePath(), loadModelConfig()]);
});
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.ai.title") }}</div>
    <div class="settings-hint">{{ t("settings.ai.hint") }}</div>

    <!-- Model settings -->
    <div class="settings-section-title">{{ t("ai.modelDialog.title") }}</div>
    <NSpin :show="modelLoading" size="small">
      <NForm label-placement="top" size="small">
        <NFormItem :label="t('ai.modelDialog.sidecarType')">
          <NSelect
            v-model:value="draftConfig.sidecarType"
            :options="sidecarTypeOptions"
            :placeholder="t('ai.modelDialog.sidecarTypePlaceholder')"
          />
        </NFormItem>

        <!-- Claude sidecar config -->
        <template v-if="!isPiDraft">
          <NFormItem :label="t('ai.modelDialog.baseUrl')">
            <NInput
              v-model:value="draftConfig.url"
              placeholder="https://api.anthropic.com"
              clearable
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.authToken')">
            <NInput
              v-model:value="draftConfig.key"
              type="password"
              show-password-on="click"
              placeholder="sk-..."
              clearable
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.modelIds')">
            <NInput
              v-model:value="draftConfig.modelIds"
              type="textarea"
              :autosize="{ minRows: 2, maxRows: 4 }"
              placeholder="claude-sonnet-4-5, claude-opus-4-5"
              @update:value="syncDraftActiveModel"
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.activeModel')">
            <NSelect
              v-model:value="draftConfig.activeModelId"
              :options="modelOptions"
              :disabled="modelOptions.length === 0"
              :placeholder="t('ai.modelDialog.activeModelPlaceholder')"
            />
          </NFormItem>
          <NFormItem :label="t('settings.ai.claudePath')">
            <NInput
              v-model:value="claudePath"
              :placeholder="t('settings.ai.claudePathPlaceholder')"
              clearable
            />
          </NFormItem>
          <NSpace align="center" :size="8" style="margin-bottom: 4px">
            <NButton
              size="small"
              :loading="detecting"
              @click="detectClaude"
            >
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
              <NButton
                size="tiny"
                quaternary
                type="primary"
                @click="applyDetectedPath"
              >
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
        </template>

        <!-- Pi sidecar config -->
        <template v-else>
          <NFormItem :label="t('ai.modelDialog.piApi')">
            <NSelect
              v-model:value="draftConfig.piApi"
              :options="piApiOptions"
              :placeholder="t('ai.modelDialog.piApiPlaceholder')"
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.piProvider')">
            <NInput v-model:value="draftConfig.piProvider" placeholder="custom" />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.piBaseUrl')">
            <NInput v-model:value="draftConfig.piBaseUrl" placeholder="https://api.openai.com/v1" />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.piApiKey')">
            <NInput
              v-model:value="draftConfig.piApiKey"
              type="password"
              show-password-on="click"
              placeholder="sk-..."
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.piModelIds')">
            <NInput
              v-model:value="draftConfig.piModelIds"
              type="textarea"
              :autosize="{ minRows: 2, maxRows: 4 }"
              placeholder="deepseek-chat, deepseek-coder"
              @update:value="syncDraftPiModel"
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.piActiveModel')">
            <NSelect
              v-model:value="draftConfig.piModel"
              :options="piModelOptions"
              :disabled="piModelOptions.length === 0"
              :placeholder="t('ai.modelDialog.piActiveModelPlaceholder')"
            />
          </NFormItem>
          <NFormItem :label="t('ai.modelDialog.piThinkingLevel')">
            <NSelect
              v-model:value="draftConfig.piThinkingLevel"
              :options="piThinkingLevelOptions"
              :placeholder="t('ai.modelDialog.piThinkingLevelPlaceholder')"
            />
          </NFormItem>
        </template>

        <div v-if="sidecarTypeChanged" class="settings-hint" style="margin-bottom: 8px">
          {{ t("ai.modelDialog.restartHint") }}
        </div>

        <NButton
          type="primary"
          :loading="modelSaving"
          :disabled="modelLoading"
          @click="saveModelConfig"
        >
          <template #icon>
            <NIcon><SaveOutline /></NIcon>
          </template>
          {{ t("ai.modelDialog.save") }}
        </NButton>
      </NForm>
    </NSpin>

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

.settings-hint {
  color: var(--ashell-text-subtle);
  font-size: 12px;
  line-height: 1.6;
}

.field-hint {
  margin: -4px 0 8px;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.5;
}
</style>
