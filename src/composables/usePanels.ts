import { nextTick, ref, watch, type ComputedRef, type Ref } from "vue";
import type { TerminalTab } from "@/types";
import type { AiAssistantExposed } from "./useTabs";
import { useLocalFilesPin } from "./useLocalFilesPin";

const ACTIVITY_BAR_KEY = "ashell:activity-bar-visible";

function loadActivityBarVisible(): boolean {
  if (typeof localStorage === "undefined") return true;
  const raw = localStorage.getItem(ACTIVITY_BAR_KEY);
  return raw === null ? true : raw === "true";
}

/**
 * 侧面板 / 抽屉开关与互斥逻辑（SFTP / 本地文件 / 主机信息 / 端口转发 / AI / 设置 / 活动栏）。
 *
 * 依赖 useTabs 产出的 activeSftpTab / activeAiTab / activeTerminalTab / activeLocalTab /
 * aiAssistantRef：由 App.vue 先创建 useTabs 再把这些响应式引用传入，
 * 保证单向依赖、无环。
 */
export function usePanels(
  activeSftpTab: ComputedRef<TerminalTab | undefined>,
  activeAiTab: ComputedRef<TerminalTab | undefined>,
  activeTerminalTab: ComputedRef<TerminalTab | undefined>,
  aiAssistantRef: Ref<AiAssistantExposed | null>,
  activeLocalTab: ComputedRef<TerminalTab | undefined>,
  hasLocalTab: ComputedRef<boolean>,
) {
  const aiOpen = ref(false);
  const sftpOpen = ref(false);
  const hostInfoOpen = ref(false);
  const forwardOpen = ref(false);
  const templateOpen = ref(false);
  const localFilesOpen = ref(false);
  const settingsOpen = ref(false);
  const aiProvidersOpen = ref(false);
  const activityBarVisible = ref(loadActivityBarVisible());

  // 本地文件抽屉固定（停靠）态：固定时其他面板 / tab 切换不自动收起它，
  // 语义与主机的「固定到左侧」一致（HostsDrawer 固定后同样不参与互斥）
  const localFilesPinned = useLocalFilesPin();

  /** 自动收起本地文件抽屉的唯一入口；固定态跳过（用户点关闭/图钉才收） */
  function dismissLocalFiles() {
    if (localFilesPinned.value) return;
    localFilesOpen.value = false;
  }

  watch(activityBarVisible, (v) => {
    try {
      localStorage.setItem(ACTIVITY_BAR_KEY, String(v));
    } catch {
      // ignore
    }
  });

  function toggleAi() {
    if (!activeAiTab.value) return;
    aiOpen.value = !aiOpen.value;
    if (aiOpen.value) {
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
      templateOpen.value = false;
      dismissLocalFiles();
    }
  }

  function onSendToAi(_tabKey: string, text: string) {
    if (!activeAiTab.value) return;
    if (!aiOpen.value) {
      aiOpen.value = true;
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
      dismissLocalFiles();
    }
    void nextTick(() => aiAssistantRef.value?.sendText(text));
  }

  function onSftpSendToAi(text: string) {
    onSendToAi("", text);
  }

  function toggleSftp() {
    if (!activeSftpTab.value) return;
    sftpOpen.value = !sftpOpen.value;
    if (sftpOpen.value) {
      aiOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
      templateOpen.value = false;
      dismissLocalFiles();
    }
  }

  function toggleHostInfo() {
    if (!activeSftpTab.value) return;
    hostInfoOpen.value = !hostInfoOpen.value;
    if (hostInfoOpen.value) {
      aiOpen.value = false;
      sftpOpen.value = false;
      forwardOpen.value = false;
      templateOpen.value = false;
      dismissLocalFiles();
    }
  }

  function toggleForward() {
    if (!activeSftpTab.value) return;
    forwardOpen.value = !forwardOpen.value;
    if (forwardOpen.value) {
      aiOpen.value = false;
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      templateOpen.value = false;
      dismissLocalFiles();
    }
  }

  function toggleTemplate() {
    if (!activeTerminalTab.value) return;
    templateOpen.value = !templateOpen.value;
    if (templateOpen.value) {
      aiOpen.value = false;
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
      dismissLocalFiles();
    }
  }

  function toggleLocalFiles() {
    if (!activeLocalTab.value) return;
    localFilesOpen.value = !localFilesOpen.value;
    if (localFilesOpen.value) {
      aiOpen.value = false;
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
      templateOpen.value = false;
    }
  }

  function toggleSettings() {
    settingsOpen.value = !settingsOpen.value;
  }

  function toggleAiProviders() {
    aiProvidersOpen.value = !aiProvidersOpen.value;
  }

  function toggleActivityBar() {
    activityBarVisible.value = !activityBarVisible.value;
  }

  watch(activeSftpTab, (tab) => {
    if (!tab) {
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
    }
  });

  watch(activeTerminalTab, (tab) => {
    if (!tab) {
      templateOpen.value = false;
    }
  });

  // 本地文件抽屉跟随本地 tab：切到远程 / 关闭最后一个本地终端时收起
  // （固定停靠态除外，见 dismissLocalFiles）
  watch(activeLocalTab, (tab) => {
    if (!tab) {
      dismissLocalFiles();
    }
  });

  // 销毁策略：本地终端 tab 全部关闭时，本地文件面板无条件关闭（固定态也不例外）——
  // 它是终端的伴随面板，没有任何本地终端就不应残留。仅"切走激活 tab 但本地
  // tab 仍存在"的场景才由上方 dismissLocalFiles 按固定态豁免。
  // 固定偏好本身保留（与主机的图钉一致，是用户偏好而非会话状态）。
  watch(hasLocalTab, (has) => {
    if (!has) {
      localFilesOpen.value = false;
    }
  });

  // AI 面板跟随 activeAiTab：切到无可用 sid 的 tab 时收起，避免悬空。
  watch(activeAiTab, (tab) => {
    if (!tab) aiOpen.value = false;
  });

  return {
    aiOpen,
    sftpOpen,
    hostInfoOpen,
    forwardOpen,
    templateOpen,
    localFilesOpen,
    settingsOpen,
    aiProvidersOpen,
    activityBarVisible,
    toggleAi,
    toggleSftp,
    toggleHostInfo,
    toggleForward,
    toggleTemplate,
    toggleLocalFiles,
    toggleSettings,
    toggleAiProviders,
    toggleActivityBar,
    onSendToAi,
    onSftpSendToAi,
  };
}
