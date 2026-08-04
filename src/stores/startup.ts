import { defineStore } from "pinia"
import { ref, watch } from "vue"

/** 启动行为偏好。 */
export interface StartupConfig {
  /** 应用启动后是否自动打开一个本地 PTY tab（前提：tab 持久化里没有恢复任何 tab）。 */
  openLocalOnStart: boolean
  /** 默认 shell；空串视为 'auto'，由后端按平台选择（Win→PowerShell，Unix→$SHELL）。 */
  defaultShell: string
  /** 是否记住上次打开的 tab；关闭时启动不恢复，且关闭后会清空已落盘的 tab 记录。 */
  restoreTabs: boolean
  /** 恢复记住的 tab 时是否自动重连；关闭时仅恢复骨架，等用户手动重连。 */
  autoConnectRememberedTabs: boolean
  /** 是否启用 AI 助手；关闭时启动不加载 AI 助手，相关入口一并隐藏。 */
  aiAssistantEnabled: boolean
}

const STORAGE_KEY = "ashell:startup-config"

const DEFAULT_CONFIG: StartupConfig = {
  openLocalOnStart: false,
  defaultShell: "auto",
  restoreTabs: true,
  autoConnectRememberedTabs: false,
  aiAssistantEnabled: true,
}

function loadConfig(): StartupConfig {
  if (typeof localStorage === "undefined") return { ...DEFAULT_CONFIG }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_CONFIG }
    const parsed = JSON.parse(raw) as Partial<StartupConfig>
    return {
      openLocalOnStart:
        typeof parsed.openLocalOnStart === "boolean"
          ? parsed.openLocalOnStart
          : DEFAULT_CONFIG.openLocalOnStart,
      defaultShell:
        typeof parsed.defaultShell === "string" && parsed.defaultShell
          ? parsed.defaultShell
          : DEFAULT_CONFIG.defaultShell,
      restoreTabs:
        typeof parsed.restoreTabs === "boolean"
          ? parsed.restoreTabs
          : DEFAULT_CONFIG.restoreTabs,
      autoConnectRememberedTabs:
        typeof parsed.autoConnectRememberedTabs === "boolean"
          ? parsed.autoConnectRememberedTabs
          : DEFAULT_CONFIG.autoConnectRememberedTabs,
      aiAssistantEnabled:
        typeof parsed.aiAssistantEnabled === "boolean"
          ? parsed.aiAssistantEnabled
          : DEFAULT_CONFIG.aiAssistantEnabled,
    }
  } catch {
    return { ...DEFAULT_CONFIG }
  }
}

/** 平台候选 shell；与后端 service::local_pty::resolve_command 的分支保持一致。 */
export interface ShellOption {
  label: string
  value: string
  description?: string
}

export function shellOptionsForPlatform(): ShellOption[] {
  const isWin =
    typeof navigator !== "undefined" && /Win/i.test(navigator.platform || "")
  if (isWin) {
    return [
      { label: "Auto (PowerShell → cmd)", value: "auto" },
      { label: "PowerShell", value: "powershell" },
      { label: "PowerShell 7 (pwsh)", value: "pwsh" },
      { label: "Command Prompt (cmd)", value: "cmd" },
      { label: "Git Bash", value: "git-bash" },
    ]
  }
  return [
    { label: "Auto ($SHELL)", value: "auto" },
    { label: "Bash", value: "bash" },
    { label: "Zsh", value: "zsh" },
    { label: "Fish", value: "fish" },
    { label: "POSIX sh", value: "sh" },
  ]
}

export const useStartupStore = defineStore("startup", () => {
  const initial = loadConfig()
  const openLocalOnStart = ref<boolean>(initial.openLocalOnStart)
  const defaultShell = ref<string>(initial.defaultShell)
  const restoreTabs = ref<boolean>(initial.restoreTabs)
  const autoConnectRememberedTabs = ref<boolean>(
    initial.autoConnectRememberedTabs,
  )
  const aiAssistantEnabled = ref<boolean>(initial.aiAssistantEnabled)

  function persist() {
    if (typeof localStorage === "undefined") return
    try {
      const snapshot: StartupConfig = {
        openLocalOnStart: openLocalOnStart.value,
        defaultShell: defaultShell.value,
        restoreTabs: restoreTabs.value,
        autoConnectRememberedTabs: autoConnectRememberedTabs.value,
        aiAssistantEnabled: aiAssistantEnabled.value,
      }
      localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot))
    } catch {
      // ignore quota / disabled
    }
  }

  watch(
    [
      openLocalOnStart,
      defaultShell,
      restoreTabs,
      autoConnectRememberedTabs,
      aiAssistantEnabled,
    ],
    persist,
  )

  function setOpenLocalOnStart(v: boolean) {
    openLocalOnStart.value = v
  }
  function setDefaultShell(v: string) {
    defaultShell.value = v.trim() || "auto"
  }
  function setRestoreTabs(v: boolean) {
    restoreTabs.value = v
  }
  function setAutoConnectRememberedTabs(v: boolean) {
    autoConnectRememberedTabs.value = v
  }
  function setAiAssistantEnabled(v: boolean) {
    aiAssistantEnabled.value = v
  }

  return {
    openLocalOnStart,
    defaultShell,
    restoreTabs,
    autoConnectRememberedTabs,
    aiAssistantEnabled,
    setOpenLocalOnStart,
    setDefaultShell,
    setRestoreTabs,
    setAutoConnectRememberedTabs,
    setAiAssistantEnabled,
  }
})
