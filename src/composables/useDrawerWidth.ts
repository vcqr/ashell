import { onBeforeUnmount, ref } from "vue";

/**
 * 侧边抽屉宽度拖拽：加载/持久化/拖动上限/指针事件接线的统一实现。
 *
 * 六个抽屉（SFTP / AI / 主机信息 / 端口转发 / 主机列表 / 本地文件）共用同一套
 * 「localStorage 记忆宽度 + 指针拖拽 + 视口 90% 上限」逻辑，此前各自复制一份。
 * 面板可锚定在右缘（默认，宽度 = 视口宽 - 光标 X）、左缘（宽度 = 光标 X）
 * 或底缘（高度 = 视口高 - 光标 Y，此时返回的 width 实为高度）。
 */
export type DrawerAnchor = "left" | "right" | "bottom";

export interface DrawerWidthParams {
  /** localStorage 持久化键 */
  storageKey: string;
  /** 拖动/加载的下限（bottom 锚定时为高度下限） */
  minWidth: number;
  /** 无持久化值时的初始宽度（bottom 锚定时为初始高度） */
  defaultWidth: number;
  /** 锚定边，默认 right */
  anchor?: DrawerAnchor;
}

export interface DrawerWidthOptions extends Partial<DrawerWidthParams> {
  /** 面板锚定边，默认 right（resolve 返回值里的 anchor 优先于本静态值） */
  anchor?: DrawerAnchor;
  /** 宽度上限占视口宽比例，默认 0.9（避免抽屉完全盖住主界面） */
  maxWidthRatio?: number;
  /**
   * 动态参数（可选）：宽度键/上下限随组件状态变化时提供（如 SFTP 单栏/双栏
   * 双记忆键）。提供后 storageKey/minWidth/defaultWidth 的静态值被忽略。
   */
  resolve?: () => DrawerWidthParams;
}

export function useDrawerWidth(options: DrawerWidthOptions) {
  const ratio = options.maxWidthRatio ?? 0.9;

  /** 当前锚定边：resolve 动态返回优先，其次静态 anchor，默认 right */
  function anchorOf(): DrawerAnchor {
    if (options.resolve) return options.resolve().anchor ?? options.anchor ?? "right";
    return options.anchor ?? "right";
  }

  function params(): DrawerWidthParams {
    if (options.resolve) return options.resolve();
    // 静态场景三个键必有值
    return {
      storageKey: options.storageKey!,
      minWidth: options.minWidth!,
      defaultWidth: options.defaultWidth!,
    };
  }

  function getMaxWidth(): number {
    // bottom 锚定按视口高计上限（面板 top 最低让位标题栏，0.9 留有余量）
    return Math.round(
      (anchorOf() === "bottom" ? window.innerHeight : window.innerWidth) * ratio,
    );
  }

  function clampWidth(v: number): number {
    const { minWidth } = params();
    return Math.min(getMaxWidth(), Math.max(minWidth, v));
  }

  function loadWidth(): number {
    const p = params();
    const raw =
      typeof localStorage !== "undefined" ? localStorage.getItem(p.storageKey) : null;
    const n = raw ? Number(raw) : NaN;
    if (!Number.isFinite(n)) return p.defaultWidth;
    return clampWidth(n);
  }

  function saveWidth(v: number) {
    try {
      localStorage.setItem(params().storageKey, String(v));
    } catch {
      // ignore
    }
  }

  const width = ref<number>(loadWidth());
  const resizing = ref(false);

  function onResizeStart(e: PointerEvent) {
    e.preventDefault();
    resizing.value = true;
    window.addEventListener("pointermove", onResizeMove);
    window.addEventListener("pointerup", onResizeEnd);
    window.addEventListener("pointercancel", onResizeEnd);
  }

  function onResizeMove(e: PointerEvent) {
    // 左缘锚定：宽度 = 光标 X；底缘锚定：高度 = 视口高 - 光标 Y；
    // 右缘锚定：宽度 = 视口宽 - 光标 X
    const anchor = anchorOf();
    const next =
      anchor === "left"
        ? Math.round(e.clientX)
        : anchor === "bottom"
          ? Math.round(window.innerHeight - e.clientY)
          : Math.round(window.innerWidth - e.clientX);
    width.value = clampWidth(next);
  }

  function onResizeEnd() {
    if (!resizing.value) return;
    resizing.value = false;
    saveWidth(width.value);
    window.removeEventListener("pointermove", onResizeMove);
    window.removeEventListener("pointerup", onResizeEnd);
    window.removeEventListener("pointercancel", onResizeEnd);
  }

  onBeforeUnmount(onResizeEnd);

  /** 按当前参数重新加载持久化宽度（模式切换后调用） */
  function reload() {
    width.value = loadWidth();
  }

  return {
    width,
    resizing,
    loadWidth,
    saveWidth,
    getMaxWidth,
    clampWidth,
    onResizeStart,
    onResizeEnd,
    reload,
  };
}
