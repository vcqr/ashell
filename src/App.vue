<script setup lang="ts">
import { computed, onErrorCaptured, watch } from "vue";
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
  CubeOutline,
} from "@vicons/ionicons5";
import TabBar from "@/components/TabBar.vue";
import HostsDrawer from "@/components/HostsDrawer.vue";
import TerminalView from "@/components/TerminalView.vue";
import AiAssistant from "@/components/AiAssistant.vue";
import AiWindow from "@/components/AiWindow.vue";
import ActivityBar from "@/components/ActivityBar.vue";
import SftpDrawer from "@/components/SftpDrawer.vue";
import SftpWindow from "@/components/SftpWindow.vue";
import WindowControls from "@/components/WindowControls.vue";
import HostInfoDrawer from "@/components/HostInfoDrawer.vue";
import ForwardDrawer from "@/components/ForwardDrawer.vue";
import TemplateDrawer from "@/components/TemplateDrawer.vue";
import SettingsModal from "@/components/settings/SettingsModal.vue";
import AiProvidersModal from "@/components/AiProvidersModal.vue";
import UpdateChecker from "@/components/UpdateChecker.vue";
import LoginGate from "@/components/LoginGate.vue";
import { isTauri } from "@/utils/platform";
import { useKeybindingStore } from "@/stores/keybindings";
import { onBeforeUnmount } from "vue";
import { useApiStore } from "@/stores/api";
import { useTerminalStore } from "@/stores/terminal";
import { useStartupStore } from "@/stores/startup";
import { useTrayStore } from "@/stores/tray";
import { useTheme } from "@/composables/useTheme";
import { useTabs } from "@/composables/useTabs";
import { usePanels } from "@/composables/usePanels";
import { useWindowControls } from "@/composables/useWindowControls";
import { useGlobalShortcuts } from "@/composables/useGlobalShortcuts";

const apiStore = useApiStore();
apiStore.init();

const terminalStore = useTerminalStore();
void terminalStore.loadSystemFonts();

const startupStore = useStartupStore();

// AI daemon 预热的开关门控：启动偏好存于 localStorage，只有前端读得到。
// 开启助手（含启动即开启、设置里后来打开）时才拉起常驻进程；关闭时不预热，
// 不让不用 AI 的用户白担 ~50MB 常驻。首次 spawn 自带懒拉起兜底，此调用仅影响时机。
watch(
  () => startupStore.aiAssistantEnabled,
  (enabled) => {
    if (!enabled || !isTauri) return;
    void import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke("prewarm_ai_daemon").catch(() => {
        /* 预热失败不致命：首次 spawn_sidecar 会再尝试 */
      }),
    );
  },
  { immediate: true },
);

// 托盘设置：启动即加载（顺带把界面语言同步给托盘菜单）
const trayStore = useTrayStore();
void trayStore.load();

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
  activeTerminalTab,
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
  toggleHosts,
  sendCommandToActive,
} = useTabs();

const {
  aiOpen,
  sftpOpen,
  hostInfoOpen,
  forwardOpen,
  templateOpen,
  settingsOpen,
  aiProvidersOpen,
  activityBarVisible,
  toggleAi,
  toggleSftp,
  toggleHostInfo,
  toggleForward,
  toggleTemplate,
  toggleSettings,
  toggleAiProviders,
  toggleActivityBar,
  onSendToAi,
  onSftpSendToAi,
} = usePanels(activeSftpTab, activeAiTab, activeTerminalTab, aiAssistantRef);

// 独立窗口：由 openSftpInNewWindow / openAiInNewWindow 创建，
// URL 带 newwin=1&kind=sftp|ai。走完整 App 实例（providers/api init/theme），
// 但只渲染对应 solo 布局。
const launchParams = new URLSearchParams(window.location.search);
const soloKind = launchParams.get("newwin") === "1" ? launchParams.get("kind") : null;
const soloSftp = soloKind === "sftp";
const soloAi = soloKind === "ai";

const {
  isMac,
  onHeaderDblClick,
} = useWindowControls(terminalRefs, activeTabKey);

// SFTP 面板右键目录 -> 当前终端 cd 到该目录（sendCommand 自动补 \r 执行）
function onSftpOpenTerminalHere(path: string) {
  sendCommandToActive(`cd '${path.replace(/'/g, `'\\''`)}'`);
}

useGlobalShortcuts({
  isMac,
  tabs,
  activeTabKey,
  openLocal,
  closeTab,
  toggleHosts,
  toggleSettings,
  toggleAi,
  toggleSftp,
  toggleAiProviders,
  toggleHostInfo,
  toggleForward,
  toggleTemplate,
  toggleActivityBar,
});

// ── Web 形态浏览器保护 ──
if (!isTauri) {
  const keybindingStore = useKeybindingStore();

  // 1) 有活跃终端会话时，刷新/关闭弹浏览器原生确认（含 Ctrl+W / Cmd+R 触发的场景，
  //    两者无法从 keydown 层面阻止，beforeunload 是唯一防线）
  const onBeforeUnload = (e: BeforeUnloadEvent) => {
    if (
      tabs.value.some((t) => t.status === "connected" || t.status === "connecting")
    ) {
      e.preventDefault();
      e.returnValue = "";
    }
  };
  window.addEventListener("beforeunload", onBeforeUnload);

  // 2) 终端常用但浏览器会抢的组合键：捕获阶段取消浏览器默认行为。
  //    preventDefault 不会阻断事件传播，xterm 与应用自身快捷键照常收到按键。
  //    浏览器硬保留（无法拦截）的 Ctrl/Cmd+W/T/N 由上面的 beforeunload 兜底。
  const GUARD_KEYS = new Set([
    "s", // 保存页面（终端流控 XOFF）
    "p", // 打印
    "o", // 打开文件
    "d", // 书签（终端 EOF/注销）
    "u", // 查看源码（终端删除到行首）
    "r", // 刷新（终端反向搜索；误触刷新会丢全部会话）
    "g", // 快速查找
    "k", // 搜索栏（终端删除到行尾）
    "b", // 书签栏（tmux 前缀）
    "h", // 历史记录（终端退格）
    "f", // 页内查找（应用有终端搜索）
    "j", // 下载列表
    "t", // 新标签（部分浏览器可拦截）
    "n", // 新窗口（同上）
    "w", // 关标签（Firefox 可拦截，Chrome 由 beforeunload 兜底）
  ]);
  const GUARD_FKEYS = new Set(["F1", "F3", "F5", "F6", "F7", "F10"]);
  const onGuardKey = (e: KeyboardEvent) => {
    if (keybindingStore.recording) return;
    if (e.shiftKey) return; // Shift 组合多为浏览器窗口功能，终端场景少用
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (
      (e.ctrlKey || e.metaKey || e.altKey) &&
      (GUARD_KEYS.has(key) || (e.ctrlKey && GUARD_FKEYS.has(key)))
    ) {
      e.preventDefault();
    }
  };
  window.addEventListener("keydown", onGuardKey, { capture: true });

  onBeforeUnmount(() => {
    window.removeEventListener("beforeunload", onBeforeUnload);
    window.removeEventListener("keydown", onGuardKey, { capture: true });
  });
}
</script>

<template>
  <NConfigProvider :theme="naiveTheme" :theme-overrides="themeOverrides" :locale="naiveLocale" :date-locale="naiveDateLocale">
    <NMessageProvider>
      <NDialogProvider>
        <NNotificationProvider>
          <LoginGate>
            <UpdateChecker v-if="isTauri && !soloSftp && !soloAi" />
            <SftpWindow v-if="soloSftp" />
            <AiWindow v-else-if="soloAi" />
            <div
              v-else
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
                <WindowControls v-if="isMac && isTauri" />
                <div v-if="!isMac || !isTauri" class="brand" data-tauri-drag-region>
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
                <WindowControls v-if="!isMac && isTauri" />
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
                :template-open="templateOpen"
                :ai-enabled="startupStore.aiAssistantEnabled"
                :has-active-session="!!activeSftpTab"
                :has-terminal-session="!!activeTerminalTab"
                :has-ai-session="!!activeAiTab"
                @toggle-sftp="toggleSftp"
                @toggle-host-info="toggleHostInfo"
                @toggle-forward="toggleForward"
                @toggle-ai="toggleAi"
                @toggle-template="toggleTemplate"
              />

              <HostsDrawer v-model:open="hostsOpen" @open-host="openHost" />
              <SftpDrawer
                v-model:open="sftpOpen"
                :sid="activeSftpTab?.sid ?? null"
                :host-name="activeSftpTab?.title"
                :host-addr="activeSftpTab?.hostInfo?.addr"
                :host-id="activeSftpTab?.hostId ?? null"
                @send-to-ai="onSftpSendToAi"
                @open-terminal-here="onSftpOpenTerminalHere"
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
              <TemplateDrawer
                v-model:open="templateOpen"
                @run="sendCommandToActive"
              />
              <AiAssistant
                v-if="startupStore.aiAssistantEnabled"
                ref="aiAssistantRef"
                v-model:open="aiOpen"
                :sid="activeAiTab?.sid ?? null"
                :host-name="activeAiTab?.title ?? null"
              />

              <SettingsModal
                v-model:open="settingsOpen"
                v-model:theme-mode="themeMode"
                :resolved-theme="resolvedTheme"
                :theme-title="themeTitle"
              />
              <AiProvidersModal v-model:open="aiProvidersOpen" />
            </div>
          </LoginGate>
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
  border-bottom: 1px solid var(--ashell-border);
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
