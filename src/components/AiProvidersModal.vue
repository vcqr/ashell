<script setup lang="ts">
import { ref, computed, watch } from "vue";
import {
  NModal,
  NCard,
  NButton,
  NIcon,
  NSpace,
  NInput,
  NSelect,
  NForm,
  NFormItem,
  NTag,
  NSpin,
  NPopconfirm,
  NEmpty,
  useMessage,
} from "naive-ui";
import {
  CloseOutline,
  AddOutline,
  TrashOutline,
  CloudDownloadOutline,
  CheckmarkOutline,
  SaveOutline,
} from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import type { AiProvider, AiProviderCreate } from "@/types";
import {
  listAiProviders,
  createAiProvider,
  updateAiProvider,
  deleteAiProvider,
  activateAiProvider,
} from "@/api/aiProviders";
import {
  sidecarTypeOptions,
  piApiOptions,
  piThinkingLevelOptions,
  parseModelIds,
  normalizeModelIds,
  resolveActiveModelId,
  inferApiType,
  getProviderEndpoint,
  fetchModelList,
} from "@/composables/useAiConfig";

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
}>();

const { t } = useI18n();
const message = useMessage();

const providers = ref<AiProvider[]>([]);
const selectedId = ref<string | null>(null);
const loading = ref(false);
const saving = ref(false);
const activating = ref(false);
const fetching = ref(false);

const selectedProvider = computed(() =>
  providers.value.find((p) => p.id === selectedId.value) ?? null,
);

type Draft = {
  name: string;
  sidecarType: string;
  url: string;
  apiKey: string;
  modelIds: string;
  activeModelId: string;
  piProvider: string;
  piModel: string;
  piModelIds: string;
  piBaseUrl: string;
  piApiKey: string;
  piApi: string;
  piThinkingLevel: string;
};

function emptyDraft(): Draft {
  return {
    name: "", sidecarType: "claude", url: "", apiKey: "", modelIds: "",
    activeModelId: "", piProvider: "", piModel: "", piModelIds: "", piBaseUrl: "",
    piApiKey: "", piApi: "", piThinkingLevel: "off",
  };
}

function providerToDraft(p: AiProvider): Draft {
  return {
    name: p.name,
    sidecarType: p.sidecar_type || "claude",
    url: p.url, apiKey: p.api_key, modelIds: p.model_ids, activeModelId: p.active_model_id,
    piProvider: p.pi_provider, piModel: p.pi_model, piModelIds: p.pi_model_ids,
    piBaseUrl: p.pi_base_url, piApiKey: p.pi_api_key, piApi: p.pi_api, piThinkingLevel: p.pi_thinking_level,
  };
}

const draft = ref<Draft>(emptyDraft());
const isPiDraft = computed(() => draft.value.sidecarType === "pi");

const modelOptions = computed(() =>
  parseModelIds(draft.value.modelIds).map((id) => ({ label: id, value: id })),
);

const piModelOptions = computed(() =>
  parseModelIds(draft.value.piModelIds).map((id) => ({ label: id, value: id })),
);

function syncDraftActiveModel() {
  draft.value.activeModelId = resolveActiveModelId(draft.value.modelIds, draft.value.activeModelId);
}
function syncDraftPiModel() {
  draft.value.piModel = resolveActiveModelId(draft.value.piModelIds, draft.value.piModel);
}

watch(selectedId, (id) => {
  const p = providers.value.find((x) => x.id === id);
  draft.value = p ? providerToDraft(p) : emptyDraft();
});

async function loadProviders() {
  loading.value = true;
  try {
    providers.value = await listAiProviders();
    if (providers.value.length > 0) {
      const active = providers.value.find((p) => p.is_active);
      selectedId.value = active?.id ?? providers.value[0].id;
    } else {
      selectedId.value = null;
      draft.value = emptyDraft();
    }
  } catch (e) {
    message.error(t("settings.ai.loadFailed", { error: String(e) }));
  } finally {
    loading.value = false;
  }
}

async function addProvider() {
  const input: AiProviderCreate = {
    name: t("settings.ai.provider.addPlaceholder"),
    sidecar_type: "claude",
  };
  try {
    const p = await createAiProvider(input);
    providers.value.push(p);
    selectedId.value = p.id;
  } catch (e) {
    message.error(t("settings.ai.provider.saveFailed", { error: String(e) }));
  }
}

async function saveProvider() {
  if (!selectedId.value) return;
  if (!draft.value.name.trim()) {
    message.warning(t("settings.ai.provider.nameRequired"));
    return;
  }
  saving.value = true;
  try {
    const modelIds = normalizeModelIds(draft.value.modelIds);
    const piModelIds = normalizeModelIds(draft.value.piModelIds);
    const updated = await updateAiProvider(selectedId.value, {
      name: draft.value.name.trim(),
      sidecar_type: draft.value.sidecarType,
      url: draft.value.url.trim(),
      api_key: draft.value.apiKey.trim(),
      model_ids: modelIds,
      active_model_id: resolveActiveModelId(modelIds, draft.value.activeModelId),
      pi_provider: draft.value.piProvider.trim(),
      pi_model: resolveActiveModelId(piModelIds, draft.value.piModel),
      pi_model_ids: piModelIds,
      pi_base_url: draft.value.piBaseUrl.trim(),
      pi_api_key: draft.value.piApiKey.trim(),
      pi_api: draft.value.piApi,
      pi_thinking_level: draft.value.piThinkingLevel,
    });
    const idx = providers.value.findIndex((p) => p.id === updated.id);
    if (idx >= 0) providers.value[idx] = updated;
    draft.value = providerToDraft(updated);
    message.success(t("settings.ai.modelSaved"));
  } catch (e) {
    message.error(t("settings.ai.provider.saveFailed", { error: String(e) }));
  } finally {
    saving.value = false;
  }
}

async function removeProvider() {
  if (!selectedId.value) return;
  try {
    await deleteAiProvider(selectedId.value);
    providers.value = providers.value.filter((p) => p.id !== selectedId.value);
    if (providers.value.length > 0) {
      selectedId.value = providers.value[0].id;
    } else {
      selectedId.value = null;
      draft.value = emptyDraft();
    }
    message.success(t("settings.ai.provider.delete"));
  } catch (e) {
    message.error(t("settings.ai.provider.saveFailed", { error: String(e) }));
  }
}

async function activateProvider() {
  if (!selectedId.value) return;
  activating.value = true;
  try {
    const activated = await activateAiProvider(selectedId.value);
    providers.value = providers.value.map((p) => ({ ...p, is_active: p.id === activated.id }));
    message.success(t("settings.ai.provider.activated"));
  } catch (e) {
    message.error(t("settings.ai.provider.saveFailed", { error: String(e) }));
  } finally {
    activating.value = false;
  }
}

async function fetchModels() {
  const { baseUrl, apiKey } = getProviderEndpoint(
    draft.value.sidecarType, draft.value.url, draft.value.piBaseUrl,
    draft.value.apiKey, draft.value.piApiKey,
  );
  if (!baseUrl.trim() || !apiKey.trim()) {
    message.warning(t("settings.ai.provider.fetchNeedKey"));
    return;
  }
  const apiType = inferApiType(draft.value.sidecarType, draft.value.piApi);
  fetching.value = true;
  try {
    const models = await fetchModelList(baseUrl.trim(), apiKey.trim(), apiType);
    if (models.length === 0) {
      message.warning(t("settings.ai.provider.fetchFailed", { error: "empty" }));
      return;
    }
    if (isPiDraft.value) {
      draft.value.piModelIds = models.join(", ");
      syncDraftPiModel();
    } else {
      draft.value.modelIds = models.join(", ");
      syncDraftActiveModel();
    }
    message.success(t("settings.ai.provider.fetchSuccess", { count: models.length }));
  } catch (e) {
    message.error(t("settings.ai.provider.fetchFailed", { error: String(e) }));
  } finally {
    fetching.value = false;
  }
}

const canFetchModels = computed(() => {
  if (isPiDraft.value) return draft.value.piBaseUrl.trim() && draft.value.piApiKey.trim();
  return draft.value.url.trim() && draft.value.apiKey.trim();
});

watch(() => props.open, (v) => {
  if (v) loadProviders();
});
</script>

<template>
  <NModal :show="open" @update:show="(v: boolean) => emit('update:open', v)">
    <NCard
      style="width: min(720px, 90vw); height: min(560px, 80vh)"
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

      <div class="provider-layout">
        <!-- Left: provider list -->
        <div class="provider-sidebar">
          <div class="provider-list">
            <NSpin :show="loading" size="small">
              <button
                v-for="p in providers"
                :key="p.id"
                class="provider-item"
                :class="{ active: p.id === selectedId }"
                type="button"
                @click="selectedId = p.id"
              >
                <span class="provider-item-name">{{ p.name }}</span>
                <NTag v-if="p.is_active" size="tiny" type="success" :bordered="false" round>
                  {{ t("settings.ai.provider.activated") }}
                </NTag>
              </button>
              <NEmpty
                v-if="!loading && providers.length === 0"
                :description="t('settings.ai.provider.emptyHint')"
                size="small"
              />
            </NSpin>
          </div>
          <NButton block dashed size="small" @click="addProvider">
            <template #icon><NIcon><AddOutline /></NIcon></template>
            {{ t("settings.ai.provider.add") }}
          </NButton>
        </div>

        <!-- Right: provider detail -->
        <div class="provider-detail">
          <template v-if="selectedProvider">
            <NScrollbar class="detail-scroll">
              <NForm label-placement="top" size="small">
                <div class="detail-header">
                  <NFormItem :label="t('settings.ai.provider.name')" style="flex: 1; margin-bottom: 0">
                    <NInput v-model:value="draft.name" :placeholder="t('settings.ai.provider.namePlaceholder')" />
                  </NFormItem>
                  <NSpace :size="8" align="end">
                    <NButton
                      size="small" type="primary" :loading="activating"
                      :disabled="selectedProvider.is_active" @click="activateProvider"
                    >
                      <template #icon><NIcon><CheckmarkOutline /></NIcon></template>
                      {{ selectedProvider.is_active ? t("settings.ai.provider.activated") : t("settings.ai.provider.activate") }}
                    </NButton>
                    <NPopconfirm @positive-click="removeProvider">
                      <template #trigger>
                        <NButton size="small" quaternary type="error">
                          <template #icon><NIcon><TrashOutline /></NIcon></template>
                          {{ t("settings.ai.provider.delete") }}
                        </NButton>
                      </template>
                      {{ t("settings.ai.provider.deleteConfirm") }}
                    </NPopconfirm>
                  </NSpace>
                </div>

                <NFormItem :label="t('ai.modelDialog.sidecarType')">
                  <NSelect
                    v-model:value="draft.sidecarType"
                    :options="sidecarTypeOptions"
                    :placeholder="t('ai.modelDialog.sidecarTypePlaceholder')"
                  />
                </NFormItem>

                <template v-if="!isPiDraft">
                  <NFormItem :label="t('ai.modelDialog.baseUrl')">
                    <NInput v-model:value="draft.url" placeholder="https://api.anthropic.com" clearable />
                  </NFormItem>
                  <NFormItem :label="t('ai.modelDialog.authToken')">
                    <NInput v-model:value="draft.apiKey" type="password" show-password-on="click" placeholder="sk-..." clearable />
                  </NFormItem>
                  <NFormItem>
                    <NButton size="small" :loading="fetching" :disabled="!canFetchModels" @click="fetchModels">
                      <template #icon><NIcon><CloudDownloadOutline /></NIcon></template>
                      {{ fetching ? t("settings.ai.provider.fetching") : t("settings.ai.provider.fetchModels") }}
                    </NButton>
                  </NFormItem>
                  <NFormItem :label="t('settings.ai.provider.modelIds')">
                    <NInput
                      v-model:value="draft.modelIds" type="textarea"
                      :autosize="{ minRows: 2, maxRows: 4 }"
                      placeholder="claude-sonnet-4-5, claude-opus-4-5"
                      @update:value="syncDraftActiveModel"
                    />
                  </NFormItem>
                  <NFormItem :label="t('settings.ai.provider.activeModel')">
                    <NSelect
                      v-model:value="draft.activeModelId" :options="modelOptions"
                      :disabled="modelOptions.length === 0"
                      :placeholder="t('settings.ai.provider.activeModelPlaceholder')"
                    />
                  </NFormItem>
                </template>

                <template v-else>
                  <NFormItem :label="t('ai.modelDialog.piApi')">
                    <NSelect v-model:value="draft.piApi" :options="piApiOptions" :placeholder="t('ai.modelDialog.piApiPlaceholder')" />
                  </NFormItem>
                  <NFormItem :label="t('ai.modelDialog.piProvider')">
                    <NInput v-model:value="draft.piProvider" placeholder="custom" />
                  </NFormItem>
                  <NFormItem :label="t('ai.modelDialog.piBaseUrl')">
                    <NInput v-model:value="draft.piBaseUrl" placeholder="https://api.openai.com/v1" />
                  </NFormItem>
                  <NFormItem :label="t('ai.modelDialog.piApiKey')">
                    <NInput v-model:value="draft.piApiKey" type="password" show-password-on="click" placeholder="sk-..." />
                  </NFormItem>
                  <NFormItem>
                    <NButton size="small" :loading="fetching" :disabled="!canFetchModels" @click="fetchModels">
                      <template #icon><NIcon><CloudDownloadOutline /></NIcon></template>
                      {{ fetching ? t("settings.ai.provider.fetching") : t("settings.ai.provider.fetchModels") }}
                    </NButton>
                  </NFormItem>
                  <NFormItem :label="t('settings.ai.provider.modelIds')">
                    <NInput
                      v-model:value="draft.piModelIds" type="textarea"
                      :autosize="{ minRows: 2, maxRows: 4 }"
                      placeholder="deepseek-chat, deepseek-coder"
                      @update:value="syncDraftPiModel"
                    />
                  </NFormItem>
                  <NFormItem :label="t('settings.ai.provider.activeModel')">
                    <NSelect
                      v-model:value="draft.piModel" :options="piModelOptions"
                      :disabled="piModelOptions.length === 0"
                      :placeholder="t('settings.ai.provider.activeModelPlaceholder')"
                    />
                  </NFormItem>
                  <NFormItem :label="t('ai.modelDialog.piThinkingLevel')">
                    <NSelect
                      v-model:value="draft.piThinkingLevel" :options="piThinkingLevelOptions"
                      :placeholder="t('ai.modelDialog.piThinkingLevelPlaceholder')"
                    />
                  </NFormItem>
                </template>

                <NButton type="primary" :loading="saving" @click="saveProvider">
                  <template #icon><NIcon><SaveOutline /></NIcon></template>
                  {{ saving ? t("settings.ai.provider.saving") : t("settings.ai.provider.save") }}
                </NButton>
              </NForm>
            </NScrollbar>
          </template>

          <NEmpty v-else :description="t('settings.ai.provider.emptyHint')" style="padding: 40px 0" />
        </div>
      </div>
    </NCard>
  </NModal>
</template>

<style scoped>
.provider-layout {
  display: flex;
  gap: 16px;
  height: 100%;
  min-height: 0;
}

.provider-sidebar {
  width: 180px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.provider-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-right: 4px;
}

.provider-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
  width: 100%;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--ashell-text-muted);
  cursor: pointer;
  font: inherit;
  font-size: 13px;
  padding: 8px 10px;
  text-align: left;
  transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
}

.provider-item:hover {
  background: var(--ashell-hover);
  color: var(--ashell-text);
}

.provider-item.active {
  background: var(--ashell-active);
  color: var(--ashell-text-strong);
  border-color: var(--ashell-primary, #7c5cff);
}

.provider-item-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.provider-detail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.detail-scroll {
  flex: 1;
}

.detail-header {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  margin-bottom: 8px;
}
</style>

<style>
.providers-card .n-card__content {
  height: 100%;
  overflow: hidden;
}
</style>
