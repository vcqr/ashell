import { onBeforeUnmount, ref } from "vue";
import type { SftpFile } from "@/types";

export type DropZone = "local" | "remote";

interface UseFileDragOptions {
  /** pointerdown 时返回本次拖拽携带的文件；空数组 = 该行不可拖 */
  collectFiles: (row: SftpFile) => SftpFile[];
  /** 落在目标 zone 上时回调 */
  onDrop: (files: SftpFile[], zone: DropZone) => void;
}

/**
 * 双栏文件拖拽传输（本地 <-> 远程）。
 *
 * 不用 HTML5 dnd：wry 上应用内 dragstart 后 dragover 经常不派发
 * （TabBar 拖拽重排已验证该坑），drop 依赖 dragover 的 preventDefault，
 * 整条链不可靠。这里用 pointer events 自实现：
 * - pointerdown 记录候选文件与起点，位移超过 5px 才真正进入拖拽
 *   （避免普通点击/勾选被误判）
 * - window 级 pointermove/pointerup 跟踪，拖出元素仍能收到事件
 * - pointerup 时用 elementFromPoint + closest('[data-drop-zone]') 判定落区
 */
export function useFileDrag(opts: UseFileDragOptions) {
  const dragging = ref(false);
  const ghostX = ref(0);
  const ghostY = ref(0);
  const dragCount = ref(0);

  let candidate: SftpFile[] = [];
  let startX = 0;
  let startY = 0;
  let active = false;
  let listening = false;

  function onMove(e: PointerEvent) {
    if (!active) {
      if (Math.hypot(e.clientX - startX, e.clientY - startY) < 5) return;
      active = true;
      dragging.value = true;
      // 拖拽中全局 grabbing 光标 + 禁止文本选中，跟随指针不丢失。
      // 选中压制走 body class + 全局 !important 规则（见 main.css）：
      // user-select 不继承，直接设 body.style 会被面板的 text 声明屏蔽
      document.body.style.cursor = "grabbing";
      document.body.classList.add("ashell-dragging");
    }
    ghostX.value = e.clientX;
    ghostY.value = e.clientY;
  }

  function cleanup() {
    if (!listening) return;
    listening = false;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onUp);
  }

  function onUp(e: PointerEvent) {
    cleanup();
    const wasActive = active;
    active = false;
    dragging.value = false;
    if (wasActive) {
      document.body.style.cursor = "";
      document.body.classList.remove("ashell-dragging");
      // pointerup 后浏览器可能仍派发一次 click（down/up 视为同一次点击），
      // 会误触发行的选中/勾选。拖拽已发生时在捕获阶段拦截掉这一次 click；
      // 若 down/up 不在同一元素则 click 不会派发，用 setTimeout 兜底移除
      // 监听，避免吞掉用户的下一次正常点击。
      const suppress = (ce: MouseEvent) => {
        ce.preventDefault();
        ce.stopPropagation();
      };
      window.addEventListener("click", suppress, { capture: true, once: true });
      window.setTimeout(() => {
        window.removeEventListener("click", suppress, { capture: true });
      }, 0);
    }
    if (!wasActive || candidate.length === 0) {
      candidate = [];
      return;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const zoneEl = el?.closest?.("[data-drop-zone]") as HTMLElement | null;
    const zone = zoneEl?.dataset.dropZone as DropZone | undefined;
    if (zone) {
      opts.onDrop(candidate, zone);
    }
    candidate = [];
  }

  /** 绑定到表格行的 onPointerdown（通过 rowProps） */
  function onRowPointerdown(row: SftpFile, e: PointerEvent) {
    if (e.button !== 0) return;
    // 勾选框/复选控件上的按下不启动拖拽，避免与勾选操作打架
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, .n-checkbox, .n-checkbox-box")) return;
    const files = opts.collectFiles(row);
    if (files.length === 0) return;
    candidate = files;
    dragCount.value = files.length;
    startX = e.clientX;
    startY = e.clientY;
    active = false;
    if (!listening) {
      listening = true;
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onUp);
    }
  }

  onBeforeUnmount(cleanup);

  return { dragging, ghostX, ghostY, dragCount, onRowPointerdown };
}
