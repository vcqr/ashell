<script setup lang="ts">
import {
  RiCheckboxCircleLine,
  RiCloseCircleLine,
  RiSearchLine,
} from "@remixicon/vue";
import { ref, computed, onMounted, watch } from "vue";
import { storeToRefs } from "pinia";
import {
  NForm,
  NInput,
  NInputNumber,
  NSelect,
  NButton,
  NSpace,
  NSwitch,
  NIcon,
  NTag,
  useMessage,
} from "naive-ui";

import { request } from "@/api/client";
import { isTauri } from "@/utils/platform";
import { useI18n } from "vue-i18n";
import { useAiConfigStore } from "@/stores/aiConfig";
import { useStartupStore } from "@/stores/startup";
import {
  sidecarTypeOptions,
  thinkingLevelOptions,
  enabledModelIds,
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
  // 只列启用的候选模型（禁用的对引擎不可见）；激活模型兜底保留展示
  const list = enabledModelIds(currentProvider.value);
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

async function patchEngine(input: {
  provider_id?: string;
  active_model_id?: string;
  thinking_level?: string;
  context_window?: string;
  max_tokens?: string;
  max_rounds?: string;
  streaming?: string;
}) {
  try {
    await aiConfig.patchEngine(input);
  } catch (e) {
    message.error(t("settings.ai.saveFailed", { error: String(e) }));
  }
}

// ── 上下文控制（空串 = 模型默认/不限制）──
// 本地 ref 承载输入：保存后 store 回写不会打断键入；失焦才提交（update-value-on-input=false）

function tokenFieldOrNull(value: string | undefined): number | null {
  const n = Number.parseInt(value ?? "", 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

const ctxWindowInput = ref<number | null>(null);
const maxTokensInput = ref<number | null>(null);
const maxRoundsInput = ref<number | null>(null);
const streamingOn = ref(true);

// 仅在引擎行切换 / 首次加载时从 store 拉取，避免保存回写覆盖正在编辑的值
watch(
  () => currentEngine.value?.engine,
  () => {
    ctxWindowInput.value = tokenFieldOrNull(currentEngine.value?.context_window);
    maxTokensInput.value = tokenFieldOrNull(currentEngine.value?.max_tokens);
    maxRoundsInput.value = tokenFieldOrNull(currentEngine.value?.max_rounds);
    streamingOn.value = currentEngine.value?.streaming !== "0";
  },
  { immediate: true },
);

function patchTokenField(field: "context_window" | "max_tokens" | "max_rounds", v: number | null) {
  void patchEngine({ [field]: v && v > 0 ? String(Math.floor(v)) : "" });
}

function patchStreaming(v: boolean) {
  // 乐观更新：点击立即响应，失败再回滚（busy 期间被丢弃的请求也回滚）
  if (v === streamingOn.value) return;
  streamingOn.value = v;
  void Promise.resolve(aiConfig.patchEngine({ streaming: v ? "1" : "0" }))
    .then((updated) => {
      if (!updated) streamingOn.value = !v;
    })
    .catch((e) => {
      streamingOn.value = !v;
      message.error(t("settings.ai.saveFailed", { error: String(e) }));
    });
}

// ── Claude Code 路径 ──

const claudePath = ref("");
const detectedPath = ref<string | null>(null);
const detecting = ref(false);
const detectionAttempted = ref(false);

async function loadClaudePath() {
  try {
    if (isTauri) {
      const { invoke } = await import("@tauri-apps/api/core");
      const config = await invoke<{ claudePath: string }>("read_ai_paths");
      claudePath.value = config.claudePath;
    } else {
      const config = await request<{ claudePath: string }>("/api/ai/paths");
      claudePath.value = config?.claudePath ?? "";
    }
  } catch {
    // ignore
  }
}

async function detectClaude() {
  detectionAttempted.value = true;
  detecting.value = true;
  try {
    detectedPath.value = isTauri
      ? await (async () => {
          const { invoke } = await import("@tauri-apps/api/core");
          return invoke<string | null>("detect_claude_path");
        })()
      : (await request<{ path: string | null }>("/api/ai/detect-claude-path", { method: "POST" }))?.path ?? null;
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
    const payload = { sidecarPath: "", claudePath: claudePath.value.trim() };
    if (isTauri) {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("write_ai_paths", { config: payload });
    } else {
      await request("/api/ai/paths", { method: "POST", json: payload });
    }
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
    <div class="settings-section-title">{{ t("settings.ai.title") }}</div>
    <NForm size="small" :show-feedback="false">
      <!-- 启用开关：应用级开关，独立小卡（无现成组标题 key，不新增） -->
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.enabled") }}</div>
            <div class="setting-row-desc">{{ t("settings.ai.enabledDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch
              :value="startupStore.aiAssistantEnabled"
              @update:value="(v: boolean) => startupStore.setAiAssistantEnabled(v)"
            />
          </div>
        </div>
      </div>

      <div class="settings-subgroup">{{ t("settings.ai.engine.title") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.active") }}</div>
          </div>
          <div class="setting-row-control w-select">
            <NSelect
              :value="enginesState?.active_engine ?? null"
              :options="sidecarTypeOptions"
              :loading="engineSaving"
              :disabled="!enginesState"
              @update:value="switchEngine"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.provider") }}</div>
          </div>
          <div class="setting-row-control w-select">
            <NSelect
              :value="currentEngine?.provider_id ?? null"
              :options="providerOptions"
              clearable
              :loading="engineSaving"
              :disabled="!enginesState"
              :placeholder="t('settings.ai.engine.providerPlaceholder')"
              @update:value="(v: string | null) => patchEngine({ provider_id: v ?? '' })"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.provider.activeModel") }}</div>
          </div>
          <div class="setting-row-control w-select">
            <NSelect
              :value="currentEngine?.active_model_id || null"
              :options="modelOptions"
              :disabled="modelOptions.length === 0 || engineSaving"
              :placeholder="t('settings.ai.provider.activeModelPlaceholder')"
              @update:value="(v: string | null) => v && patchEngine({ active_model_id: v })"
            />
          </div>
        </div>
        <div v-if="isPiEngine" class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.thinkingLevel") }}</div>
          </div>
          <div class="setting-row-control w-select">
            <NSelect
              :value="currentEngine?.thinking_level || 'off'"
              :options="thinkingLevelOptions"
              :disabled="engineSaving"
              @update:value="(v: string) => patchEngine({ thinking_level: v })"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.contextWindow") }}</div>
            <div class="setting-row-desc">{{ t("settings.ai.engine.tokenFieldHint") }}</div>
          </div>
          <div class="setting-row-control w-num">
            <NInputNumber
              v-model:value="ctxWindowInput"
              :update-value-on-input="false"
              :min="1"
              :step="1000"
              :disabled="engineSaving"
              clearable
              :placeholder="t('settings.ai.engine.tokenFieldPlaceholder')"
              @update:value="(v: number | null) => patchTokenField('context_window', v)"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.maxTokens") }}</div>
            <div class="setting-row-desc">{{ t("settings.ai.engine.tokenFieldHint") }}</div>
          </div>
          <div class="setting-row-control w-num">
            <NInputNumber
              v-model:value="maxTokensInput"
              :update-value-on-input="false"
              :min="1"
              :step="1000"
              :disabled="engineSaving"
              clearable
              :placeholder="t('settings.ai.engine.tokenFieldPlaceholder')"
              @update:value="(v: number | null) => patchTokenField('max_tokens', v)"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.maxRounds") }}</div>
            <div class="setting-row-desc">{{ t("settings.ai.engine.maxRoundsHint") }}</div>
          </div>
          <div class="setting-row-control w-num">
            <NInputNumber
              v-model:value="maxRoundsInput"
              :update-value-on-input="false"
              :min="1"
              :step="1"
              :disabled="engineSaving"
              clearable
              :placeholder="t('settings.ai.engine.tokenFieldPlaceholder')"
              @update:value="(v: number | null) => patchTokenField('max_rounds', v)"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.ai.engine.streaming") }}</div>
            <div class="setting-row-desc">{{ t("settings.ai.engine.streamingHint") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch
              :value="streamingOn"
              :disabled="engineSaving"
              @update:value="(v: boolean) => patchStreaming(v)"
            />
          </div>
        </div>
      </div>
      <p class="settings-hint">{{ t("settings.ai.engine.hint") }}</p>

      <!-- Claude Code 路径（仅非 pi 引擎） -->
      <template v-if="!isPiEngine">
        <div class="settings-block">
          <div class="setting-row">
            <div class="setting-row-main">
              <div class="setting-row-label">{{ t("settings.ai.claudePath") }}</div>
              <div class="setting-row-desc">{{ t("settings.ai.claudePathHint") }}</div>
            </div>
            <div class="setting-row-control w-path">
              <NInput
                v-model:value="claudePath"
                :placeholder="t('settings.ai.claudePathPlaceholder')"
                clearable
                @blur="saveClaudePath"
              />
            </div>
          </div>
          <div class="setting-row">
            <div class="setting-row-main"></div>
            <div class="setting-row-control">
              <NSpace align="center" :size="8">
                <NButton size="small" :loading="detecting" @click="detectClaude">
                  <template #icon>
                    <NIcon><RiSearchLine /></NIcon>
                  </template>
                  {{ t("settings.ai.detect") }}
                </NButton>
                <template v-if="detectedPath !== null">
                  <NTag size="small" type="success" :bordered="false">
                    <template #icon>
                      <NIcon><RiCheckboxCircleLine /></NIcon>
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
                      <NIcon><RiCloseCircleLine /></NIcon>
                    </template>
                    {{ t("settings.ai.notDetected") }}
                  </NTag>
                </template>
              </NSpace>
            </div>
          </div>
        </div>
        <p class="settings-hint">{{ t("settings.ai.configFileHint") }}</p>
      </template>
    </NForm>
  </section>
</template>

<style scoped>
/* 分组标题、页面骨架样式已抽到 src/assets/main.css 统一维护 */

.w-select {
  width: 280px;
}

.w-num {
  width: 140px;
}

.w-path {
  width: 320px;
}
</style>
