import { nextTick, ref, watch } from "vue";

/**
 * 贴底跟随滚动：用户停在底部附近时内容增长自动跟随；上滚阅读后不再拉扯。
 *
 * 所有滚动一律用 behavior:"auto" 瞬时滚动——部分 WebView 环境
 * behavior:"smooth" 会被整体忽略（实测 scrollTo smooth 完全不动），
 * 且工具步骤连续到达时平滑滚动互相打断，表现为"挂不住底"；
 * naive NScrollbar 的 scrollTo 方法在内容未溢出时还会静默 no-op，
 * 因此这里对滚动容器元素直接做原生 scrollTo。
 */
export interface UseStickToBottomOptions {
  /** 返回滚动容器元素（如 NScrollbar 内部的 .n-scrollbar-container）；每次调用现取，容忍节点重建 */
  getEl: () => HTMLElement | null | undefined;
  /** 内容增长的响应式来源（getter 形式，可闭包取多个值）：变化时若处于跟随态则贴底 */
  sources: () => unknown;
  /** 距底小于该值（px）视为"停在底部"，默认 48 */
  threshold?: number;
}

export function useStickToBottom(options: UseStickToBottomOptions) {
  const threshold = options.threshold ?? 48;

  /** 跟随状态：距底按钮按此显隐 */
  const stickBottom = ref(true);

  /** 瞬时滚动到底（不判断状态） */
  function stickToBottomNow() {
    options.getEl()?.scrollTo({ top: 1e9, behavior: "auto" });
  }

  /** 绑定滚动容器的 @scroll：距底不足阈值视为"停在底部"，恢复跟随 */
  function onScroll(e: Event) {
    const el = e.target as HTMLElement;
    stickBottom.value =
      el.scrollHeight - el.scrollTop - el.clientHeight < threshold;
  }

  /** 用户主动动作（发送、回底按钮、切换会话/面板重开）：恢复跟随并贴底 */
  function scrollToBottom() {
    stickBottom.value = true;
    void nextTick(() => stickToBottomNow());
  }

  // 内容增长 watcher：仅在跟随态贴底（flush post：DOM 更新后再量高）
  watch(
    options.sources,
    () => {
      if (stickBottom.value) stickToBottomNow();
    },
    { flush: "post" },
  );

  return { stickBottom, onScroll, scrollToBottom, scrollEl: options.getEl };
}
