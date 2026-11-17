import { onBeforeUnmount, reactive } from "vue";

/**
 * 语义色 token 的 JS 侧读取：NProgress color、NBadge color 等只接受字符串
 * 传色，拿不到 CSS 变量；这里把 main.css 的 --ashell-success/warning/danger/info
 * 读成响应式字符串。主题切换靠 MutationObserver 监听 data-ashell-theme
 * （与 TerminalView 的 applyTheme 同款手法），初值在 setup 时同步读一次。
 * 返回 reactive 对象而非一组 ref：模板里 `semantic.danger` 这类属性访问
 * 不会自动解包 ref，reactive 保证模板与 JS 侧都拿到纯字符串。
 */
export function useSemanticColors() {
  const colors = reactive({
    success: "#4ade80",
    warning: "#f59e0b",
    danger: "#ef4444",
    info: "#80b5ff",
  });

  function refresh() {
    if (typeof window === "undefined") return;
    const style = getComputedStyle(document.documentElement);
    const read = (name: string, fallback: string) =>
      style.getPropertyValue(name).trim() || fallback;
    colors.success = read("--ashell-success", colors.success);
    colors.warning = read("--ashell-warning", colors.warning);
    colors.danger = read("--ashell-danger", colors.danger);
    colors.info = read("--ashell-info", colors.info);
  }

  refresh();

  const observer = new MutationObserver(refresh);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-ashell-theme"],
  });
  onBeforeUnmount(() => observer.disconnect());

  return colors;
}
