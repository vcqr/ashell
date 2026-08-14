import { nextTick, onBeforeUnmount, ref, type Ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useBroadcastStore } from "@/stores/broadcast";
import { useTerminalStore } from "@/stores/terminal";
import type { TerminalViewExposed } from "./useTabs";

function detectMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent || "";
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } })
      .userAgentData?.platform ||
    navigator.platform ||
    "";
  return /Mac|iPhone|iPad|iPod/i.test(`${ua} ${platform}`);
}

/**
 * 窗口控制与窗口级生命周期：最大化/最小化/关闭、平台检测、跨窗口广播初始化、
 * 透明度/壁纸初始化、首帧 show、resize 时重排所有终端。
 *
 * 依赖 useTabs 产出的 terminalRefs（resize 时需要 relayout 所有终端实例），
 * 由 App.vue 创建 useTabs 后传入。
 */
export function useWindowControls(
  terminalRefs: Map<string, TerminalViewExposed>,
  activeTabKey: Ref<string>,
) {
  const broadcastStore = useBroadcastStore();
  const terminalStore = useTerminalStore();

  const appWindow = getCurrentWindow();
  const isMaximized = ref(false);
  const isMac = detectMac();

  // 初始化跨窗口广播：用当前窗口 label 作为 windowId
  void broadcastStore.init(appWindow.label);

  // 初始化窗口透明度 CSS 变量 + Acrylic 效果（由 setWindowOpacity 内部驱动）
  terminalStore.setWindowOpacity(terminalStore.windowOpacity);
  terminalStore.setWallpaperOpacity(terminalStore.wallpaperOpacity);
  // 加载持久化的壁纸
  void terminalStore.loadWallpaper();

  // tauri.conf 中窗口 visible:false。透明度 CSS 变量已在上面同步设置，
  // 等首帧 DOM patch 后 show，避免透明窗启动先白屏；失败由 Rust fallback 兜底。
  void nextTick(() => {
    appWindow.show().catch(() => {
      // ignore: Rust fallback 会强制 show
    });
  });

  async function syncMaximized() {
    try {
      isMaximized.value = await appWindow.isMaximized();
    } catch {
      // ignore
    }
  }

  syncMaximized();
  let relayoutRaf: number | null = null;
  const unlistenResizePromise = appWindow.onResized(() => {
    syncMaximized();
    // Tauri/wry 在最大化/最小化/还原时不一定派发浏览器 window.resize，
    // 这里走 Tauri 自己的事件源兜底重排终端。
    // rAF 节流：最大化/还原动画期间 onResized 高频触发，合并为每帧一次；
    // 且只重排激活 tab——隐藏 tab 是 display:none（尺寸 0），切回时会由
    // TerminalView 的 watch(active) 自行 fit，这里 fit 它们纯属浪费。
    if (relayoutRaf !== null) return;
    relayoutRaf = requestAnimationFrame(() => {
      relayoutRaf = null;
      terminalRefs.get(activeTabKey.value)?.relayout();
    });
  });
  onBeforeUnmount(async () => {
    if (relayoutRaf !== null) {
      cancelAnimationFrame(relayoutRaf);
      relayoutRaf = null;
    }
    broadcastStore.destroy();
    try {
      const unlisten = await unlistenResizePromise;
      unlisten();
    } catch {
      // ignore
    }
  });

  function minimizeWindow() {
    appWindow.minimize();
  }

  async function toggleMaximize() {
    await appWindow.toggleMaximize();
    syncMaximized();
  }

  function closeWindow() {
    appWindow.close();
  }

  let lastHeaderMouseDown = 0;
  let headerClickWasMaximized = false;

  function onHeaderMouseDown(e: MouseEvent) {
    if (!isMac || e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest('[data-tauri-drag-region="false"]')) return;

    const now = performance.now();
    if (now - lastHeaderMouseDown < 350) {
      // 双击：阻止冒泡，防止 Tauri drag-region 脚本再次调用 startDragging()，
      // 否则 startDragging 会在窗口刚最大化后触发 macOS 自动 un-zoom 还原。
      e.stopPropagation();
      e.preventDefault();
      lastHeaderMouseDown = 0;
      if (!headerClickWasMaximized) {
        void toggleMaximize();
      }
    } else {
      headerClickWasMaximized = isMaximized.value;
      lastHeaderMouseDown = now;
    }
  }

  return {
    isMaximized,
    isMac,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
    onHeaderMouseDown,
  };
}
