/**
 * 无边框窗口标题栏的双击最大化支持（自绘头部 + data-tauri-drag-region 配套）。
 *
 * Tauri 2.12 内建 drag-region 脚本已支持三种取值：bare/"true" 仅元素自身可拖、
 * "deep" 整条子树可拖、"false" 连同祖先一并禁用；button/a/input 等可交互元素
 * 不带属性时自动阻断拖拽。但内建的双击最大化路径在 macOS 依赖 mouseup 与第二
 * 次 mousedown 像级同位、且首次按下已进入原生拖拽会话，触控板轻点也不产生
 * detail=2，双击经常不触发；Windows 上则会与自定义 @dblclick 处理叠加成
 * 二次切换（最大化又立刻还原）。这里把多次点击从内建路径接管过来，统一走
 * DOM dblclick 事件处理。
 */

let dblclickGuardInstalled = false;

/**
 * 安装全局守卫（每文档一次）：deep 拖拽区上的双击/三击（e.detail ≥ 2）不再交给
 * Tauri 内建脚本——capture 阶段 stopPropagation 后 document 上的内建冒泡监听收
 * 不到，macOS 的 mouseup 最大化路径随之失效；preventDefault 同时阻止 WKWebView
 * 从第二次按下启动双击选词/拖选，把标题栏下方的主界面内容选亮。
 * ="false" 区域（标签栏/窗口按钮/动作区）不在拦截范围，双击选词等原生行为保留
 * （如标签重命名输入框内的双击选词）。App 挂载时调用一次。
 */
export function installTauriDblclickGuard(): void {
  if (dblclickGuardInstalled || typeof document === "undefined") return;
  dblclickGuardInstalled = true;
  document.addEventListener(
    "mousedown",
    (e) => {
      const el = e.target as HTMLElement | null;
      if (
        e.detail >= 2 &&
        el?.closest?.('[data-tauri-drag-region="deep"]') &&
        !el.closest('[data-tauri-drag-region="false"]')
      ) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    true,
  );
}

/**
 * 标题栏 @dblclick 处理：点在可交互元素（按钮/链接/输入类，或显式 ="false"）
 * 上不触发最大化，其余区域（品牌区、空白、分隔线等）等同原生标题栏双击缩放；
 * 并清掉按下位置漂移时浏览器可能已产生的选区。全平台统一走这里。
 */
export function onTitlebarDblclick(e: MouseEvent, toggle: () => void): void {
  const el = e.target as HTMLElement | null;
  if (el?.closest("button, a, input, select, textarea, [data-tauri-drag-region='false']")) return;
  e.preventDefault();
  window.getSelection()?.removeAllRanges();
  toggle();
}
