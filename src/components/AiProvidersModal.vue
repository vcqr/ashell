<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { storeToRefs } from "pinia";
import {
  NModal,
  NCard,
  NButton,
  NIcon,
  NInput,
  NSelect,
  NForm,
  NFormItem,
  NSpin,
  NPopconfirm,
  NEmpty,
  NScrollbar,
  useMessage,
} from "naive-ui";
import {
  CloseOutline,
  AddOutline,
  TrashOutline,
  CloudDownloadOutline,
  SaveOutline,
} from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import type { AiProvider, AiProviderCreate } from "@/types";
import {
  createAiProvider,
  updateAiProvider,
  deleteAiProvider,
} from "@/api/aiProviders";
import { useAiConfigStore } from "@/stores/aiConfig";
import {
  apiTypeOptions,
  normalizeModelIds,
  inferFetchApiType,
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
const aiConfig = useAiConfigStore();
const { providers } = storeToRefs(aiConfig);

const selectedId = ref<string | null>(null);
const loading = ref(false);
const saving = ref(false);
const fetching = ref(false);

const selectedProvider = computed(() =>
  providers.value.find((p) => p.id === selectedId.value) ?? null,
);

type Draft = {
  name: string;
  apiType: string;
  baseUrl: string;
  apiKey: string;
  modelIds: string;
};

function emptyDraft(): Draft {
  return {
    name: "",
    apiType: "openai-completions",
    baseUrl: "",
    apiKey: "",
    modelIds: "",
  };
}

function providerToDraft(p: AiProvider): Draft {
  return {
    name: p.name,
    apiType: p.api_type || "openai-completions",
    baseUrl: p.base_url,
    apiKey: p.api_key,
    modelIds: p.model_ids,
  };
}

const draft = ref<Draft>(emptyDraft());

watch(selectedId, (id) => {
  const p = providers.value.find((x) => x.id === id);
  draft.value = p ? providerToDraft(p) : emptyDraft();
});

async function loadProviders() {
  loading.value = true;
  try {
    await aiConfig.load();
    selectedId.value = providers.value[0]?.id ?? null;
    if (!selectedId.value) {
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
  };
  try {
    const p = await createAiProvider(input);
    await aiConfig.load();
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
    const updated = await updateAiProvider(selectedId.value, {
      name: draft.value.name.trim(),
      api_type: draft.value.apiType,
      base_url: draft.value.baseUrl.trim(),
      api_key: draft.value.apiKey.trim(),
      model_ids: normalizeModelIds(draft.value.modelIds),
    });
    await aiConfig.load();
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
    await aiConfig.load();
    selectedId.value = providers.value[0]?.id ?? null;
    if (!selectedId.value) {
      draft.value = emptyDraft();
    }
    message.success(t("settings.ai.provider.delete"));
  } catch (e) {
    message.error(t("settings.ai.provider.saveFailed", { error: String(e) }));
  }
}

async function fetchModels() {
  const baseUrl = draft.value.baseUrl.trim();
  const apiKey = draft.value.apiKey.trim();
  if (!baseUrl || !apiKey) {
    message.warning(t("settings.ai.provider.fetchNeedKey"));
    return;
  }
  fetching.value = true;
  try {
    const models = await fetchModelList(baseUrl, apiKey, inferFetchApiType(draft.value.apiType));
    if (models.length === 0) {
      message.warning(t("settings.ai.provider.fetchFailed", { error: "empty" }));
      return;
    }
    draft.value.modelIds = models.join(", ");
    message.success(t("settings.ai.provider.fetchSuccess", { count: models.length }));
  } catch (e) {
    message.error(t("settings.ai.provider.fetchFailed", { error: String(e) }));
  } finally {
    fetching.value = false;
  }
}

const canFetchModels = computed(() => draft.value.baseUrl.trim() && draft.value.apiKey.trim());

watch(() => props.open, (v) => {
  if (v) loadProviders();
});
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
                <NFormItem :label="t('settings.ai.provider.name')">
                  <NInput v-model:value="draft.name" :placeholder="t('settings.ai.provider.namePlaceholder')" />
                </NFormItem>

                <NFormItem :label="t('settings.ai.provider.apiType')">
                  <NSelect
                    v-model:value="draft.apiType"
                    :options="apiTypeOptions"
                    :placeholder="t('settings.ai.provider.apiTypePlaceholder')"
                  />
                </NFormItem>
                <NFormItem :label="t('settings.ai.provider.baseUrl')">
                  <NInput v-model:value="draft.baseUrl" placeholder="https://api.openai.com/v1" clearable />
                </NFormItem>
                <NFormItem :label="t('settings.ai.provider.apiKey')">
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
                    placeholder="deepseek-chat, deepseek-coder"
                  />
                </NFormItem>
              </NForm>
            </NScrollbar>

            <div class="detail-actions">
              <NPopconfirm @positive-click="removeProvider">
                <template #trigger>
                  <NButton type="error">
                    <template #icon><NIcon><TrashOutline /></NIcon></template>
                    {{ t("settings.ai.provider.delete") }}
                  </NButton>
                </template>
                {{ t("settings.ai.provider.deleteConfirm") }}
              </NPopconfirm>
              <NButton type="primary" :loading="saving" @click="saveProvider">
                <template #icon><NIcon><SaveOutline /></NIcon></template>
                {{ saving ? t("settings.ai.provider.saving") : t("settings.ai.provider.save") }}
              </NButton>
            </div>
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
  flex: 1;
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
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.detail-scroll {
  flex: 1;
}

.detail-actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 12px;
  padding-top: 12px;
  flex-shrink: 0;
}
</style>

<style>
.providers-card .n-card-content {
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
