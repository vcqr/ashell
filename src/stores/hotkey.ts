import { defineStore } from "pinia"
import { ref } from "vue"
import { invoke } from "@tauri-apps/api/core"
import { isTauri } from "@/utils/platform"

/** 后端 ~/.ashell/hotkey.json 的镜像（全局热键注册在 Rust 侧，与前端生命周期无关）。 */
export interface HotkeySettings {
  enabled: boolean
  accelerator: string
}

export const useHotkeyStore = defineStore("hotkey", () => {
  const enabled = ref(false)
  const accelerator = ref("")
  const loaded = ref(false)

  async function load() {
    if (!isTauri) return
    try {
      const s = await invoke<HotkeySettings>("hotkey_get_settings")
      enabled.value = s.enabled
      accelerator.value = s.accelerator
      loaded.value = true
    } catch {
      // 非 Tauri 环境（浏览器 dev）下忽略
    }
  }

  /** 保存并注册全局热键；返回 null 表示成功，否则为后端错误信息（由调用方提示）。 */
  async function save(
    nextEnabled: boolean,
    nextAccelerator: string,
  ): Promise<string | null> {
    if (!isTauri) {
      enabled.value = nextEnabled
      accelerator.value = nextAccelerator
      loaded.value = true
      return null
    }
    try {
      const s = await invoke<HotkeySettings>("hotkey_set_settings", {
        enabled: nextEnabled,
        accelerator: nextAccelerator,
      })
      enabled.value = s.enabled
      accelerator.value = s.accelerator
      return null
    } catch (e) {
      return String(e)
    }
  }

  /** 录制期间临时注销当前热键（否则它会把用户按下的同名组合在 OS 层吃掉） */
  async function suspend() {
    if (!isTauri) return
    try {
      await invoke("hotkey_suspend")
    } catch {
      // ignore
    }
  }

  /** 录制取消后按持久化设置恢复注册 */
  async function resume() {
    if (!isTauri) return
    try {
      await invoke("hotkey_resume")
    } catch {
      // ignore
    }
  }

  return { enabled, accelerator, loaded, load, save, suspend, resume }
})
