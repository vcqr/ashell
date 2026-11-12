import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { AiProvider, AiEnginesState } from "@/types";
import {
  listAiProviders,
  listAiEngines,
  updateAiEngine,
  activateAiEngine,
} from "@/api/aiProviders";
import { sidecarTypeOptions } from "@/composables/useAiConfig";

/**
 * AI 引擎与模型供应商的共享配置状态。
 *
 * 设置面板（AiSection）与 AI 助手面板（AiAssistant）共享同一份状态，
 * 任一处切换引擎 / 供应商 / 模型，另一处立即可见--避免设置里切了引擎
 * 但助手面板 spawn 仍用旧引擎的问题。
 */
export const useAiConfigStore = defineStore("aiConfig", () => {
  const providers = ref<AiProvider[]>([]);
  const enginesState = ref<AiEnginesState | null>(null);
  const busy = ref(false);

  let loaded = false;

  const activeEngine = computed(
    () =>
      enginesState.value?.engines.find(
        (e) => e.engine === enginesState.value?.active_engine,
      ) ?? null,
  );

  const activeSidecarType = computed(
    // 默认引擎 pi：enginesState 未加载或未设置时兜底
    () => enginesState.value?.active_engine || "pi",
  );

  const activeEngineLabel = computed(() => {
    const engine = enginesState.value?.active_engine;
    return sidecarTypeOptions.find((o) => o.value === engine)?.label ?? "";
  });

  const activeProvider = computed(
    () =>
      providers.value.find((p) => p.id === activeEngine.value?.provider_id) ?? null,
  );

  async function load() {
    // 启动竞态防御：应用刚启动时 API 服务可能尚未完成端口绑定，首次请求会
    // 失败（表现为模型选择器长时间空白，直到设置面板打开才触发重载）。
    // 失败自动退避重试，服务就绪后任一次成功即完成加载。
    const maxAttempts = 5;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      try {
        const [state, list] = await Promise.all([listAiEngines(), listAiProviders()]);
        enginesState.value = state;
        providers.value = list;
        loaded = true;
        return;
      } catch (err) {
        console.error(
          `[aiConfig] load attempt ${attempt}/${maxAttempts} failed:`,
          err,
        );
        if (attempt < maxAttempts) {
          await new Promise((r) => setTimeout(r, 1000 * attempt));
        }
      }
    }
  }

  async function ensureLoaded() {
    if (!loaded) await load();
  }

  async function switchEngine(engine: string) {
    if (!enginesState.value || engine === enginesState.value.active_engine) return;
    busy.value = true;
    try {
      enginesState.value = await activateAiEngine(engine);
    } catch (err) {
      console.error("[aiConfig] switchEngine failed:", err);
      throw err;
    } finally {
      busy.value = false;
    }
  }

  async function patchEngine(input: {
    provider_id?: string;
    active_model_id?: string;
    thinking_level?: string;
    context_window?: string;
    max_tokens?: string;
    max_rounds?: string;
    streaming?: string;
  }) {
    const engine = enginesState.value?.active_engine;
    if (!engine || busy.value) return null;
    busy.value = true;
    try {
      const updated = await updateAiEngine(engine, input);
      if (enginesState.value) {
        enginesState.value = {
          ...enginesState.value,
          engines: enginesState.value.engines.map((e) =>
            e.engine === updated.engine ? updated : e,
          ),
        };
      }
      return updated;
    } catch (err) {
      console.error("[aiConfig] patchEngine failed:", err);
      throw err;
    } finally {
      busy.value = false;
    }
  }

  return {
    providers,
    enginesState,
    busy,
    activeEngine,
    activeSidecarType,
    activeEngineLabel,
    activeProvider,
    load,
    ensureLoaded,
    switchEngine,
    patchEngine,
  };
});
