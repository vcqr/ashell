import { defineStore } from "pinia";
import { ref } from "vue";
import type { CommandTemplate, CommandTemplateCreate, CommandTemplateUpdate } from "@/types";
import {
  listTemplates,
  createTemplate,
  updateTemplate,
  deleteTemplate,
} from "@/api/templates";

/**
 * 模板命令 store。
 *
 * 预置命令片段列表，全局共享，持久化在后端 SQLite。
 * TemplateDrawer 组件在 onMounted 时调用 load() 拉取。
 */
export const useTemplateStore = defineStore("templates", () => {
  const templates = ref<CommandTemplate[]>([]);
  const loaded = ref(false);

  async function load() {
    if (loaded.value) return;
    try {
      templates.value = await listTemplates();
      loaded.value = true;
    } catch (err) {
      console.error("[templates] load failed:", err);
    }
  }

  async function add(input: CommandTemplateCreate): Promise<boolean> {
    try {
      const item = await createTemplate(input);
      templates.value = [...templates.value, item];
      return true;
    } catch (err) {
      console.error("[templates] add failed:", err);
      return false;
    }
  }

  async function edit(id: number, input: CommandTemplateUpdate): Promise<boolean> {
    try {
      const updated = await updateTemplate(id, input);
      templates.value = templates.value.map((t) => (t.id === id ? updated : t));
      return true;
    } catch (err) {
      console.error("[templates] edit failed:", err);
      return false;
    }
  }

  async function remove(id: number) {
    try {
      await deleteTemplate(id);
      templates.value = templates.value.filter((t) => t.id !== id);
    } catch (err) {
      console.error("[templates] remove failed:", err);
    }
  }

  return {
    templates,
    loaded,
    load,
    add,
    edit,
    remove,
  };
});
