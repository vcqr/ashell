import { onBeforeUnmount, onMounted } from "vue";
import type { Ref } from "vue";
import type { TerminalTab } from "@/types";
import { useTerminalStore } from "@/stores/terminal";

export interface GlobalShortcutDeps {
  isMac: boolean;
  tabs: Ref<TerminalTab[]>;
  activeTabKey: Ref<string>;
  openLocal: () => void;
  closeTab: (key: string) => void;
}

/**
 * 标签页级全局快捷键（在 window 捕获阶段拦截，优先于 xterm 的 textarea）：
 *
 * - Ctrl/Cmd + T            新建本地终端 tab（Ctrl+T 在 shell 里是 transpose-chars，
 *                            使用率远低于新开 tab，业界终端普遍拦截）
 * - Ctrl/Cmd + Shift + W    关闭当前 tab（带 Shift 避开终端里 Ctrl+W 删词）
 * - Ctrl/Cmd(+Shift) + Tab  切换上/下一个 tab（macOS 上 Cmd+Tab 被系统占用，
 *                            Mac 也用 Ctrl+Tab，与 iTerm2 一致）
 * - Cmd + 1..9              跳转到第 N 个 tab（仅 macOS，无控制字符冲突）
 * - Ctrl + Alt + 1..9       跳转到第 N 个 tab（Windows/Linux；Ctrl+2..8 会产生
 *                            控制字符，如 Ctrl+3 等同 Ctrl+C，必须避开，参考
 *                            Windows Terminal 的键位选择）
 *
 * 可通过 terminalStore.tabShortcutsEnabled 整体关闭（有用户就是想把所有
 * Ctrl 组合键透传给远端 shell）。
 */
export function useGlobalShortcuts(deps: GlobalShortcutDeps) {
  const terminalStore = useTerminalStore();

  function handleKeydown(e: KeyboardEvent) {
    if (!terminalStore.tabShortcutsEnabled) return;
    if (!(e.ctrlKey || e.metaKey)) return;
    const key = e.key;

    // 新建 tab：忽略按住重复（否则连开一串 tab）
    if (!e.shiftKey && !e.altKey && key.toLowerCase() === "t") {
      if (e.repeat) return;
      e.preventDefault();
      e.stopPropagation();
      deps.openLocal();
      return;
    }

    // 关闭当前 tab
    if (e.shiftKey && !e.altKey && key.toLowerCase() === "w") {
      if (e.repeat) return;
      e.preventDefault();
      e.stopPropagation();
      if (deps.activeTabKey.value) deps.closeTab(deps.activeTabKey.value);
      return;
    }

    // 上一个 / 下一个 tab（e.repeat 放行，按住连续切换与浏览器一致）
    if (key === "Tab" && !e.altKey) {
      const tabs = deps.tabs.value;
      if (tabs.length < 2) return;
      e.preventDefault();
      e.stopPropagation();
      const idx = tabs.findIndex((t) => t.key === deps.activeTabKey.value);
      if (idx < 0) return;
      const next = e.shiftKey
        ? (idx - 1 + tabs.length) % tabs.length
        : (idx + 1) % tabs.length;
      deps.activeTabKey.value = tabs[next]!.key;
      return;
    }

    // 数字 1-9 跳 tab（平台键位见函数头注释）
    if (/^[1-9]$/.test(key)) {
      const matches = deps.isMac
        ? e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey
        : e.ctrlKey && e.altKey && !e.metaKey && !e.shiftKey;
      if (!matches) return;
      e.preventDefault();
      e.stopPropagation();
      const tab = deps.tabs.value[Number(key) - 1];
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
