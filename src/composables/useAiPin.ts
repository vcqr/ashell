import { ref, watch } from "vue";

const PIN_KEY = "ashell:ai-pinned";

function loadPinned(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(PIN_KEY) === "true";
}

/**
 * AI 助手面板「固定到右侧」状态（模块级单例）：
 * 面板的图钉按钮、停靠渲染、App 的内容区让位、
 * usePanels 的「切换 tab / 开其他面板不自动收起」共享同一份状态。
 * 语义与 useLocalFilesPin（本地文件固定到右侧）一致。
 */
const aiPinned = ref(loadPinned());

watch(aiPinned, (v) => {
  try {
    localStorage.setItem(PIN_KEY, String(v));
  } catch {
    // ignore
  }
});

export function useAiPin() {
  return aiPinned;
}
