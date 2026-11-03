<script setup lang="ts">
import {
  RiAddLine,
  RiCloseLine,
  RiDeleteBinLine,
  RiDownloadCloudLine,
  RiSaveLine,
  RiSearchLine,
} from "@remixicon/vue";
import { ref, computed, onMounted, watch } from "vue";
import { storeToRefs } from "pinia";
import {
  NButton,
  NCheckbox,
  NIcon,
  NInput,
  NInputGroup,
  NSelect,
  NForm,
  NSpin,
  NPopconfirm,
  NEmpty,
  NScrollbar,
  useMessage,
} from "naive-ui";

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
  parseModelIds,
  inferFetchApiType,
  fetchModelList,
} from "@/composables/useAiConfig";

/**
 * 模型供应商管理（供应商列表 + 详情编辑）。
 *
 * 同一份主体两处复用：设置弹窗的「模型供应商」分区、独立 AI 窗口的
 * AiProvidersModal 包装。数据走共享 aiConfig store，与 AI 助手/设置 AI
 * 分区即时同步。
 */
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
  disabledIds: string[];
};

function emptyDraft(): Draft {
  return {
    name: "",
    apiType: "openai-completions",
    baseUrl: "",
    apiKey: "",
    modelIds: "",
    disabledIds: [],
  };
}

function providerToDraft(p: AiProvider): Draft {
  return {
    name: p.name,
    apiType: p.api_type || "openai-completions",
    baseUrl: p.base_url,
    apiKey: p.api_key,
    modelIds: p.model_ids,
    disabledIds: parseModelIds(p.disabled_model_ids ?? ""),
  };
}

const draft = ref<Draft>(emptyDraft());

// ── 候选模型列表（textarea 的结构化镜像：筛选 / 勾选启停 / 手动添加）──

const modelFilter = ref("");
const newModelId = ref("");

/** 候选全量：与 textarea 双向镜像（textarea 手改实时反映到这里） */
const modelList = computed(() => parseModelIds(draft.value.modelIds));

const disabledSet = computed(() => new Set(draft.value.disabledIds));

const enabledCount = computed(
  () => modelList.value.filter((id) => !disabledSet.value.has(id)).length,
);

const filteredModels = computed(() => {
  const kw = modelFilter.value.trim().toLowerCase();
  if (!kw) return modelList.value;
  return modelList.value.filter((id) => id.toLowerCase().includes(kw));
});

/** 勾选/取消勾选 = 加入/移出禁用子集 */
function toggleModel(id: string, enabled: boolean) {
  const rest = draft.value.disabledIds.filter((m) => m !== id);
  if (!enabled) rest.push(id);
  draft.value.disabledIds = rest;
}

/** 批量启停：只作用于当前筛选可见的行（与搜索框组合：搜到什么选什么） */
function bulkToggleModel(mode: "all" | "none" | "invert") {
  const disabled = new Set(draft.value.disabledIds);
  for (const id of filteredModels.value) {
    if (mode === "all") {
      disabled.delete(id);
    } else if (mode === "none") {
      disabled.add(id);
    } else if (disabled.has(id)) {
      disabled.delete(id);
    } else {
      disabled.add(id);
    }
  }
  draft.value.disabledIds = [...disabled];
}

/** 从候选列表移除该模型（同步清掉禁用标记） */
function removeModel(id: string) {
  draft.value.modelIds = modelList.value.filter((m) => m !== id).join(", ");
  draft.value.disabledIds = draft.value.disabledIds.filter((m) => m !== id);
}

/** 手动添加：支持一次粘多个（逗号分隔），已存在的跳过 */
function addManualModel() {
  const parts = parseModelIds(newModelId.value);
  if (parts.length === 0) return;
  const merged = [...modelList.value, ...parts.filter((p) => !modelList.value.includes(p))];
  draft.value.modelIds = merged.join(", ");
  newModelId.value = "";
}

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
      // 禁用子集按当前候选全量 prune（textarea 手删的模型不再残留禁用标记）
      disabled_model_ids: draft.value.disabledIds
        .filter((id) => modelList.value.includes(id))
        .join(", "),
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
    // 合并写入而非整体覆盖：手工挑选的模型不丢，禁用标记保留在 disabledIds，
    // 新发现的模型默认启用
    const existing = modelList.value;
    const added = models.filter((m) => !existing.includes(m));
    draft.value.modelIds = [...existing, ...added].join(", ");
    message.success(
      t("settings.ai.provider.fetchMerged", { added: added.length, count: existing.length + added.length }),
    );
  } catch (e) {
    message.error(t("settings.ai.provider.fetchFailed", { error: String(e) }));
  } finally {
    fetching.value = false;
  }
}

const canFetchModels = computed(() => draft.value.baseUrl.trim() && draft.value.apiKey.trim());

onMounted(() => {
  void loadProviders();
});
</script>

<template>
  <section class="providers-section">
    <div class="provider-layout">
      <!-- Left: provider list（增删交互块保持原有内部结构，整体入卡） -->
      <div class="provider-sidebar">
        <NButton block dashed size="small" @click="addProvider">
          <template #icon><NIcon><RiAddLine /></NIcon></template>
          {{ t("settings.ai.provider.add") }}
        </NButton>
        <div class="provider-list">
          <div class="settings-block list-card">
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
        </div>
      </div>

      <!-- Right: provider detail（表单项转行式入卡） -->
      <div class="provider-detail">
        <template v-if="selectedProvider">
          <NScrollbar class="detail-scroll">
            <NForm size="small" :show-feedback="false">
              <div class="settings-block detail-card">
                <div class="setting-row">
                  <div class="setting-row-main">
                    <div class="setting-row-label">{{ t("settings.ai.provider.name") }}</div>
                  </div>
                  <div class="setting-row-control w-input">
                    <NInput v-model:value="draft.name" :placeholder="t('settings.ai.provider.namePlaceholder')" />
                  </div>
                </div>
                <div class="setting-row">
                  <div class="setting-row-main">
                    <div class="setting-row-label">{{ t("settings.ai.provider.apiType") }}</div>
                  </div>
                  <div class="setting-row-control w-input">
                    <NSelect
                      v-model:value="draft.apiType"
                      :options="apiTypeOptions"
                      :placeholder="t('settings.ai.provider.apiTypePlaceholder')"
                    />
                  </div>
                </div>
                <div class="setting-row">
                  <div class="setting-row-main">
                    <div class="setting-row-label">{{ t("settings.ai.provider.baseUrl") }}</div>
                  </div>
                  <div class="setting-row-control w-input">
                    <NInput v-model:value="draft.baseUrl" placeholder="https://api.openai.com/v1" clearable />
                  </div>
                </div>
                <div class="setting-row">
                  <div class="setting-row-main">
                    <div class="setting-row-label">{{ t("settings.ai.provider.apiKey") }}</div>
                  </div>
                  <div class="setting-row-control w-input">
                    <NInput v-model:value="draft.apiKey" type="password" show-password-on="click" placeholder="sk-..." clearable />
                  </div>
                </div>
                <div class="setting-row row-stacked">
                  <div class="setting-row-main">
                    <div class="setting-row-label">{{ t("settings.ai.provider.modelIds") }}</div>
                  </div>
                  <div class="setting-row-control model-ids-control">
                    <NButton size="small" :loading="fetching" :disabled="!canFetchModels" @click="fetchModels">
                      <template #icon><NIcon><RiDownloadCloudLine /></NIcon></template>
                      {{ fetching ? t("settings.ai.provider.fetching") : t("settings.ai.provider.fetchModels") }}
                    </NButton>
                    <NInput
                      v-model:value="draft.modelIds" type="textarea"
                      :autosize="{ minRows: 4, maxRows: 12 }"
                      placeholder="deepseek-chat, deepseek-coder"
                    />
                    <!-- 结构化列表：仅在已有候选模型时出现，逐行筛选 / 勾选启停 / 移除 -->
                    <div v-if="modelList.length > 0" class="model-list-block">
                      <div class="model-list-toolbar">
                        <NInput
                          v-model:value="modelFilter" size="small" clearable
                          :placeholder="t('settings.ai.provider.modelSearchPlaceholder')"
                        >
                          <template #prefix><NIcon :size="14"><RiSearchLine /></NIcon></template>
                        </NInput>
                        <NButton size="tiny" tertiary @click="bulkToggleModel('all')">
                          {{ t("settings.ai.provider.selectAll") }}
                        </NButton>
                        <NButton size="tiny" tertiary @click="bulkToggleModel('none')">
                          {{ t("settings.ai.provider.selectNone") }}
                        </NButton>
                        <NButton size="tiny" tertiary @click="bulkToggleModel('invert')">
                          {{ t("settings.ai.provider.invertSelection") }}
                        </NButton>
                        <span class="model-count">
                          {{ t("settings.ai.provider.enabledCount", { enabled: enabledCount, total: modelList.length }) }}
                        </span>
                      </div>
                      <div class="model-list">
                        <div v-for="id in filteredModels" :key="id" class="model-item">
                          <NCheckbox
                            size="small"
                            :checked="!disabledSet.has(id)"
                            @update:checked="(v: boolean) => toggleModel(id, v)"
                          >
                            <span class="model-item-name" :class="{ off: disabledSet.has(id) }">{{ id }}</span>
                          </NCheckbox>
                          <button
                            class="model-item-remove" type="button"
                            :title="t('settings.ai.provider.modelRemove')"
                            @click="removeModel(id)"
                          >
                            <NIcon :size="14"><RiCloseLine /></NIcon>
                          </button>
                        </div>
                        <div v-if="filteredModels.length === 0" class="model-list-empty">
                          {{ t("settings.ai.provider.modelNoMatch") }}
                        </div>
                      </div>
                      <NInputGroup size="small">
                        <NInput
                          v-model:value="newModelId"
                          :placeholder="t('settings.ai.provider.modelAddPlaceholder')"
                          @keyup.enter="addManualModel"
                        />
                        <NButton size="small" @click="addManualModel">
                          {{ t("settings.ai.provider.modelAdd") }}
                        </NButton>
                      </NInputGroup>
                    </div>
                  </div>
                </div>
              </div>

              <!-- 操作按钮跟随表单内容（删除分离到左侧，避免与保存误触） -->
              <div class="detail-actions">
                <NPopconfirm @positive-click="removeProvider">
                  <template #trigger>
                    <NButton type="error">
                      <template #icon><NIcon><RiDeleteBinLine /></NIcon></template>
                      {{ t("settings.ai.provider.delete") }}
                    </NButton>
                  </template>
                  {{ t("settings.ai.provider.deleteConfirm") }}
                </NPopconfirm>
                <NButton type="primary" :loading="saving" @click="saveProvider">
                  <template #icon><NIcon><RiSaveLine /></NIcon></template>
                  {{ saving ? t("settings.ai.provider.saving") : t("settings.ai.provider.save") }}
                </NButton>
              </div>
            </NForm>
          </NScrollbar>
        </template>

        <NEmpty v-else :description="t('settings.ai.provider.emptyHint')" style="padding: 40px 0" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.providers-section {
  /* 设置分区（min-height:100% 的 flex 容器）与弹窗（定高 flex 容器）内都撑满可用高度 */
  height: 100%;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

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
  border-color: var(--ashell-primary, #6d5ae8);
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

/* 操作按钮跟随表单尾部（删除有 popconfirm 二次确认，无需隔离） */
.detail-actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 12px;
  padding-top: 12px;
}

/* 本分区对全局卡片的两处布局微调（不重定义全局类） */
/* 列表卡贴住侧栏顶部与详情卡对齐；横向内边距收窄给列表项留空间 */
.list-card {
  margin-top: 0;
  padding: 4px 8px;
}

.detail-card {
  margin-top: 0;
}

/* 输入列加宽：Base URL / API Key 内容长，280px 显示不全。
 * label 列固定等宽（取最长文案"Model Provider Name"）不换行，
 * 输入框弹性占满剩余宽度，四行起点对齐 */
.detail-card > .setting-row:not(.row-stacked) > .setting-row-main {
  flex: none;
  width: 150px;
  white-space: nowrap;
}

.detail-card > .setting-row:not(.row-stacked) .w-input {
  width: auto;
  flex: 1;
  min-width: 0;
}

/* 候选模型行改为上下布局：标签在上，按钮+textarea 占满整卡宽度，
 * 模型多时列表框够用 */
.row-stacked {
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
}

.row-stacked > .setting-row-main {
  flex: none;
}

/* 「获取模型列表」按钮在上、候选模型 textarea 在下，纵向堆叠 */
.model-ids-control {
  width: 100%;
  flex-direction: column;
  align-items: stretch;
}

.model-ids-control > .n-button {
  align-self: flex-start;
}

/* 候选模型结构化列表：筛选行 + 逐行勾选 + 手动添加，超过 5 行左右内部滚动 */
.model-list-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.model-list-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.model-count {
  font-size: 12px;
  color: var(--ashell-text-muted);
  white-space: nowrap;
}

.model-list {
  border: 1px solid var(--ashell-border);
  border-radius: 8px;
  max-height: 220px;
  overflow-y: auto;
}

.model-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 10px;
  font-size: 13px;
}

.model-item + .model-item {
  border-top: 1px solid var(--ashell-border-soft);
}

.model-item:hover {
  background: var(--ashell-hover);
}

.model-item-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-item-name.off {
  color: var(--ashell-text-muted);
}

.model-item-remove {
  opacity: 0;
  display: inline-flex;
  border: none;
  background: transparent;
  color: var(--ashell-text-muted);
  cursor: pointer;
  padding: 2px;
  border-radius: 4px;
  transition: opacity 0.15s ease, color 0.15s ease;
}

.model-item:hover .model-item-remove {
  opacity: 1;
}

.model-item-remove:hover {
  color: var(--ashell-text);
}

.model-list-empty {
  padding: 14px;
  font-size: 12px;
  color: var(--ashell-text-muted);
  text-align: center;
}
</style>
