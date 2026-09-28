import { defineStore } from "pinia"
import { ref } from "vue"
import { invoke } from "@tauri-apps/api/core"
import { isTauri } from "@/utils/platform"

export type ProxyMode = "system" | "custom" | "direct"

/** 后端 ~/.ashell/proxy.json 的镜像。 */
export interface ProxySettings {
  mode: ProxyMode
  url: string
  noProxy: string
}

export const useProxyStore = defineStore("proxy", () => {
  const mode = ref<ProxyMode>("system")
  const url = ref("")
  const noProxy = ref("")
  const loaded = ref(false)

  async function load() {
    if (!isTauri) return
    try {
      const s = await invoke<ProxySettings>("proxy_get_settings")
      apply(s)
      loaded.value = true
    } catch {
      // 非 Tauri 环境（浏览器 dev）或后端过旧时忽略
    }
  }

  /** 保存设置；失败时抛错，由调用方回退 UI 并提示。 */
  async function save(next?: {
    mode?: ProxyMode
    url?: string
    noProxy?: string
  }) {
    if (next?.mode !== undefined) mode.value = next.mode
    if (next?.url !== undefined) url.value = next.url
    if (next?.noProxy !== undefined) noProxy.value = next.noProxy
    if (!isTauri) return
    apply(await invoke<ProxySettings>("proxy_set_settings", {
      mode: mode.value,
      url: url.value,
      noProxy: noProxy.value,
    }))
  }

  function apply(s: ProxySettings) {
    mode.value = ["system", "custom", "direct"].includes(s.mode)
      ? (s.mode as ProxyMode)
      : "system"
    url.value = s.url ?? ""
    noProxy.value = s.noProxy ?? ""
  }

  return { mode, url, noProxy, loaded, load, save }
})
