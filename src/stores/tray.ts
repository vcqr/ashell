import { defineStore } from "pinia"
import { ref, watch } from "vue"
import { invoke } from "@tauri-apps/api/core"
import { currentLocale } from "@/locales"
import { isTauri } from "@/utils/platform"

export type TrayCloseAction = "quit" | "hide"

/** 后端 ~/.ashell/tray.json 的镜像（关闭策略由 Rust 侧在窗口关闭时读取）。 */
export interface TraySettings {
  enabled: boolean
  closeAction: TrayCloseAction
  /** 退出路径仍有活动连接时是否先弹窗确认 */
  warnOnClose: boolean
}

export const useTrayStore = defineStore("tray", () => {
  const enabled = ref(true)
  const closeAction = ref<TrayCloseAction>("quit")
  const warnOnClose = ref(true)
  const autostart = ref(false)
  const loaded = ref(false)

  async function load() {
    if (!isTauri) return
    try {
      const [settings, auto] = await Promise.all([
        invoke<TraySettings>("tray_get_settings"),
        invoke<boolean>("tray_get_autostart"),
      ])
      enabled.value = settings.enabled
      closeAction.value = settings.closeAction === "hide" ? "hide" : "quit"
      warnOnClose.value = settings.warnOnClose !== false
      autostart.value = auto
      loaded.value = true
    } catch {
      // 非 Tauri 环境（浏览器 dev）下忽略
    }
  }

  async function setEnabled(v: boolean) {
    enabled.value = v
    if (!isTauri) return
    try {
      const s = await invoke<TraySettings>("tray_set_settings", {
        enabled: v,
        closeAction: closeAction.value,
        warnOnClose: warnOnClose.value,
      })
      enabled.value = s.enabled
      closeAction.value = s.closeAction === "hide" ? "hide" : "quit"
      warnOnClose.value = s.warnOnClose !== false
    } catch {
      // ignore
    }
  }

  async function setCloseAction(v: TrayCloseAction) {
    if (!isTauri) {
      closeAction.value = v
      return
    }
    closeAction.value = v
    try {
      await invoke("tray_set_settings", {
        enabled: enabled.value,
        closeAction: v,
        warnOnClose: warnOnClose.value,
      })
    } catch {
      // ignore
    }
  }

  async function setWarnOnClose(v: boolean) {
    warnOnClose.value = v
    if (!isTauri) return
    try {
      await invoke("tray_set_settings", {
        enabled: enabled.value,
        closeAction: closeAction.value,
        warnOnClose: v,
      })
    } catch {
      // ignore
    }
  }

  /** 返回后端确认后的实际状态，失败返回 null（由调用方提示错误）。 */
  async function setAutostart(v: boolean): Promise<boolean | null> {
    if (!isTauri) return null
    try {
      autostart.value = await invoke<boolean>("tray_set_autostart", { enable: v })
      return autostart.value
    } catch {
      return null
    }
  }

  // 托盘菜单文案跟随界面语言（后端持久化 + 重建菜单）
  watch(
    currentLocale,
    (locale) => {
      if (!isTauri) return
      invoke("tray_apply_locale", { locale }).catch(() => {})
    },
    { immediate: true },
  )

  return {
    enabled,
    closeAction,
    warnOnClose,
    autostart,
    loaded,
    load,
    setEnabled,
    setCloseAction,
    setWarnOnClose,
    setAutostart,
  }
})
