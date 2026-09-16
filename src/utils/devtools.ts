/**
 * WebView 开发者工具开关（桌面端专属，Web 形态为 no-op）。
 *
 * 背景：Windows 上 lib.rs 的 disable_browser_accelerators 关闭了 WebView2 的
 * 浏览器加速键（防 F5 误刷新、让 F 键可录制成全局热键），F12 到不了 DevTools；
 * 且 release 构建需要 tauri 的 "devtools" feature 才编译进开发者工具。因此
 * 快捷键由前端捕获后调用后端 devtools_toggle / devtools_set 命令完成开关。
 *
 * 开关状态持久化在 localStorage：纯前端偏好（只决定前端是否拦截按键），
 * Rust 侧无需感知，与热键等后端权威设置不同。重启后开关保持，但开发者
 * 工具窗口不自动打开，需要时按 F12 / Ctrl+Shift+I 唤出。
 */
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/utils/platform";
import { useKeybindingStore } from "@/stores/keybindings";

const STORAGE_KEY = "ashell:devtools";

export function devtoolsEnabled(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

/** 设置开发者模式：持久化偏好 + 立即打开/关闭开发者工具窗口 */
export async function setDevtoolsEnabled(enabled: boolean): Promise<void> {
  try {
    if (enabled) localStorage.setItem(STORAGE_KEY, "1");
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // localStorage 不可用时仅本次会话生效
  }
  if (isTauri) {
    await invoke("devtools_set", { open: enabled });
  }
}

/**
 * 全局捕获 F12 / Ctrl+Shift+I（macOS Cmd+Opt+I）切换开发者工具。
 * 仅开发者模式开启时拦截；录制快捷键时让位给录制器。
 */
export function installDevtoolsShortcut(): void {
  if (!isTauri) return;
  window.addEventListener(
    "keydown",
    (e: KeyboardEvent) => {
      if (useKeybindingStore().recording) return;
      if (!devtoolsEnabled()) return;
      const isF12 = e.key === "F12";
      // 用 e.code 而非 e.key：非美式键盘布局下带修饰键的字母 key 值会变
      const isDevtoolsI =
        e.code === "KeyI" &&
        e.shiftKey &&
        (e.ctrlKey || (e.metaKey && e.altKey));
      if (!isF12 && !isDevtoolsI) return;
      e.preventDefault();
      e.stopPropagation();
      void invoke<boolean>("devtools_toggle");
    },
    { capture: true },
  );
}
