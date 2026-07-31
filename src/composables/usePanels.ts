import { nextTick, ref, watch, type ComputedRef, type Ref } from "vue";
import type { TerminalTab } from "@/types";
import type { AiAssistantExposed } from "./useTabs";

const ACTIVITY_BAR_KEY = "ashell:activity-bar-visible";

function loadActivityBarVisible(): boolean {
  if (typeof localStorage === "undefined") return true;
  const raw = localStorage.getItem(ACTIVITY_BAR_KEY);
  return raw === null ? true : raw === "true";
}

/**
 * 侧面板 / 抽屉开关与互斥逻辑（SFTP / 主机信息 / 端口转发 / AI / 设置 / 活动栏）。
 *
 * 依赖 useTabs 产出的 activeSftpTab / activeAiTab / aiAssistantRef：
 * 由 App.vue 先创建 useTabs 再把这三个响应式引用传入，保证单向依赖、无环。
 */
export function usePanels(
  activeSftpTab: ComputedRef<TerminalTab | undefined>,
  activeAiTab: ComputedRef<TerminalTab | undefined>,
  aiAssistantRef: Ref<AiAssistantExposed | null>,
) {
  const aiOpen = ref(false);
  const sftpOpen = ref(false);
  const hostInfoOpen = ref(false);
  const forwardOpen = ref(false);
  const settingsOpen = ref(false);
  const aiProvidersOpen = ref(false);
  const activityBarVisible = ref(loadActivityBarVisible());

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
    }
  }

  function onSendToAi(_tabKey: string, text: string) {
    if (!activeAiTab.value) return;
    if (!aiOpen.value) {
      aiOpen.value = true;
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
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
    }
  }

  function toggleHostInfo() {
    if (!activeSftpTab.value) return;
    hostInfoOpen.value = !hostInfoOpen.value;
    if (hostInfoOpen.value) {
      aiOpen.value = false;
      sftpOpen.value = false;
      forwardOpen.value = false;
    }
  }

  function toggleForward() {
    if (!activeSftpTab.value) return;
    forwardOpen.value = !forwardOpen.value;
    if (forwardOpen.value) {
      aiOpen.value = false;
      sftpOpen.value = false;
      hostInfoOpen.value = false;
    }
  }

  watch(activeSftpTab, (tab) => {
    if (!tab) {
      sftpOpen.value = false;
      hostInfoOpen.value = false;
      forwardOpen.value = false;
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
    settingsOpen,
    aiProvidersOpen,
    activityBarVisible,
    toggleAi,
    toggleSftp,
    toggleHostInfo,
    toggleForward,
    onSendToAi,
    onSftpSendToAi,
  };
}
