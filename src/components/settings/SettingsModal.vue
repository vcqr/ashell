<script setup lang="ts">
import { ref } from "vue";
import { NModal, NCard, NButton, NIcon } from "naive-ui";
import { CloseOutline } from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import GeneralSection from "./GeneralSection.vue";
import ThemeSection from "./ThemeSection.vue";
import TerminalSection from "./TerminalSection.vue";
import IconsSection from "./IconsSection.vue";
import StartupSection from "./StartupSection.vue";
import WindowSection from "./WindowSection.vue";
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
  | "window"
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

function close() {
  emit("update:open", false);
}
</script>

<template>
  <NModal :show="open" @update:show="(v: boolean) => emit('update:open', v)">
    <NCard
      style="width: min(900px, 88vw); height: min(680px, 84vh)"
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
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'general' }"
            type="button"
            @click="activeTab = 'general'"
          >
            {{ t("settings.tabs.general") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'theme' }"
            type="button"
            @click="activeTab = 'theme'"
          >
            {{ t("settings.tabs.theme") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'terminal' }"
            type="button"
            @click="activeTab = 'terminal'"
          >
            {{ t("settings.tabs.terminal") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'window' }"
            type="button"
            @click="activeTab = 'window'"
          >
            {{ t("settings.tabs.window") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'icons' }"
            type="button"
            @click="activeTab = 'icons'"
          >
            {{ t("settings.tabs.icons") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'startup' }"
            type="button"
            @click="activeTab = 'startup'"
          >
            {{ t("settings.tabs.startup") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'ai' }"
            type="button"
            @click="activeTab = 'ai'"
          >
            {{ t("settings.tabs.ai") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'security' }"
            type="button"
            @click="activeTab = 'security'"
          >
            {{ t("settings.tabs.security") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'backup' }"
            type="button"
            @click="activeTab = 'backup'"
          >
            {{ t("settings.tabs.backup") }}
          </button>
          <button
            class="settings-tab"
            :class="{ active: activeTab === 'about' }"
            type="button"
            @click="activeTab = 'about'"
          >
            {{ t("settings.tabs.about") }}
          </button>
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
          <WindowSection v-else-if="activeTab === 'window'" />
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
  width: 160px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-right: 12px;
  border-right: 1px solid var(--ashell-border-soft);
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
  padding: 10px 12px;
  text-align: left;
  transition:
    background 0.15s ease,
    color 0.15s ease;
}

.settings-tab:hover {
  background: var(--ashell-hover);
  color: var(--ashell-text);
}

.settings-tab.active {
  background: var(--ashell-active);
  color: var(--ashell-text-strong);
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
