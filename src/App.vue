<script setup lang="ts">
import { computed, onErrorCaptured, provide, ref, watch, watchEffect } from "vue";
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
} from "@vicons/ionicons5";
import TabBar from "@/components/TabBar.vue";
import HostsDrawer from "@/components/HostsDrawer.vue";
import TerminalView from "@/components/TerminalView.vue";
import AiAssistant from "@/components/AiAssistant.vue";
import AiWindow from "@/components/AiWindow.vue";
import ActivityBar from "@/components/ActivityBar.vue";
import SftpDrawer from "@/components/SftpDrawer.vue";
import SftpWindow from "@/components/SftpWindow.vue";
import LocalFilesDrawer from "@/components/LocalFilesDrawer.vue";
import WindowControls from "@/components/WindowControls.vue";
import HostInfoDrawer from "@/components/HostInfoDrawer.vue";
import ForwardDrawer from "@/components/ForwardDrawer.vue";
import TemplateDrawer from "@/components/TemplateDrawer.vue";
import SettingsModal from "@/components/settings/SettingsModal.vue";
import UpdateChecker from "@/components/UpdateChecker.vue";
import LoginGate from "@/components/LoginGate.vue";
import { isTauri } from "@/utils/platform";
import { installDevtoolsShortcut } from "@/utils/devtools";
import { useKeybindingStore } from "@/stores/keybindings";
import { onBeforeUnmount } from "vue";
import { useApiStore } from "@/stores/api";
import { useTerminalStore } from "@/stores/terminal";
import { useStartupStore } from "@/stores/startup";
import { useTrayStore } from "@/stores/tray";
import { useTheme } from "@/composables/useTheme";
import {
  useTabs,
  HOST_SESSION_STATUS_KEY,
  type SplitDir,
  type SplitPaneIdx,
} from "@/composables/useTabs";
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
  activeLocalTab,
  hasLocalTab,
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
  onCwdChange,
  closeHostsIfOpen,
  toggleHosts,
  sendCommandToActive,
  hostSessionStatus,
  splitEnabled,
  splitDir,
  splitGrid,
  splitRatio,
  splitRatio2,
  splitPaneKeys,
  activateTab,
  focusPane,
  focusNextPane,
  disableSplit,
  splitTab,
  splitGridTab,
} = useTabs();

// 主机会话状态下发给主机树/卡片视图(状态点);HostsDrawer 中间层无需透传
provide(HOST_SESSION_STATUS_KEY, hostSessionStatus);

// ── 分屏布局（split view）──
// 每个包一层 .terminal-pane 绝对定位容器：未分屏时占满整个内容区（等价于
// 历史上 TerminalView 直接作为 .app-content 子元素），分屏时按方向与比例切分
// （双格 = 单条分隔条；四格 = 2×2 网格 + 竖横两条分隔条），分隔条是 8px 命中区
// 的细线。TerminalView 实例自带的 ResizeObserver 会在窗格几何变化时自动 fit
// 并把新行列发给后端，无需额外处理。

/** 分隔条总宽（px）：两侧窗格各让出一半，避免终端画布贴死。 */
const SPLIT_GAP = 8;
/** 窗格最小占比，拖拽与恢复时夹取。 */
const SPLIT_MIN_RATIO = 0.15;
const SPLIT_MAX_RATIO = 0.85;

const appContentRef = ref<HTMLDivElement | null>(null);

/** 当前布局的窗格槽数：双格 2，四格 4。 */
const splitSlotCount = computed(() => (splitGrid.value ? 4 : 2));

/** 当前实际可见的 tab key 集合（v-show 判定源）。 */
const visiblePaneKeys = computed<Set<string>>(() => {
  if (!splitEnabled.value) {
    return new Set(activeTabKey.value ? [activeTabKey.value] : []);
  }
  const s = new Set<string>();
  for (const k of splitPaneKeys.value) if (k) s.add(k);
  return s;
});

/** tab 所在窗格下标；不在任何窗格（隐藏）返回 -1。 */
function paneIndex(key: string): SplitPaneIdx | -1 {
  if (!splitEnabled.value) return activeTabKey.value === key ? 0 : -1;
  return (splitPaneKeys.value.indexOf(key) as SplitPaneIdx | -1) ?? -1;
}

/** 按窗格下标计算绝对定位几何（双格 0-1，四格 0-3 = 左上/右上/左下/右下）。 */
function paneStyleByIdx(idx: SplitPaneIdx): Record<string, string> {
  const gap = SPLIT_GAP / 2;
  if (splitGrid.value) {
    const x = `${(splitRatio.value * 100).toFixed(4)}%`;
    const y = `${(splitRatio2.value * 100).toFixed(4)}%`;
    const leftCol = idx % 2 === 0;
    const topRow = idx < 2;
    return {
      ...(leftCol
        ? { left: "0", width: `calc(${x} - ${gap}px)` }
        : { left: `calc(${x} + ${gap}px)`, right: "0" }),
      ...(topRow
        ? { top: "0", height: `calc(${y} - ${gap}px)` }
        : { top: `calc(${y} + ${gap}px)`, bottom: "0" }),
    };
  }
  const pct = `${(splitRatio.value * 100).toFixed(4)}%`;
  if (splitDir.value === "h") {
    return idx === 0
      ? { left: "0", top: "0", bottom: "0", width: `calc(${pct} - ${gap}px)` }
      : { left: `calc(${pct} + ${gap}px)`, top: "0", bottom: "0", right: "0" };
  }
  return idx === 0
    ? { left: "0", right: "0", top: "0", height: `calc(${pct} - ${gap}px)` }
    : { left: "0", right: "0", top: `calc(${pct} + ${gap}px)`, bottom: "0" };
}

/** 单个 tab 的窗格容器样式：隐藏 / 单窗格占满 / 分屏几何。 */
function paneStyle(key: string): Record<string, string> {
  if (!visiblePaneKeys.value.has(key)) return { display: "none" };
  if (!splitEnabled.value) {
    return { left: "0", right: "0", top: "0", bottom: "0" };
  }
  const idx = paneIndex(key);
  return idx >= 0 ? paneStyleByIdx(idx as SplitPaneIdx) : { display: "none" };
}

/** 空窗格（槽位为 null）下标；没有空窗格返回 -1。 */
const emptyPaneIdx = computed<SplitPaneIdx | -1>(() => {
  if (!splitEnabled.value) return -1;
  const idx = splitPaneKeys.value.findIndex((k) => k === null);
  return idx >= 0 && idx < splitSlotCount.value ? (idx as SplitPaneIdx) : -1;
});

/**
 * 点击窗格区域时切换焦点窗格。只有点在终端本体（.terminal-host）上才算
 * "进入这个窗格工作"——点在搜索条 / 命令建议等浮层上不切换，避免激活窗格
 * 时 TerminalView 的 rAF focus 把焦点从浮层输入框里抢走。
 */
function onPaneMousedown(key: string, e: MouseEvent) {
  const el = e.target as HTMLElement | null;
  if (!el || !el.closest(".terminal-host")) return;
  const idx = paneIndex(key);
  if (idx >= 0) focusPane(idx as SplitPaneIdx);
}

// ---- 分隔条拖拽（pointer capture：指针出界也持续跟踪，且不惊动 xterm）----
const splitDragging = ref(false);
/** 正在拖动的分隔条轴向：x = 竖条（调左右占比），y = 横条（调上下占比）。 */
const splitDragAxis = ref<"x" | "y">("x");

function onDividerPointerdown(e: PointerEvent, axis: "x" | "y") {
  if (!splitEnabled.value) return;
  e.preventDefault();
  splitDragging.value = true;
  splitDragAxis.value = axis;
  // pointer capture 让指针拖出条外仍持续跟踪；个别环境（无活动指针）会抛错，忽略
  try {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  } catch {
    // ignore
  }
}

function onDividerPointermove(e: PointerEvent, axis: "x" | "y") {
  if (!splitDragging.value || !appContentRef.value) return;
  const rect = appContentRef.value.getBoundingClientRect();
  const clamp = (raw: number) =>
    Math.min(SPLIT_MAX_RATIO, Math.max(SPLIT_MIN_RATIO, raw));
  if (axis === "x") {
    splitRatio.value = clamp((e.clientX - rect.left) / rect.width);
  } else if (splitGrid.value) {
    // 四分屏的横条调独立的比例（splitRatio2），双格横条仍调 splitRatio
    splitRatio2.value = clamp((e.clientY - rect.top) / rect.height);
  } else {
    splitRatio.value = clamp((e.clientY - rect.top) / rect.height);
  }
}

function onDividerPointerup(e: PointerEvent) {
  if (!splitDragging.value) return;
  splitDragging.value = false;
  try {
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  } catch {
    // ignore
  }
}

// 拖拽期间锁定光标与文本选区，松开后还原
watch(splitDragging, (v) => {
  document.body.style.cursor = v
    ? splitDragAxis.value === "x"
      ? "col-resize"
      : "row-resize"
    : "";
  document.body.style.userSelect = v ? "none" : "";
});

const splitRatioPct = computed(() => `${(splitRatio.value * 100).toFixed(4)}%`);
const splitRatio2Pct = computed(() =>
  `${(splitRatio2.value * 100).toFixed(4)}%`,
);

/** TabBar 右键菜单分屏入口（事件参数为 tab key 在前）。 */
function onTabSplit(key: string, dir: SplitDir) {
  splitTab(dir, key);
}

function onTabSplitGrid(key: string) {
  splitGridTab(key);
}

const {
  aiOpen,
  sftpOpen,
  hostInfoOpen,
  forwardOpen,
  templateOpen,
  localFilesOpen,
  settingsOpen,
  activityBarVisible,
  toggleAi,
  toggleSftp,
  toggleHostInfo,
  toggleForward,
  toggleTemplate,
  toggleLocalFiles,
  toggleSettings,
  toggleActivityBar,
  onSendToAi,
  onSftpSendToAi,
} = usePanels(activeSftpTab, activeAiTab, activeTerminalTab, aiAssistantRef, activeLocalTab, hasLocalTab);

// ── 供应商管理入口（设置弹窗「模型供应商」分区）──
//
// 顶栏独立按钮已移除：引导卡片「添加供应商」与全局快捷键统一打开设置并
// 定位到该分区；关闭设置后清掉深链，下次打开恢复"记住上次分区"的默认行为。
const settingsInitialTab = ref<string | undefined>(undefined);

function openProvidersInSettings() {
  settingsInitialTab.value = "ai-providers";
  settingsOpen.value = true;
}

function toggleProvidersInSettings() {
  if (settingsOpen.value && settingsInitialTab.value === "ai-providers") {
    settingsOpen.value = false;
    settingsInitialTab.value = undefined;
    return;
  }
  openProvidersInSettings();
}

watch(settingsOpen, (open) => {
  if (!open) settingsInitialTab.value = undefined;
});

// 独立窗口：由 openSftpInNewWindow / openAiInNewWindow 创建，
// URL 带 newwin=1&kind=sftp|ai。走完整 App 实例（providers/api init/theme），
// 但只渲染对应 solo 布局。
const launchParams = new URLSearchParams(window.location.search);
const soloKind = launchParams.get("newwin") === "1" ? launchParams.get("kind") : null;
const soloSftp = soloKind === "sftp";
const soloAi = soloKind === "ai";

// 活动栏宽度挂 documentElement：内容区让位计算与右侧抽屉（SFTP / 本地文件 /
// 模板等，均 Teleport 到 body）共用同一变量。此前挂在 .app-root 上，Teleport
// 出去的抽屉读不到（回退 0px），会盖住活动栏；固定停靠的抽屉还会与终端之间
// 露出一条活动栏宽度的空隙。solo 独立窗口无活动栏，恒为 0。
watchEffect(() => {
  const w =
    !soloSftp && !soloAi && tabs.value.length > 0 && activityBarVisible.value
      ? "44px"
      : "0px";
  document.documentElement.style.setProperty("--ashell-activity-w", w);
});
onBeforeUnmount(() => {
  document.documentElement.style.setProperty("--ashell-activity-w", "0px");
});

const {
  isMac,
  onHeaderDblClick,
} = useWindowControls(terminalRefs, activeTabKey);

// SFTP 面板右键目录 -> 当前终端 cd 到该目录（sendCommand 自动补 \r 执行）
function onSftpOpenTerminalHere(path: string) {
  sendCommandToActive(`cd '${path.replace(/'/g, `'\\''`)}'`)
}

// 本地文件抽屉右键"在终端中打开"：按绑定本地终端的 shell 生成对应语法的 cd
// 命令（PowerShell/cmd/POSIX 的引号规则不同），发送给抽屉绑定的那个本地 tab
// ——即使活动 tab 已切到别处（固定停靠的抽屉），也始终作用于它跟随的终端。
function onLocalOpenInTerminal(path: string) {
  const tab = activeLocalTab.value;
  if (!tab) return;
  const shell = tab.shell ?? "auto";
  const cmd =
    shell === "cmd"
      ? `cd /d "${path}"`
      : shell === "bash" ||
          shell === "git-bash" ||
          shell === "zsh" ||
          shell === "sh" ||
          shell === "fish"
        ? `cd '${path.replace(/'/g, `'\\''`)}'`
        : `cd '${path.replace(/'/g, "''")}'`;
  terminalRefs.get(tab.key)?.sendCommand(cmd);
}

// 开发者模式快捷键：F12 / Ctrl+Shift+I 切换 WebView 开发者工具
// （开关在 设置-通用-开发者选项；Web 形态内部自动跳过）
installDevtoolsShortcut();

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
  toggleAiProviders: toggleProvidersInSettings,
  toggleHostInfo,
  toggleForward,
  toggleTemplate,
  toggleActivityBar,
  splitRight: () => splitTab("h"),
  splitDown: () => splitTab("v"),
  splitGrid: () => splitGridTab(),
  splitClose: disableSplit,
  focusNextPane,
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
            <div v-else class="app-root">
              <!-- 毛玻璃浓度色罩：亚克力原生效果在 Windows 11 走 DWM 系统背景路径，
                   不接受自定义色罩（window-vibrancy 忽略 color），浓度改由这层 DOM
                   色罩控制（透明度 = 磨砂浓度滑杆）。置于壁纸层之下：设壁纸时壁纸
                   作为底层背景盖住色罩，与历史行为一致。 -->
              <div
                v-if="terminalStore.windowBlur"
                class="acrylic-tint-layer"
                :style="{ opacity: String(terminalStore.acrylicTint) }"
              />
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
                    :split-enabled="splitEnabled"
                    :split-grid="splitGrid"
                    :get-session-content="getSessionContent"
                    @update:active-key="activateTab"
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
                    @split="onTabSplit"
                    @split-grid="onTabSplitGrid"
                    @split-close="disableSplit"
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
                ref="appContentRef"
                class="app-content"
                :style="{
                  top: 'var(--ashell-header-h)',
                  left: 'var(--ashell-hosts-width, 0px)',
                  right:
                    'calc(var(--ashell-activity-w, 0px) + var(--ashell-local-files-w, 0px) + var(--ashell-ai-w, 0px))',
                  bottom: 0,
                }"
                @mousedown="closeHostsIfOpen"
              >
                <div
                  v-for="tab in tabs"
                  :key="tab.key"
                  class="terminal-pane"
                  :style="paneStyle(tab.key)"
                  @mousedown.capture="onPaneMousedown(tab.key, $event)"
                >
                  <TerminalView
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
                    @cwd-change="onCwdChange"
                    @send-to-ai="onSendToAi"
                    @close-tab="closeTab"
                  />
                </div>
                <!-- 分屏时的空窗格占位：点击聚焦该窗格，随后点标签页即在此打开 -->
                <div
                  v-if="emptyPaneIdx !== -1"
                  class="terminal-pane pane-placeholder"
                  :style="paneStyleByIdx(emptyPaneIdx)"
                  @mousedown="focusPane(emptyPaneIdx)"
                >
                  <NIcon :size="40" depth="3"><TerminalOutline /></NIcon>
                  <p>{{ t("app.splitPane.empty") }}</p>
                </div>
                <!-- 双格分隔条：8px 命中区，视觉上是一条 1px 线 -->
                <div
                  v-if="splitEnabled && !splitGrid"
                  class="split-divider"
                  :class="{
                    'dir-h': splitDir === 'h',
                    'dir-v': splitDir === 'v',
                    dragging: splitDragging,
                  }"
                  :style="
                    splitDir === 'h' ? { left: splitRatioPct } : { top: splitRatioPct }
                  "
                  @pointerdown="onDividerPointerdown($event, splitDir === 'h' ? 'x' : 'y')"
                  @pointermove="onDividerPointermove($event, splitDir === 'h' ? 'x' : 'y')"
                  @pointerup="onDividerPointerup"
                  @pointercancel="onDividerPointerup"
                />
                <!-- 四分屏（2×2）的分隔条：竖横各一条，各自调一个比例 -->
                <div
                  v-if="splitEnabled && splitGrid"
                  class="split-divider dir-h"
                  :class="{ dragging: splitDragging && splitDragAxis === 'x' }"
                  :style="{ left: splitRatioPct }"
                  @pointerdown="onDividerPointerdown($event, 'x')"
                  @pointermove="onDividerPointermove($event, 'x')"
                  @pointerup="onDividerPointerup"
                  @pointercancel="onDividerPointerup"
                />
                <div
                  v-if="splitEnabled && splitGrid"
                  class="split-divider dir-v"
                  :class="{ dragging: splitDragging && splitDragAxis === 'y' }"
                  :style="{ top: splitRatio2Pct }"
                  @pointerdown="onDividerPointerdown($event, 'y')"
                  @pointermove="onDividerPointermove($event, 'y')"
                  @pointerup="onDividerPointerup"
                  @pointercancel="onDividerPointerup"
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
                :local-files-open="localFilesOpen"
                :ai-enabled="startupStore.aiAssistantEnabled"
                :has-active-session="!!activeSftpTab"
                :has-terminal-session="!!activeTerminalTab"
                :has-ai-session="!!activeAiTab"
                :has-local-terminal="!!activeLocalTab"
                @toggle-sftp="toggleSftp"
                @toggle-host-info="toggleHostInfo"
                @toggle-forward="toggleForward"
                @toggle-ai="toggleAi"
                @toggle-template="toggleTemplate"
                @toggle-local-files="toggleLocalFiles"
              />

              <HostsDrawer v-model:open="hostsOpen" @open-host="openHost" />
              <LocalFilesDrawer
                v-model:open="localFilesOpen"
                :cwd="activeLocalTab?.cwd ?? ''"
                :tab-title="activeLocalTab?.title"
                @open-in-terminal="onLocalOpenInTerminal"
              />
              <SftpDrawer
                v-model:open="sftpOpen"
                :sid="activeSftpTab?.sid ?? null"
                :host-name="activeSftpTab?.title"
                :host-addr="activeSftpTab?.hostInfo?.addr"
                :host-id="activeSftpTab?.hostId ?? null"
                :cwd="activeSftpTab?.cwd ?? ''"
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
                @open-providers="openProvidersInSettings"
              />

              <SettingsModal
                v-model:open="settingsOpen"
                v-model:theme-mode="themeMode"
                :resolved-theme="resolvedTheme"
                :theme-title="themeTitle"
                :initial-tab="settingsInitialTab"
              />
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

/* 毛玻璃浓度色罩（见模板内注释）：盖在亚克力原生效果之上、壁纸与内容之下 */
.acrylic-tint-layer {
  position: absolute;
  inset: 0;
  background: var(--ashell-bg-solid, #0f1115);
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

/* ===== 分屏窗格 ===== */
.terminal-pane {
  position: absolute;
  overflow: hidden;
}

/* 空窗格占位：虚线框 + 居中提示，可点击聚焦 */
.pane-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--ashell-text-muted, rgba(128, 128, 128, 0.6));
  background: var(--ashell-bg-alpha, transparent);
  border: 1px dashed var(--ashell-border, rgba(128, 128, 128, 0.3));
  border-radius: 8px;
  margin: 2px;
  user-select: none;
  cursor: pointer;
  font-size: 13px;
}

.pane-placeholder p {
  margin: 0;
  padding: 0 16px;
  text-align: center;
}

/* 分隔条：8px 命中区，中间 1px 视觉线；悬停/拖拽时高亮 */
.split-divider {
  position: absolute;
  z-index: 3;
  background: transparent;
  touch-action: none;
}

.split-divider.dir-h {
  top: 0;
  bottom: 0;
  width: 8px;
  transform: translateX(-50%);
  cursor: col-resize;
}

.split-divider.dir-v {
  left: 0;
  right: 0;
  height: 8px;
  transform: translateY(-50%);
  cursor: row-resize;
}

.split-divider::before {
  content: "";
  position: absolute;
  background: var(--ashell-border, rgba(128, 128, 128, 0.3));
  transition: background 120ms ease;
}

.split-divider.dir-h::before {
  top: 0;
  bottom: 0;
  left: 50%;
  width: 1px;
  transform: translateX(-50%);
}

.split-divider.dir-v::before {
  left: 0;
  right: 0;
  top: 50%;
  height: 1px;
  transform: translateY(-50%);
}

.split-divider:hover::before,
.split-divider.dragging::before {
  background: var(--ashell-accent, #80b5ff);
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
  pointer-events: none;
}

.empty-state > * {
  pointer-events: auto;
}
</style>
