import { computed, nextTick, ref, watch, type ComputedRef, type Ref } from "vue";
import type { TerminalTab } from "@/types";
import type { AiAssistantExposed } from "./useTabs";
import { useLocalFilesPin } from "./useLocalFilesPin";
import { useAiPin } from "./useAiPin";

const ACTIVITY_BAR_KEY = "ashell:activity-bar-visible";

function loadActivityBarVisible(): boolean {
  if (typeof localStorage === "undefined") return true;
  const raw = localStorage.getItem(ACTIVITY_BAR_KEY);
  return raw === null ? true : raw === "true";
}

/**
 * 互斥切换时的离场动画抑制。
 *
 * 右栏抽屉共用同一锚点（right）与 z-index，A→B 切换时旧面板滑出与新面板
 * 滑入两个 0.25s 动画同帧交叠会闪出重影；被互斥关掉的面板当帧以
 * transition:none 消失，只保留新面板的滑入。模块级状态：六个抽屉组件
 * 各自通过 usePanelSwitchClosed 读取自己那份。
 */
export type SwitchablePanelId =
  | "ai"
  | "sftp"
  | "hostInfo"
  | "forward"
  | "template"
  | "localFiles";

const switchClosedIds = ref(new Set<SwitchablePanelId>());

function markSwitchClosed(ids: SwitchablePanelId[]) {
  if (!ids.length) return;
  const next = new Set(switchClosedIds.value);
  for (const id of ids) next.add(id);
  switchClosedIds.value = next;
  // 标志只需覆盖 open 翻转的那一帧渲染；略长于 0.25s 滑动动画后过期，
  // 兜底防止残留影响后续普通开合（如面板自身关闭按钮）的动画
  window.setTimeout(() => {
    const expired = new Set(switchClosedIds.value);
    for (const id of ids) expired.delete(id);
    switchClosedIds.value = expired;
  }, 300);
}

/** 抽屉组件用：本面板是否刚被互斥切换关闭（应跳过离场滑动动画） */
export function usePanelSwitchClosed(id: SwitchablePanelId) {
  return computed(() => switchClosedIds.value.has(id));
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

  // AI 面板固定（停靠）态：语义与本地文件抽屉的图钉一致
  const aiPinned = useAiPin();

  /** 自动收起 AI 面板的唯一入口；固定态跳过（用户点关闭/图钉才收） */
  function dismissAi() {
    if (aiPinned.value) return;
    aiOpen.value = false;
  }

  watch(activityBarVisible, (v) => {
    try {
      localStorage.setItem(ACTIVITY_BAR_KEY, String(v));
    } catch {
      // ignore
    }
  });

  /**
   * 互斥收起除 except 外的全部右栏面板，返回被实际收起的面板 id
   * （供 markSwitchClosed 跳过其离场动画）。AI / 本地文件走各自的
   * dismiss：固定停靠态豁免收起，未实际收起则不记入。
   */
  function closeOthers(except: SwitchablePanelId): SwitchablePanelId[] {
    const closed: SwitchablePanelId[] = [];
    if (except !== "sftp" && sftpOpen.value) {
      sftpOpen.value = false;
      closed.push("sftp");
    }
    if (except !== "hostInfo" && hostInfoOpen.value) {
      hostInfoOpen.value = false;
      closed.push("hostInfo");
    }
    if (except !== "forward" && forwardOpen.value) {
      forwardOpen.value = false;
      closed.push("forward");
    }
    if (except !== "template" && templateOpen.value) {
      templateOpen.value = false;
      closed.push("template");
    }
    if (except !== "localFiles" && localFilesOpen.value) {
      dismissLocalFiles();
      if (!localFilesOpen.value) closed.push("localFiles");
    }
    if (except !== "ai" && aiOpen.value) {
      dismissAi();
      if (!aiOpen.value) closed.push("ai");
    }
    return closed;
  }

  function toggleAi() {
    if (!activeAiTab.value) return;
    aiOpen.value = !aiOpen.value;
    if (aiOpen.value) markSwitchClosed(closeOthers("ai"));
  }

  function onSendToAi(_tabKey: string, text: string) {
    if (!activeAiTab.value) return;
    if (!aiOpen.value) {
      aiOpen.value = true;
      markSwitchClosed(closeOthers("ai"));
    }
    void nextTick(() => aiAssistantRef.value?.sendText(text));
  }

  function onSftpSendToAi(text: string) {
    onSendToAi("", text);
  }

  function toggleSftp() {
    if (!activeSftpTab.value) return;
    sftpOpen.value = !sftpOpen.value;
    if (sftpOpen.value) markSwitchClosed(closeOthers("sftp"));
  }

  function toggleHostInfo() {
    if (!activeSftpTab.value) return;
    hostInfoOpen.value = !hostInfoOpen.value;
    if (hostInfoOpen.value) markSwitchClosed(closeOthers("hostInfo"));
  }

  function toggleForward() {
    if (!activeSftpTab.value) return;
    forwardOpen.value = !forwardOpen.value;
    if (forwardOpen.value) markSwitchClosed(closeOthers("forward"));
  }

  function toggleTemplate() {
    if (!activeTerminalTab.value) return;
    templateOpen.value = !templateOpen.value;
    if (templateOpen.value) markSwitchClosed(closeOthers("template"));
  }

  function toggleLocalFiles() {
    if (!activeLocalTab.value) return;
    localFilesOpen.value = !localFilesOpen.value;
    if (localFilesOpen.value) markSwitchClosed(closeOthers("localFiles"));
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

  // AI 面板跟随 activeAiTab：切到无可用 sid 的 tab 时收起，避免悬空
  // （固定停靠态除外，与本地文件抽屉的图钉语义一致）
  watch(activeAiTab, (tab) => {
    if (!tab) dismissAi();
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
