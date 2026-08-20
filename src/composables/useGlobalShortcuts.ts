import { onBeforeUnmount, onMounted } from "vue";
import type { Ref } from "vue";
import type { TerminalTab } from "@/types";
import { useTerminalStore } from "@/stores/terminal";
import { useKeybindingStore, matchesBinding } from "@/stores/keybindings";

export interface GlobalShortcutDeps {
  isMac: boolean;
  tabs: Ref<TerminalTab[]>;
  activeTabKey: Ref<string>;
  openLocal: () => void;
  closeTab: (key: string) => void;
  toggleHosts: () => void;
  toggleSettings: () => void;
  toggleAi: () => void;
  toggleSftp: () => void;
  toggleAiProviders: () => void;
  toggleHostInfo: () => void;
  toggleForward: () => void;
  toggleTemplate: () => void;
  toggleActivityBar: () => void;
}

/**
 * 全局快捷键（在 window 捕获阶段拦截，优先于 xterm 的 textarea）。
 * 快捷键可通过设置面板的「快捷键」Tab 自定义，此 composable 从 keybinding store
 * 读取当前绑定进行匹配。
 *
 * 分两类：
 * 1. 面板快捷键 -- 不受 tabShortcutsEnabled 门控，始终生效
 * 2. 标签页快捷键 -- 受 tabShortcutsEnabled 门控（有用户想把所有 Ctrl 组合键透传给 shell）
 *
 * 录制中时（keybindingStore.recording === true）全部跳过，让录制器独占按键。
 */
export function useGlobalShortcuts(deps: GlobalShortcutDeps) {
  const terminalStore = useTerminalStore();
  const keybindingStore = useKeybindingStore();

  function handleKeydown(e: KeyboardEvent) {
    if (keybindingStore.recording) return;

    // ---- 面板快捷键（不受 tabShortcutsEnabled 门控） ----
    if (matchesBinding(keybindingStore.getBinding("panel.hosts"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleHosts();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.settings"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleSettings();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.ai"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleAi();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.sftp"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleSftp();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.aiProviders"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleAiProviders();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.hostInfo"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleHostInfo();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.forward"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleForward();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.template"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleTemplate();
      return;
    }
    if (matchesBinding(keybindingStore.getBinding("panel.activityBar"), e)) {
      e.preventDefault();
      e.stopPropagation();
      deps.toggleActivityBar();
      return;
    }

    // ---- 标签页快捷键（受 tabShortcutsEnabled 门控） ----
    if (!terminalStore.tabShortcutsEnabled) return;

    // 新建 tab：忽略按住重复（否则连开一串 tab）
    if (matchesBinding(keybindingStore.getBinding("tab.new"), e)) {
      if (e.repeat) return;
      e.preventDefault();
      e.stopPropagation();
      deps.openLocal();
      return;
    }

    // 关闭当前 tab
    if (matchesBinding(keybindingStore.getBinding("tab.close"), e)) {
      if (e.repeat) return;
      e.preventDefault();
      e.stopPropagation();
      if (deps.activeTabKey.value) deps.closeTab(deps.activeTabKey.value);
      return;
    }

    // 上一个 / 下一个 tab（e.repeat 放行，按住连续切换与浏览器一致）
    if (matchesBinding(keybindingStore.getBinding("tab.next"), e)) {
      const tabs = deps.tabs.value;
      if (tabs.length < 2) return;
      e.preventDefault();
      e.stopPropagation();
      const idx = tabs.findIndex((t) => t.key === deps.activeTabKey.value);
      if (idx < 0) return;
      deps.activeTabKey.value = tabs[(idx + 1) % tabs.length]!.key;
      return;
    }

    if (matchesBinding(keybindingStore.getBinding("tab.prev"), e)) {
      const tabs = deps.tabs.value;
      if (tabs.length < 2) return;
      e.preventDefault();
      e.stopPropagation();
      const idx = tabs.findIndex((t) => t.key === deps.activeTabKey.value);
      if (idx < 0) return;
      deps.activeTabKey.value =
        tabs[(idx - 1 + tabs.length) % tabs.length]!.key;
      return;
    }

    // 数字 1-9 跳 tab
    if (matchesBinding(keybindingStore.getBinding("tab.jump"), e)) {
      e.preventDefault();
      e.stopPropagation();
      const tab = deps.tabs.value[Number(e.key) - 1];
      if (tab) deps.activeTabKey.value = tab.key;
    }
  }

  onMounted(() =>
    window.addEventListener("keydown", handleKeydown, true),
  );
  onBeforeUnmount(() =>
    window.removeEventListener("keydown", handleKeydown, true),
  );
}
