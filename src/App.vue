<script setup lang="ts">
import { computed, onErrorCaptured } from "vue";
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  NNotificationProvider,
  NIcon,
  NButton,
  NTooltip,
  NSpace,
  zhCN,
  enUS,
  dateZhCN,
  dateEnUS,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { currentLocale } from "@/locales";
import {
  TerminalOutline,
  MenuOutline,
  SettingsOutline,
  GridOutline,
  CloseOutline,
  RemoveOutline,
  SquareOutline,
  CopyOutline,
  AddOutline,
  CubeOutline,
} from "@vicons/ionicons5";
import TabBar from "@/components/TabBar.vue";
import HostsDrawer from "@/components/HostsDrawer.vue";
import TerminalView from "@/components/TerminalView.vue";
import AiAssistant from "@/components/AiAssistant.vue";
import ActivityBar from "@/components/ActivityBar.vue";
import SftpDrawer from "@/components/SftpDrawer.vue";
import HostInfoDrawer from "@/components/HostInfoDrawer.vue";
import ForwardDrawer from "@/components/ForwardDrawer.vue";
import SettingsModal from "@/components/settings/SettingsModal.vue";
import AiProvidersModal from "@/components/AiProvidersModal.vue";
import UpdateChecker from "@/components/UpdateChecker.vue";
import { useApiStore } from "@/stores/api";
import { useTerminalStore } from "@/stores/terminal";
import { useStartupStore } from "@/stores/startup";
import { useTheme } from "@/composables/useTheme";
import { useTabs } from "@/composables/useTabs";
import { usePanels } from "@/composables/usePanels";
import { useWindowControls } from "@/composables/useWindowControls";

const apiStore = useApiStore();
apiStore.init();

const terminalStore = useTerminalStore();
void terminalStore.loadSystemFonts();

const startupStore = useStartupStore();

// 全局错误边界：捕获子组件未处理的异常，避免白屏
onErrorCaptured((err, _instance, info) => {
  console.error("[AShell] uncaught component error:", err, "\ncomponent:", _instance?.$options?.name ?? "anonymous", "\ninfo:", info);
  return false;
});

const { t } = useI18n();

const naiveLocale = computed(() =>
  currentLocale.value === "zh-CN" ? zhCN : enUS,
);
const naiveDateLocale = computed(() =>
  currentLocale.value === "zh-CN" ? dateZhCN : dateEnUS,
);

const { themeMode, resolvedTheme, naiveTheme, themeOverrides, themeTitle } =
  useTheme();

const {
  hostsOpen,
  tabs,
  activeTabKey,
  activeSftpTab,
  activeAiTab,
  restoredTabKeys,
  terminalRefs,
  aiAssistantRef,
  setTerminalRef,
  openHost,
  openLocal,
  onTabBarNew,
  closeTab,
  reorderTabs,
  reconnectTab,
  disconnectTab,
  getSessionContent,
  duplicateTab,
  openInNewWindow,
  renameTab,
  closeOtherTabs,
  closeLeftTabs,
  closeRightTabs,
  onSidReady,
  onStatusChange,
  onTitleChange,
  closeHostsIfOpen,
} = useTabs();

const {
  aiOpen,
  sftpOpen,
  hostInfoOpen,
  forwardOpen,
  settingsOpen,
  aiProvidersOpen,
  activityBarVisible,
  toggleAi,
  toggleSftp,
  toggleHostInfo,
  toggleForward,
  onSendToAi,
  onSftpSendToAi,
} = usePanels(activeSftpTab, activeAiTab, aiAssistantRef);

const {
  isMaximized,
  isMac,
  minimizeWindow,
  toggleMaximize,
  closeWindow,
  onHeaderDblClick,
} = useWindowControls(terminalRefs, activeTabKey);
</script>

<template>
  <NConfigProvider :theme="naiveTheme" :theme-overrides="themeOverrides" :locale="naiveLocale" :date-locale="naiveDateLocale">
    <NMessageProvider>
      <NDialogProvider>
        <NNotificationProvider>
          <UpdateChecker />
          <div
            class="app-root"
            :style="{
              '--ashell-activity-w':
                tabs.length > 0 && activityBarVisible ? '44px' : '0px',
            }"
          >
            <div
              v-if="terminalStore.wallpaperUrl"
              class="wallpaper-layer"
              :style="{
                backgroundImage: `url(${terminalStore.wallpaperUrl})`,
                opacity: 'var(--ashell-wallpaper-opacity, 1)',
              }"
            />
            <header class="app-header" data-tauri-drag-region @dblclick="onHeaderDblClick">
              <div
                v-if="isMac"
                class="window-controls is-mac"
                data-tauri-drag-region="false"
              >
                <button
                  class="window-control mac-close"
                  type="button"
                  :title="t('app.closeWindow')"
                  @click="closeWindow"
                >
                  <NIcon :size="10"><CloseOutline /></NIcon>
                </button>
                <button
                  class="window-control mac-min"
                  type="button"
                  :title="t('app.minimize')"
                  @click="minimizeWindow"
                >
                  <NIcon :size="10"><RemoveOutline /></NIcon>
                </button>
                <button
                  class="window-control mac-max"
                  type="button"
                  :title="isMaximized ? t('app.restore') : t('app.maximize')"
                  @click="toggleMaximize"
                >
                  <NIcon :size="10">
                    <CopyOutline v-if="isMaximized" />
                    <AddOutline v-else />
                  </NIcon>
                </button>
              </div>
              <div v-if="!isMac" class="brand" data-tauri-drag-region>
                <div class="brand-logo">
                  <img src="/icon.png" alt="AShell" />
                </div>
                <span class="brand-name">AShell</span>
              </div>
              <NButton
                quaternary
                circle
                class="collapse-btn"
                data-tauri-drag-region="false"
                :title="hostsOpen ? t('app.hideHosts') : t('app.showHosts')"
                @click="hostsOpen = !hostsOpen"
              >
                <template #icon>
                  <NIcon :size="18"><MenuOutline /></NIcon>
                </template>
              </NButton>
              <div class="header-divider" />
              <div class="tabs-wrap" data-tauri-drag-region="false">
                <TabBar
                  :tabs="tabs"
                  :active-key="activeTabKey"
                  :get-session-content="getSessionContent"
                  @update:active-key="(k: string) => (activeTabKey = k)"
                  @close="closeTab"
                  @new="onTabBarNew"
                  @reorder="reorderTabs"
                  @reconnect="reconnectTab"
                  @disconnect="disconnectTab"
                  @duplicate="duplicateTab"
                  @rename="renameTab"
                  @close-others="closeOtherTabs"
                  @close-left="closeLeftTabs"
                  @close-right="closeRightTabs"
                  @open-in-new-window="openInNewWindow"
                />
              </div>
              <nav class="drag-spacer" data-tauri-drag-region />
              <NSpace
                :size="8"
                align="center"
                class="header-actions"
                data-tauri-drag-region="false"
              >
                <NTooltip v-if="tabs.length > 0">
                  <template #trigger>
                    <NButton
                      circle
                      quaternary
                      :type="activityBarVisible ? 'primary' : 'default'"
                      @click="activityBarVisible = !activityBarVisible"
                    >
                      <template #icon>
                        <NIcon :size="18"><GridOutline /></NIcon>
                      </template>
                    </NButton>
                  </template>
                  {{ activityBarVisible ? t("app.hideSidebar") : t("app.showSidebar") }}
                </NTooltip>
                <NTooltip>
                  <template #trigger>
                    <NButton circle quaternary @click="aiProvidersOpen = true">
                      <template #icon>
                        <NIcon :size="18"><CubeOutline /></NIcon>
                      </template>
                    </NButton>
                  </template>
                  {{ t("settings.ai.provider.title") }}
                </NTooltip>
                <NTooltip>
                  <template #trigger>
                    <NButton circle quaternary @click="settingsOpen = true">
                      <template #icon>
                        <NIcon :size="18"><SettingsOutline /></NIcon>
                      </template>
                    </NButton>
                  </template>
                  {{ t("app.settings") }}
                </NTooltip>
              </NSpace>
              <div
                v-if="!isMac"
                class="window-controls"
                data-tauri-drag-region="false"
              >
                <button
                  class="window-control"
                  type="button"
                  :title="t('app.minimize')"
                  @click="minimizeWindow"
                >
                  <NIcon :size="14"><RemoveOutline /></NIcon>
                </button>
                <button
                  class="window-control"
                  type="button"
                  :title="isMaximized ? t('app.restore') : t('app.maximize')"
                  @click="toggleMaximize"
                >
                  <NIcon :size="13">
                    <CopyOutline v-if="isMaximized" />
                    <SquareOutline v-else />
                  </NIcon>
                </button>
                <button
                  class="window-control window-control-close"
                  type="button"
                  :title="t('app.closeWindow')"
                  @click="closeWindow"
                >
                  <NIcon :size="14"><CloseOutline /></NIcon>
                </button>
              </div>
            </header>

            <div
              class="app-content"
              :style="{
                top: 'var(--ashell-header-h)',
                left: 0,
                right: 'var(--ashell-activity-w, 0px)',
                bottom: 0,
              }"
              @mousedown="closeHostsIfOpen"
            >
              <TerminalView
                v-for="tab in tabs"
                v-show="tab.key === activeTabKey"
                :key="tab.key"
                :ref="(el) => setTerminalRef(tab.key, el)"
                :tab="tab"
                :active="tab.key === activeTabKey"
                :auto-connect="
                  !restoredTabKeys.has(tab.key) ||
                  startupStore.autoConnectRememberedTabs
                "
                @sid-ready="onSidReady"
                @status-change="onStatusChange"
                @title-change="onTitleChange"
                @send-to-ai="onSendToAi"
                @close-tab="closeTab"
              />
              <div v-if="tabs.length === 0" class="empty-state">
                <NIcon :size="48" depth="3"><TerminalOutline /></NIcon>
                <p>{{ t("app.emptyState.title") }}</p>
                <NSpace :size="12">
                  <NButton tertiary @click="hostsOpen = true">
                    {{ t("app.emptyState.openHosts") }}
                  </NButton>
                  <NButton tertiary @click="openLocal()"> {{ t("app.emptyState.openLocal") }} </NButton>
                </NSpace>
              </div>
            </div>

            <ActivityBar
              v-if="tabs.length > 0 && activityBarVisible"
              :tabs="tabs"
              :active-key="activeTabKey"
              :sftp-open="sftpOpen"
              :host-info-open="hostInfoOpen"
              :forward-open="forwardOpen"
              :ai-open="aiOpen"
              :has-active-session="!!activeSftpTab"
              :has-ai-session="!!activeAiTab"
              @toggle-sftp="toggleSftp"
              @toggle-host-info="toggleHostInfo"
              @toggle-forward="toggleForward"
              @toggle-ai="toggleAi"
            />

            <HostsDrawer v-model:open="hostsOpen" @open-host="openHost" />
            <SftpDrawer
              v-model:open="sftpOpen"
              :sid="activeSftpTab?.sid ?? null"
              :host-name="activeSftpTab?.title"
              :host-addr="activeSftpTab?.hostInfo?.addr"
              @send-to-ai="onSftpSendToAi"
            />
            <HostInfoDrawer
              v-model:open="hostInfoOpen"
              :sid="activeSftpTab?.sid ?? null"
              :host-name="activeSftpTab?.title"
              :host-icon="activeSftpTab?.icon ?? null"
              :host-info="activeSftpTab?.hostInfo"
            />
            <ForwardDrawer
              v-model:open="forwardOpen"
              :sid="activeSftpTab?.sid ?? null"
              :host-name="activeSftpTab?.title"
            />
            <AiAssistant
              ref="aiAssistantRef"
              v-model:open="aiOpen"
              :sid="activeAiTab?.sid ?? null"
            />

            <SettingsModal
              v-model:open="settingsOpen"
              v-model:theme-mode="themeMode"
              :resolved-theme="resolvedTheme"
              :theme-title="themeTitle"
            />
            <AiProvidersModal v-model:open="aiProvidersOpen" />
          </div>
        </NNotificationProvider>
      </NDialogProvider>
    </NMessageProvider>
  </NConfigProvider>
</template>

<style scoped>
.app-root {
  height: 100vh;
  width: 100vw;
  background: var(--ashell-bg);
  position: relative;
}

.wallpaper-layer {
  position: absolute;
  inset: 0;
  background-size: cover;
  background-position: center;
  background-repeat: no-repeat;
  z-index: 0;
  pointer-events: none;
}

.app-header {
  height: var(--ashell-header-h);
  display: flex;
  align-items: center;
  padding: 0 12px;
  gap: 8px;
  position: relative;
  z-index: 1;
  background: linear-gradient(
    180deg,
    var(--ashell-header-start) 0%,
    var(--ashell-header-end) 100%
  );
  z-index: 10;
}

.collapse-btn {
  color: var(--ashell-text-muted);
  flex-shrink: 0;
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 0 4px;
}

.brand-logo {
  width: 24px;
  height: 24px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.brand-logo img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.brand-name {
  font-weight: 600;
  font-size: 14px;
  color: var(--ashell-text-strong);
  letter-spacing: 0.5px;
}

.header-divider {
  width: 1px;
  height: 20px;
  background: var(--ashell-border);
  flex-shrink: 0;
  margin: 0 4px;
}

.tabs-wrap {
  flex: 0 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  overflow: hidden;
}

.drag-spacer {
  flex: 1;
  height: 100%;
  min-width: 0;
  -webkit-app-region: no-drag;
}

.header-actions {
  flex-shrink: 0;
}

.window-controls {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  margin-left: 8px;
  height: 100%;
  -webkit-app-region: no-drag;
}

.window-control {
  width: 40px;
  height: var(--ashell-header-h);
  border: 0;
  background: transparent;
  color: var(--ashell-text-muted);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 0;
  transition:
    background 0.12s ease,
    color 0.12s ease;
}

.window-control:hover {
  background: rgba(255, 255, 255, 0.08);
  color: var(--ashell-text-strong);
}

.window-control:active {
  background: rgba(255, 255, 255, 0.12);
}

.window-control-close:hover {
  background: #e81123;
  color: #fff;
}

/* macOS traffic light style */
.window-controls.is-mac {
  margin-left: 4px;
  margin-right: 8px;
  gap: 8px;
  padding: 0 4px;
}

.window-controls.is-mac .window-control {
  width: 12px;
  height: 12px;
  min-width: 12px;
  border-radius: 50%;
  padding: 0;
  background: var(--ashell-mac-dot-idle, rgba(255, 255, 255, 0.16));
  color: rgba(0, 0, 0, 0.55);
  border: 0.5px solid rgba(0, 0, 0, 0.18);
  transition:
    background 0.15s ease,
    color 0.15s ease,
    transform 0.1s ease;
}

.window-controls.is-mac .window-control :deep(.n-icon) {
  opacity: 0;
  transition: opacity 0.12s ease;
}

.window-controls.is-mac:hover .window-control :deep(.n-icon) {
  opacity: 1;
}

.window-controls.is-mac .mac-close {
  background: #ff5f57;
}
.window-controls.is-mac .mac-min {
  background: #febc2e;
}
.window-controls.is-mac .mac-max {
  background: #28c840;
}

.window-controls.is-mac .window-control:hover {
  filter: brightness(1.05);
}

.window-controls.is-mac .window-control:active {
  filter: brightness(0.9);
  background: var(--ashell-mac-dot-active, currentColor);
}

.window-controls.is-mac .window-control-close:hover {
  background: #ff5f57;
  color: rgba(0, 0, 0, 0.6);
}

.app-content {
  position: absolute;
  overflow: auto;
  background: transparent;
  z-index: 1;
}

.empty-state {
  position: fixed;
  top: var(--ashell-header-h);
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  color: var(--ashell-text-muted);
  padding-left: var(--ashell-hosts-width, 0px);
  text-align: center;
  transition: padding-left 0.25s ease;
  pointer-events: none;
}

.empty-state > * {
  pointer-events: auto;
}
</style>
