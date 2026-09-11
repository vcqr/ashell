<script setup lang="ts">
import { computed, ref, type Component } from "vue";
import {
  NModal,
  NCard,
  NButton,
  NIcon,
} from "naive-ui";
import {
  CloseOutline,
  OptionsOutline,
  ColorPaletteOutline,
  TerminalOutline,
  KeyOutline,
  BrowsersOutline,
  FileTrayFullOutline,
  AppsOutline,
  RocketOutline,
  SparklesOutline,
  ShieldCheckmarkOutline,
  CloudOutline,
  InformationCircleOutline,
} from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import GeneralSection from "./GeneralSection.vue";
import ThemeSection from "./ThemeSection.vue";
import TerminalSection from "./TerminalSection.vue";
import ShortcutsSection from "./ShortcutsSection.vue";
import IconsSection from "./IconsSection.vue";
import StartupSection from "./StartupSection.vue";
import WindowSection from "./WindowSection.vue";
import TraySection from "./TraySection.vue";
import AiSection from "./AiSection.vue";
import SecuritySection from "./SecuritySection.vue";
import AboutSection from "./AboutSection.vue";
import BackupSection from "./BackupSection.vue";

type ThemeMode = "system" | "dark" | "light";
type ResolvedTheme = "dark" | "light";
type SettingsTab =
  | "general"
  | "theme"
  | "terminal"
  | "shortcuts"
  | "window"
  | "tray"
  | "icons"
  | "startup"
  | "ai"
  | "backup"
  | "security"
  | "about";

defineProps<{
  open: boolean;
  themeMode: ThemeMode;
  resolvedTheme: ResolvedTheme;
  themeTitle: string;
}>();

const emit = defineEmits<{
  (e: "update:open", value: boolean): void;
  (e: "update:themeMode", value: ThemeMode): void;
}>();

const { t } = useI18n();
const activeTab = ref<SettingsTab>("general");

/** 左侧导航按功能域分组：外观/终端/快捷键归"通用"，窗口/托盘/图标/启动归
 *  "系统"，AI 单列，安全/备份归"数据"，关于收尾（无组标题）。 */
const navGroups = computed<
  { label: string; items: { key: SettingsTab; icon: Component }[] }[]
>(() => [
  {
    label: t("settings.groups.general"),
    items: [
      { key: "general", icon: OptionsOutline },
      { key: "theme", icon: ColorPaletteOutline },
      { key: "terminal", icon: TerminalOutline },
      { key: "shortcuts", icon: KeyOutline },
    ],
  },
  {
    label: t("settings.groups.system"),
    items: [
      { key: "window", icon: BrowsersOutline },
      { key: "tray", icon: FileTrayFullOutline },
      { key: "icons", icon: AppsOutline },
      { key: "startup", icon: RocketOutline },
    ],
  },
  {
    label: t("settings.groups.features"),
    items: [{ key: "ai", icon: SparklesOutline }],
  },
  {
    label: t("settings.groups.data"),
    items: [
      { key: "security", icon: ShieldCheckmarkOutline },
      { key: "backup", icon: CloudOutline },
    ],
  },
  {
    label: "",
    items: [{ key: "about", icon: InformationCircleOutline }],
  },
]);

function close() {
  emit("update:open", false);
}
</script>

<template>
  <NModal :show="open" :mask-closable="false" @update:show="(v: boolean) => emit('update:open', v)">
    <NCard
      style="width: min(1060px, 94vw); height: min(800px, 92vh)"
      :title="t('settings.title')"
      size="medium"
      :bordered="false"
      class="settings-card"
      role="dialog"
      aria-modal="true"
    >
      <template #header-extra>
        <NButton
          quaternary
          circle
          size="small"
          :title="t('settings.close')"
          @click="close"
        >
          <template #icon>
            <NIcon><CloseOutline /></NIcon>
          </template>
        </NButton>
      </template>

      <div class="settings-layout">
        <nav class="settings-tabs" :aria-label="t('settings.sections')">
          <template v-for="group in navGroups" :key="group.label || 'tail'">
            <div v-if="group.label" class="settings-group-label">
              {{ group.label }}
            </div>
            <button
              v-for="item in group.items"
              :key="item.key"
              class="settings-tab"
              :class="{ active: activeTab === item.key }"
              type="button"
              @click="activeTab = item.key"
            >
              <NIcon :size="15" class="settings-tab-icon">
                <component :is="item.icon" />
              </NIcon>
              {{ t(`settings.tabs.${item.key}`) }}
            </button>
          </template>
        </nav>

        <div class="settings-content">
          <GeneralSection v-if="activeTab === 'general'" />
          <ThemeSection
            v-else-if="activeTab === 'theme'"
            :theme-mode="themeMode"
            :resolved-theme="resolvedTheme"
            :theme-title="themeTitle"
            @update:theme-mode="(v: ThemeMode) => emit('update:themeMode', v)"
          />
          <TerminalSection v-else-if="activeTab === 'terminal'" />
          <ShortcutsSection v-else-if="activeTab === 'shortcuts'" />
          <WindowSection v-else-if="activeTab === 'window'" />
          <TraySection v-else-if="activeTab === 'tray'" />
          <IconsSection v-else-if="activeTab === 'icons'" />
          <StartupSection v-else-if="activeTab === 'startup'" />
          <AiSection v-else-if="activeTab === 'ai'" />
          <SecuritySection v-else-if="activeTab === 'security'" />
          <BackupSection v-else-if="activeTab === 'backup'" />
          <AboutSection v-else-if="activeTab === 'about'" />
        </div>
      </div>
    </NCard>
  </NModal>
</template>

<style scoped>
.settings-layout {
  display: flex;
  gap: 24px;
  flex: 1;
  min-height: 0;
}

.settings-tabs {
  width: 168px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-right: 12px;
  border-right: 1px solid var(--ashell-border-soft);
  overflow-y: auto;
}

.settings-group-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.5px;
  color: var(--ashell-text-subtle);
  padding: 9px 12px 4px;
  user-select: none;
}
.settings-group-label:first-child {
  padding-top: 2px;
}

.settings-tab {
  width: 100%;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--ashell-text-muted);
  cursor: pointer;
  font: inherit;
  font-size: 14px;
  padding: 8px 12px;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 9px;
  transition:
    background 0.15s ease,
    color 0.15s ease;
}

.settings-tab-icon {
  flex-shrink: 0;
  opacity: 0.75;
}

.settings-tab:hover {
  background: var(--ashell-hover);
  color: var(--ashell-text);
}

.settings-tab.active {
  background: var(--ashell-active);
  color: var(--ashell-text-strong);
}
.settings-tab.active .settings-tab-icon {
  opacity: 1;
}

.settings-content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  padding-right: 4px;
}
</style>

<style>
/* Not scoped: NModal clones the NCard vnode and re-renders it inside its own
   BodyWrapper, which strips this component's scope id from .settings-card.
   See memory: scoped-style-on-child-root-unreliable. */
.settings-card .n-card-content {
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
