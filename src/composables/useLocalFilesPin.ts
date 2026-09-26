import { ref, watch } from "vue";

const PIN_KEY = "ashell:local-files-pinned";

function loadPinned(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(PIN_KEY) === "true";
}

/**
 * 本地文件抽屉「固定到右侧」状态（模块级单例）：
 * 抽屉的图钉按钮、停靠渲染、App 的内容区让位、
 * usePanels 的「切换 tab / 开其他面板不自动收起」共享同一份状态。
 * 语义与 useHostsPin（主机树固定到左侧）一致。
 */
const localFilesPinned = ref(loadPinned());

watch(localFilesPinned, (v) => {
  try {
    localStorage.setItem(PIN_KEY, String(v));
  } catch {
    // ignore
  }
});

export function useLocalFilesPin() {
  return localFilesPinned;
}
