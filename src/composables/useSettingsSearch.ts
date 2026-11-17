import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import i18n from "@/locales";

/**
 * 设置分区 key → i18n 命名空间（settings.* 下的子对象）。
 * 用于把每个分区下的全部文案（行 label/desc、选项、按钮…）纳入搜索索引，
 * 这样不必挂载分区组件就能"搜'代理'命中通用分区"。ai-providers 与 ai
 * 共用 settings.ai 命名空间（供应商文案在 ai.provider 下）。
 */
const TAB_NAMESPACES: Record<string, string[]> = {
  general: ["general"],
  theme: ["theme", "terminalColors"],
  terminal: ["terminal"],
  shortcuts: ["shortcuts"],
  window: ["window"],
  tray: ["tray"],
  icons: ["icons"],
  startup: ["startup"],
  "ai-providers": ["ai"],
  ai: ["ai"],
  notify: ["notify"],
  remote: ["remote"],
  backup: ["backup"],
  security: ["security"],
  about: ["about"],
};

/** 递归收集 i18n 消息树里的字符串叶子 */
function flattenValues(node: unknown, out: string[]) {
  if (typeof node === "string") {
    if (node.trim()) out.push(node);
  } else if (Array.isArray(node)) {
    for (const item of node) flattenValues(item, out);
  } else if (node && typeof node === "object") {
    for (const value of Object.values(node as Record<string, unknown>)) {
      flattenValues(value, out);
    }
  }
}

/**
 * 设置内搜索：各分区为 v-if 单挂载，无法逐行搜 DOM；改为对 i18n 文案建
 * 分区级索引——nav 标签 + 该分区命名空间下全部文案任一包含 query 即命中。
 * 索引随 locale 切换自动重算（computed 依赖 locale）。
 */
export function useSettingsSearch() {
  const { t, locale } = useI18n();
  const searchQuery = ref("");

  const isSearching = computed(() => searchQuery.value.trim().length > 0);

  /** tab → 全量检索文本列表（小写，nav 标签在前）。语言切换时随 computed 重算 */
  const keywordIndex = computed<Record<string, string[]>>(() => {
    const messages = i18n.global.getLocaleMessage(locale.value) as Record<
      string,
      unknown
    >;
    const settings = (messages?.settings ?? {}) as Record<string, unknown>;
    const index: Record<string, string[]> = {};
    for (const [tab, namespaces] of Object.entries(TAB_NAMESPACES)) {
      const parts: string[] = [t(`settings.tabs.${tab}`)];
      for (const ns of namespaces) {
        flattenValues(settings[ns], parts);
      }
      // 保序去重（ai-providers 与 ai 共享命名空间会重复收录）
      index[tab] = [...new Set(parts)].map((s) => s.toLowerCase());
    }
    return index;
  });

  /**
   * 过滤给定分区（保持传入顺序，供调用方按 nav 顺序展示）。
   * 未在搜索态返回 null（调用方据此走常规分组渲染）。
   */
  function filterMatchedTabs<T extends string>(availableTabs: T[]): T[] | null {
    const q = searchQuery.value.trim().toLowerCase();
    if (!q) return null;
    const index = keywordIndex.value;
    return availableTabs.filter((tab) =>
      (index[tab] ?? []).some((text) => text.includes(q)),
    );
  }

  /** 分区 → 命中的具体文案列表（供 nav 命中项下方展示"匹配到哪里"） */
  const matchedSnippetsByTab = computed<Record<string, string[]>>(() => {
    const q = searchQuery.value.trim().toLowerCase();
    if (!q) return {};
    const out: Record<string, string[]> = {};
    for (const [tab, texts] of Object.entries(keywordIndex.value)) {
      const hits = texts.filter((text) => text.includes(q));
      if (hits.length > 0) out[tab] = hits;
    }
    return out;
  });

  function clear() {
    searchQuery.value = "";
  }

  return {
    searchQuery,
    isSearching,
    filterMatchedTabs,
    matchedSnippetsByTab,
    clear,
  };
}
