import { ref } from "vue";

const RECENT_KEY = "ashell:recent-host-ids";
const MAX_RECENT = 10;

function loadRecentIds(): number[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    const arr = raw ? (JSON.parse(raw) as unknown) : [];
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (n): n is number => typeof n === "number" && Number.isFinite(n),
    );
  } catch {
    return [];
  }
}

/**
 * 最近连接的主机 id（模块级单例，MRU 序，队首最新）：
 * openHost 时经 recordRecentHost 记录，快速连接面板（Ctrl+Shift+K）
 * 默认列表按此排序。纯前端偏好（localStorage），不动后端 schema。
 */
const recentHostIds = ref<number[]>(loadRecentIds());

function persist() {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recentHostIds.value));
  } catch {
    // ignore
  }
}

/** 记录一次连接：id 提到队首（去重），超长截断 */
export function recordRecentHost(id: number): void {
  const next = recentHostIds.value.filter((x) => x !== id);
  next.unshift(id);
  recentHostIds.value = next.slice(0, MAX_RECENT);
  persist();
}

/** 只读引用：面板按此顺序渲染「最近连接」段 */
export function useRecentHostIds() {
  return recentHostIds;
}
