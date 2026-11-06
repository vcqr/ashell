import { defineStore } from "pinia"
import { ref, watch } from "vue"

/** 自定义终端启动项（终端 Profile）：+ 菜单里可直开的命名命令。 */
export interface TerminalProfile {
  /** 稳定 id；菜单 key 与增删改定位用 */
  id: string
  /** 菜单显示名，如「Ubuntu WSL」 */
  name: string
  /** 完整启动命令行（含参数），如 `wsl.exe ~`；带空格路径用引号包裹 */
  command: string
  /** 初始工作目录；空 = 默认家目录 */
  cwd?: string
  /** 图标：~/.ashell/icons/ 图标库文件名（与主机图标同库）；空 = 默认终端图标 */
  icon?: string
}

const STORAGE_KEY = "ashell:terminal-profiles"

function genId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `p-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

function loadProfiles(): TerminalProfile[] {
  if (typeof localStorage === "undefined") return []
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    // 逐字段校验，坏数据丢弃而不是整表拒绝
    return parsed
      .filter(
        (p): p is Partial<TerminalProfile> =>
          !!p && typeof p === "object",
      )
      .map((p) => ({
        id: typeof p.id === "string" && p.id ? p.id : genId(),
        name: typeof p.name === "string" ? p.name : "",
        command: typeof p.command === "string" ? p.command : "",
        cwd: typeof p.cwd === "string" && p.cwd ? p.cwd : undefined,
        icon: typeof p.icon === "string" && p.icon ? p.icon : undefined,
      }))
  } catch {
    return []
  }
}

/** 自定义启动项 store：localStorage 持久化，设置面板与 TabBar 菜单共用。
 *  与启动设置（ashell:startup-config）同族的本机启动偏好。 */
export const useTerminalProfilesStore = defineStore("terminalProfiles", () => {
  const list = ref<TerminalProfile[]>(loadProfiles())

  function persist() {
    if (typeof localStorage === "undefined") return
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(list.value))
    } catch {
      // ignore quota / disabled
    }
  }
  watch(list, persist, { deep: true })

  /** 追加一个空项并返回 id（设置面板据此聚焦新行） */
  function add(): string {
    const id = genId()
    list.value = [...list.value, { id, name: "", command: "", cwd: "", icon: "" }]
    return id
  }

  function update(id: string, patch: Partial<Omit<TerminalProfile, "id">>) {
    list.value = list.value.map((p) => (p.id === id ? { ...p, ...patch } : p))
  }

  function remove(id: string) {
    list.value = list.value.filter((p) => p.id !== id)
  }

  return { list, add, update, remove }
})
