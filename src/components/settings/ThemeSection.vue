<script setup lang="ts">
import { ref, watch } from "vue";
import {
  NRadioGroup,
  NRadioButton,
  NIcon,
  NColorPicker,
  NSpace,
  NButton,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  DesktopOutline,
  MoonOutline,
  SunnyOutline,
} from "@vicons/ionicons5";
import {
  TERMINAL_THEME_FIELDS,
  type TerminalThemeName,
} from "@/theme/terminal";
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
        <NButton
          size="small"
          @click="terminalStore.resetTerminalTheme(themeEditTarget)"
        >
          {{ t("settings.theme.resetCurrent", { name: themeEditTarget === "dark" ? t("settings.theme.dark") : t("settings.theme.light") }) }}
        </NButton>
        <NButton size="small" @click="terminalStore.resetTerminalThemes()">
          {{ t("settings.theme.resetAll") }}
        </NButton>
      </NSpace>
    </div>
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

.theme-color-grid {
  margin-top: 4px;
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
</style>
