<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  NRadioGroup,
  NRadioButton,
  NIcon,
  NColorPicker,
  NSpace,
  NButton,
  NModal,
  NCard,
  NInput,
  NPopconfirm,
  NForm,
  NFormItem,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  DesktopOutline,
  MoonOutline,
  SunnyOutline,
  DownloadOutline,
  ShareOutline,
  CloseOutline,
} from "@vicons/ionicons5";
import {
  TERMINAL_THEME_FIELDS,
  type TerminalThemeName,
} from "@/theme/terminal";
import {
  presetsByVariant,
  type TerminalThemePreset,
} from "@/theme/terminalPresets";
import {
  guessVariant,
  parseTerminalThemeInput,
  toSchemeJson,
  terminalThemeSignature,
} from "@/theme/terminalImport";
import { useTerminalStore } from "@/stores/terminal";

type ThemeMode = "system" | "dark" | "light";
type ResolvedTheme = "dark" | "light";

const props = defineProps<{
  themeMode: ThemeMode;
  resolvedTheme: ResolvedTheme;
  themeTitle: string;
}>();

const emit = defineEmits<{
  (e: "update:themeMode", value: ThemeMode): void;
}>();

const terminalStore = useTerminalStore();
const { t } = useI18n();
const message = useMessage();
const themeEditTarget = ref<TerminalThemeName>(props.resolvedTheme);

watch(
  () => props.resolvedTheme,
  (v) => {
    themeEditTarget.value = v;
  },
);

function setMode(v: ThemeMode) {
  emit("update:themeMode", v);
}

/* ---------------- 主题预设 ---------------- */

/** 预设跟随当前编辑目标：编辑深色时只展示深色预设，反之亦然。 */
const visiblePresets = computed(() => presetsByVariant(themeEditTarget.value));

const currentThemeSignature = computed(() =>
  terminalThemeSignature(
    themeEditTarget.value === "dark"
      ? terminalStore.darkTheme
      : terminalStore.lightTheme,
  ),
);

function isThemeActive(theme: TerminalThemePreset["theme"]): boolean {
  return terminalThemeSignature(theme) === currentThemeSignature.value;
}

function swatchColors(theme: TerminalThemePreset["theme"]): string[] {
  return ["red", "green", "yellow", "blue", "magenta", "cyan"].map(
    (k) => String(theme[k as keyof typeof theme] ?? "#888"),
  );
}

function applyPreset(preset: TerminalThemePreset) {
  terminalStore.applyPresetTheme(preset, themeEditTarget.value);
}

/* ---------------- 自定义主题 ---------------- */

const customCards = computed(() =>
  terminalStore.customThemes.map((item) => ({
    ...item,
    variant: guessVariant(item.theme),
  })),
);

function applyCustom(item: { id: string; name: string; theme: TerminalThemePreset["theme"] }) {
  // 自定义主题按自身明暗归属写入对应槽位，并把编辑目标切过去
  const variant = guessVariant(item.theme);
  terminalStore.applyPresetTheme(
    { id: item.id, name: item.name, variant, theme: item.theme },
    variant,
  );
  themeEditTarget.value = variant;
}

function removeCustom(id: string) {
  terminalStore.removeCustomTheme(id);
}

/* ---------------- 导入 ---------------- */

const importShow = ref(false);
const importName = ref("");
const importContent = ref("");
const fileInput = ref<HTMLInputElement | null>(null);

function openImport() {
  importName.value = "";
  importContent.value = "";
  importShow.value = true;
}

async function onPickFile(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  try {
    const text = await file.text();
    const parsed = parseTerminalThemeInput(text, file.name);
    // 统一回填规范化 JSON，确认时走同一条解析路径
    importContent.value = toSchemeJson(parsed.name, parsed.theme);
    if (!importName.value.trim()) importName.value = parsed.name;
  } catch (err) {
    message.error(
      t("settings.theme.importFailed", {
        reason: err instanceof Error ? err.message : String(err),
      }),
    );
  }
}

function confirmImport() {
  try {
    const parsed = parseTerminalThemeInput(importContent.value, undefined, importName.value);
    // 按导入主题自身的明暗归属写入对应槽位，并把编辑目标切换过去
    terminalStore.addCustomTheme(parsed.name, parsed.theme, parsed.variant);
    themeEditTarget.value = parsed.variant;
    message.success(t("settings.theme.importSuccess", { name: parsed.name }));
    importShow.value = false;
  } catch (err) {
    message.error(
      t("settings.theme.importFailed", {
        reason: err instanceof Error ? err.message : String(err),
      }),
    );
  }
}

/* ---------------- 导出 ---------------- */

async function exportCurrent() {
  const name = `Ashell ${themeEditTarget.value === "dark" ? t("settings.theme.dark") : t("settings.theme.light")}`;
  const json = terminalStore.exportThemeJson(name, themeEditTarget.value);
  try {
    await navigator.clipboard.writeText(json);
    message.success(t("settings.theme.exportSuccess"));
  } catch {
    message.error(t("settings.theme.exportFailed"));
  }
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.theme.appearance") }}</div>
    <NRadioGroup
      :value="themeMode"
      name="theme-mode"
      @update:value="setMode"
    >
      <NRadioButton value="system">
        <span class="theme-option">
          <NIcon :size="15"><DesktopOutline /></NIcon>
          {{ t("settings.theme.followSystem") }}
        </span>
      </NRadioButton>
      <NRadioButton value="dark">
        <span class="theme-option">
          <NIcon :size="15"><MoonOutline /></NIcon>
          {{ t("settings.theme.dark") }}
        </span>
      </NRadioButton>
      <NRadioButton value="light">
        <span class="theme-option">
          <NIcon :size="15"><SunnyOutline /></NIcon>
          {{ t("settings.theme.light") }}
        </span>
      </NRadioButton>
    </NRadioGroup>
    <div class="settings-hint">{{ themeTitle }}</div>

    <div class="terminal-theme-block">
      <div class="settings-section-title">{{ t("settings.theme.terminalColors") }}</div>
      <NRadioGroup v-model:value="themeEditTarget" size="small">
        <NRadioButton value="dark">{{ t("settings.theme.darkTheme") }}</NRadioButton>
        <NRadioButton value="light">{{ t("settings.theme.lightTheme") }}</NRadioButton>
      </NRadioGroup>
      <div class="settings-hint">
        {{ t("settings.theme.editingHint", { name: themeEditTarget === "dark" ? t("settings.theme.dark") : t("settings.theme.light") }) }}
      </div>

      <div class="preset-head">
        <div>
          <div class="settings-subgroup">{{ t("settings.theme.presets") }}</div>
          <div class="settings-hint">{{ t("settings.theme.presetsSourceHint") }}</div>
        </div>
        <NSpace :size="8">
          <NButton size="small" @click="openImport">
            <template #icon>
              <NIcon :size="14"><DownloadOutline /></NIcon>
            </template>
            {{ t("settings.theme.importTheme") }}
          </NButton>
          <NButton size="small" @click="exportCurrent">
            <template #icon>
              <NIcon :size="14"><ShareOutline /></NIcon>
            </template>
            {{ t("settings.theme.exportTheme") }}
          </NButton>
        </NSpace>
      </div>

      <div class="preset-grid">
        <button
          v-for="preset in visiblePresets"
          :key="preset.id"
          type="button"
          class="preset-card"
          :class="{ active: isThemeActive(preset.theme) }"
          :title="preset.name"
          @click="applyPreset(preset)"
        >
          <span
            class="preset-swatch"
            :style="{ background: preset.theme.background }"
          >
            <span
              class="preset-swatch-text"
              :style="{ color: preset.theme.foreground }"
            >$ ssh host</span>
            <span class="preset-swatch-dots">
              <i
                v-for="(c, i) in swatchColors(preset.theme)"
                :key="i"
                :style="{ background: c }"
              />
            </span>
          </span>
          <span class="preset-name">{{ preset.name }}</span>
        </button>
      </div>

      <template v-if="customCards.length > 0">
        <div class="settings-subgroup" style="margin-top: 6px">
          {{ t("settings.theme.customThemes") }}
        </div>
        <div class="preset-grid">
          <div
            v-for="item in customCards"
            :key="item.id"
            class="preset-card custom"
            :class="{ active: isThemeActive(item.theme) }"
            :title="item.name"
          >
            <button type="button" class="preset-card-main" @click="applyCustom(item)">
              <span
                class="preset-swatch"
                :style="{ background: item.theme.background }"
              >
                <span
                  class="preset-swatch-text"
                  :style="{ color: item.theme.foreground }"
                >$ ssh host</span>
                <span class="preset-swatch-dots">
                  <i
                    v-for="(c, i) in swatchColors(item.theme)"
                    :key="i"
                    :style="{ background: c }"
                  />
                </span>
              </span>
            </button>
            <span class="preset-name">
              <span class="preset-name-text">{{ item.name }}</span>
              <span class="preset-variant">
                {{ item.variant === "dark" ? t("settings.theme.dark") : t("settings.theme.light") }}
              </span>
            </span>
            <NPopconfirm
              @positive-click="removeCustom(item.id)"
            >
              <template #trigger>
                <NButton
                  class="preset-delete"
                  quaternary
                  circle
                  size="tiny"
                  :title="t('settings.theme.deleteTheme')"
                >
                  <template #icon>
                    <NIcon :size="12"><CloseOutline /></NIcon>
                  </template>
                </NButton>
              </template>
              {{ t("settings.theme.deleteThemeConfirm", { name: item.name }) }}
            </NPopconfirm>
          </div>
        </div>
      </template>

      <div class="theme-color-grid">
        <div
          v-for="field in TERMINAL_THEME_FIELDS"
          :key="field.key"
          class="theme-color-item"
        >
          <NColorPicker
            :value="
              (themeEditTarget === 'dark'
                ? terminalStore.darkTheme
                : terminalStore.lightTheme)[field.key] ?? ''
            "
            :modes="['hex', 'rgb', 'hsl']"
            size="small"
            @update:value="
              (v: string) => {
                const target =
                  themeEditTarget === 'dark'
                    ? terminalStore.darkTheme
                    : terminalStore.lightTheme;
                target[field.key] = v;
              }
            "
          />
          <span class="theme-color-label">{{ t(field.label) }}</span>
        </div>
      </div>
      <NSpace>
        <NPopconfirm @positive-click="terminalStore.resetTerminalTheme(themeEditTarget)">
          <template #trigger>
            <NButton size="small">
              {{ t("settings.theme.resetCurrent", { name: themeEditTarget === "dark" ? t("settings.theme.dark") : t("settings.theme.light") }) }}
            </NButton>
          </template>
          {{ t("settings.theme.resetCurrentConfirm") }}
        </NPopconfirm>
        <NPopconfirm @positive-click="terminalStore.resetTerminalThemes()">
          <template #trigger>
            <NButton size="small">
              {{ t("settings.theme.resetAll") }}
            </NButton>
          </template>
          {{ t("settings.theme.resetAllConfirm") }}
        </NPopconfirm>
      </NSpace>
    </div>

    <NModal
      v-model:show="importShow"
      :auto-focus="false"
      transform-origin="center"
    >
      <NCard
        style="width: min(560px, 90vw)"
        :title="t('settings.theme.importTitle')"
        size="small"
        :bordered="false"
        role="dialog"
        aria-modal="true"
      >
        <template #header-extra>
          <NButton quaternary circle size="small" @click="importShow = false">
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </template>
        <NForm label-placement="top" size="small" :show-feedback="false">
          <NFormItem :label="t('settings.theme.importName')">
            <NInput
              v-model:value="importName"
              :placeholder="t('settings.theme.importNamePlaceholder')"
              clearable
            />
          </NFormItem>
          <NFormItem :label="t('settings.theme.importContent')">
            <NInput
              v-model:value="importContent"
              type="textarea"
              :rows="8"
              :placeholder="t('settings.theme.importContentPlaceholder')"
              class="import-content-input"
            />
          </NFormItem>
        </NForm>
        <p class="settings-hint">{{ t("settings.theme.importFileHint") }}</p>
        <NSpace justify="end" style="margin-top: 12px">
          <NButton size="small" @click="fileInput?.click()">
            <template #icon>
              <NIcon :size="14"><DownloadOutline /></NIcon>
            </template>
            {{ t("settings.theme.importPickFile") }}
          </NButton>
          <NButton
            size="small"
            type="primary"
            :disabled="!importContent.trim()"
            @click="confirmImport"
          >
            {{ t("settings.theme.importApply") }}
          </NButton>
        </NSpace>
      </NCard>
    </NModal>
    <input
      ref="fileInput"
      type="file"
      accept=".itermcolors,.json,.txt"
      class="hidden-file-input"
      @change="onPickFile"
    />
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

.settings-subgroup {
  font-size: 12px;
  font-weight: 500;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
}

.settings-hint {
  color: var(--ashell-text-subtle);
  font-size: 12px;
}

.theme-option {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.terminal-theme-block {
  margin-top: 16px;
  padding-top: 16px;
  border-top: 1px solid var(--ashell-border-soft);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.preset-head {
  margin-top: 4px;
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.preset-grid {
  margin-top: 2px;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
  gap: 10px;
}

.preset-card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 0;
  padding: 0;
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
  background: transparent;
  cursor: pointer;
  overflow: hidden;
  font: inherit;
  text-align: left;
  transition:
    border-color 0.15s ease,
    box-shadow 0.15s ease,
    transform 0.1s ease;
}

.preset-card:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.18);
}

.preset-card.active {
  border-color: var(--ashell-primary, #7c5cff);
  box-shadow: 0 0 0 1px var(--ashell-primary, #7c5cff);
}

.preset-card.custom {
  cursor: default;
}

.preset-card-main {
  all: unset;
  display: block;
  cursor: pointer;
}

.preset-swatch {
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  height: 52px;
  padding: 7px 9px;
  border-bottom: 1px solid var(--ashell-border-soft);
}

.preset-swatch-text {
  font-family: "Fira Code", "JetBrains Mono", Menlo, Consolas, monospace;
  font-size: 11px;
  line-height: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.preset-swatch-dots {
  display: flex;
  gap: 4px;
}

.preset-swatch-dots i {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}

.preset-name {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 9px;
  font-size: 12px;
  line-height: 1.2;
  color: var(--ashell-text-muted);
  min-width: 0;
}

.preset-name-text {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.preset-variant {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--ashell-hover, rgba(128, 128, 128, 0.15));
  color: var(--ashell-text-subtle);
}

.preset-delete {
  position: absolute;
  top: 4px;
  right: 4px;
  background: rgba(0, 0, 0, 0.35);
  color: #fff;
}

.theme-color-grid {
  margin-top: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--ashell-border-soft);
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 12px 10px;
  align-items: start;
}

.theme-color-item {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 6px;
  min-width: 0;
}

.theme-color-item :deep(.n-color-picker) {
  width: 100%;
}

.theme-color-item :deep(.n-color-picker-trigger) {
  height: 28px;
}

.theme-color-label {
  font-size: 12px;
  line-height: 1.2;
  color: var(--ashell-text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.hidden-file-input {
  display: none;
}

.import-content-input :deep(textarea) {
  font-family: "Fira Code", "JetBrains Mono", Menlo, Consolas, monospace;
  font-size: 12px;
}
</style>
