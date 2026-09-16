import { ref, watch } from "vue";

const PIN_KEY = "ashell:hosts-pinned";

function loadPinned(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(PIN_KEY) === "true";
}

/**
 * 主机树「固定到左侧」状态（模块级单例）：
 * HostTree 的固定按钮、HostsDrawer 的停靠渲染、App 的内容区让位、
 * useTabs 的「打开连接后不收起」共享同一份状态。
 */
const hostsPinned = ref(loadPinned());

watch(hostsPinned, (v) => {
  try {
    localStorage.setItem(PIN_KEY, String(v));
  } catch {
    // ignore
  }
});

export function useHostsPin() {
  return hostsPinned;
}
