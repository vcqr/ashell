import { nextTick, onBeforeUnmount, ref, type Ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useBroadcastStore } from "@/stores/broadcast";
import { useTerminalStore } from "@/stores/terminal";
import { detectMac, getWindowId, isTauri } from "@/utils/platform";
import type { TerminalViewExposed } from "./useTabs";

/**
 * 窗口控制与窗口级生命周期：最大化/最小化/关闭、平台检测、跨窗口广播初始化、
 * 透明度/壁纸初始化、首帧 show、resize 时重排所有终端。
 *
 * 依赖 useTabs 产出的 terminalRefs（resize 时需要 relayout 所有终端实例），
 * 由 App.vue 创建 useTabs 后传入。
 *
 * 桌面端走 Tauri Window API；Web 端（浏览器）无窗口系统调用：仅初始化
 * 广播（windowId 用每标签页随机 id）、挂原生 window resize 做终端重排；
 * 最小化/最大化/关闭为 no-op（对应 UI 在 Web 下隐藏）。getCurrentWindow()
 * 只允许在 isTauri 分支内调用（浏览器端会抛错）。
 */
export function useWindowControls(
  terminalRefs: Map<string, TerminalViewExposed>,
  activeTabKey: Ref<string>,
) {
  const broadcastStore = useBroadcastStore();
  const terminalStore = useTerminalStore();

  const isMaximized = ref(false);
  const isMac = detectMac();

  // 初始化跨窗口广播：桌面用 Tauri window label，Web 用每标签页 id
  void broadcastStore.init(isTauri ? getCurrentWindow().label : getWindowId());

  // 初始化窗口透明度 CSS 变量 + Acrylic 效果（由 setWindowOpacity 内部驱动）
  terminalStore.setWindowOpacity(terminalStore.windowOpacity);
  terminalStore.setWallpaperOpacity(terminalStore.wallpaperOpacity);
  // 加载持久化的壁纸
  void terminalStore.loadWallpaper();

  let unlistenResize: (() => void) | null = null;

  /** 取当前 Tauri 窗口句柄（仅桌面形态有意义） */
  function appWindow() {
    if (!isTauri) return null;
    return getCurrentWindow();
  }

  onBeforeUnmount(() => {
    broadcastStore.destroy();
  });

  if (isTauri) {
    const appWindow = getCurrentWindow();

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
      try {
        const unlisten = await unlistenResizePromise;
        unlisten();
      } catch {
        // ignore
      }
    });
  } else {
    // Web 端：浏览器派发原生 resize 事件，做同样的 rAF 节流重排
    let relayoutRaf: number | null = null;
    const onResize = () => {
      if (relayoutRaf !== null) return;
      relayoutRaf = requestAnimationFrame(() => {
        relayoutRaf = null;
        terminalRefs.get(activeTabKey.value)?.relayout();
      });
    };
    window.addEventListener("resize", onResize);
    unlistenResize = () => window.removeEventListener("resize", onResize);
    onBeforeUnmount(() => {
      if (relayoutRaf !== null) {
        cancelAnimationFrame(relayoutRaf);
        relayoutRaf = null;
      }
      unlistenResize?.();
      unlistenResize = null;
    });
  }

  function minimizeWindow() {
    if (!isTauri) return;
    appWindow()?.minimize();
  }

  async function toggleMaximize() {
    if (!isTauri) return;
    const win = appWindow();
    if (!win) return;
    await win.toggleMaximize();
    try {
      isMaximized.value = await win.isMaximized();
    } catch {
      // ignore
    }
  }

  function closeWindow() {
    if (!isTauri) return;
    appWindow()?.close();
  }

  function onHeaderDblClick(e: MouseEvent) {
    if (!isMac) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest('[data-tauri-drag-region="false"]')) return;
    // 阻止 Tauri drag-region 脚本的 dblclick 处理器（会调用 unmaximize），
    // 由我们统一用 toggleMaximize 处理双向切换
    e.stopPropagation();
    void toggleMaximize();
  }

  return {
    isMaximized,
    isMac,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
    onHeaderDblClick,
  };
}
