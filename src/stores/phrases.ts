import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { QuickPhrase } from "@/types";
import { listPhrases, createPhrase, deletePhrase, clearAllPhrases } from "@/api/phrases";

/**
 * AI 常用语 store。
 *
 * 收藏的用户消息列表，全局共享（不按 ssid 分割），持久化在后端 SQLite。
 * AiAssistant 组件在 onMounted 时调用 load() 拉取。
 */
export const usePhraseStore = defineStore("phrases", () => {
  const phrases = ref<QuickPhrase[]>([]);
  const loaded = ref(false);

  /** 已收藏的 content 集合，用于气泡上的星标状态判断 */
  const contentsSet = computed(
    () => new Set(phrases.value.map((p) => p.content)),
  );

  function isFavorited(content: string): boolean {
    return contentsSet.value.has(content);
  }

  async function load() {
    if (loaded.value) return;
    try {
      phrases.value = await listPhrases();
      loaded.value = true;
    } catch (err) {
      console.error("[phrases] load failed:", err);
    }
  }

  async function add(content: string): Promise<boolean> {
    const trimmed = content.trim();
    if (!trimmed || isFavorited(trimmed)) return false;
    try {
      const phrase = await createPhrase({ content: trimmed });
      phrases.value = [...phrases.value, phrase];
      return true;
    } catch (err) {
      console.error("[phrases] add failed:", err);
      return false;
    }
  }

  async function remove(id: number) {
    try {
      await deletePhrase(id);
      phrases.value = phrases.value.filter((p) => p.id !== id);
    } catch (err) {
      console.error("[phrases] remove failed:", err);
    }
  }

  /** 按 content 取消收藏（气泡上取消时用） */
  async function removeByContent(content: string) {
    const phrase = phrases.value.find((p) => p.content === content);
    if (phrase) {
      await remove(phrase.id);
    }
  }

  async function clearAll() {
    try {
      await clearAllPhrases();
      phrases.value = [];
    } catch (err) {
      console.error("[phrases] clearAll failed:", err);
    }
  }

  return {
    phrases,
    loaded,
    isFavorited,
    load,
    add,
    remove,
    removeByContent,
    clearAll,
  };
});
