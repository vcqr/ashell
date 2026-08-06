import { defineStore } from "pinia"
import { reactive, ref, watch } from "vue"
import type { ITheme } from "@xterm/xterm"
import { invoke, convertFileSrc } from "@tauri-apps/api/core"
import {
  defaultTerminalTheme,
  mergeTerminalTheme,
  resolveCurrentTerminalTheme,
  type TerminalThemeName,
} from "@/theme/terminal"

export type CursorStyle = "block" | "underline" | "bar"
export type RightClickAction = "paste" | "smart" | "contextMenu" | "none"
export type LeftClickAction = "copyOnSelect" | "copyAndMiddlePaste" | "middlePasteOnly" | "none"
export type DisconnectAction = "keep" | "closeTab" | "closeWindow"

export interface TerminalConfig {
  fontSize: number
  fontFamily: string
  cursorStyle: CursorStyle
  cursorBlink: boolean
  rightClickAction: RightClickAction
  leftClickAction: LeftClickAction
  disconnectAction: DisconnectAction
  darkTheme: ITheme
  lightTheme: ITheme
  webglEnabled: boolean
  webLinksEnabled: boolean
  unicode11Enabled: boolean
  searchHotkeyEnabled: boolean
  ligaturesEnabled: boolean
  progressEnabled: boolean
  commandSuggestEnabled: boolean
  scrollback: number
}

const STORAGE_KEY = "ashell:terminal-config"

/**
 * 字体预设列表。每个 value 是完整的 CSS font-family 回退链。
 *
 * CJK 回退说明：所有预设末尾都追加了跨平台中文字体（PingFang SC / Microsoft
 * YaHei / Noto Sans CJK SC），确保 xterm WebGL 渲染器在构建字形图集时能为
 * 中文找到合适的字形，而不是走浏览器兜底导致模糊、宽度错位、中英文高低不齐。
 * 顺序：英文等宽主字体 → 其他英文等宽 fallback → CJK 字体 → monospace 兜底。
 */
export const FONT_FAMILY_PRESETS: { label: string; value: string }[] = [
  { label: "Fira Code", value: "'Fira Code', 'JetBrains Mono', Menlo, Consolas, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
  { label: "JetBrains Mono", value: "'JetBrains Mono', 'Fira Code', Menlo, Consolas, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
  { label: "Cascadia Code", value: "'Cascadia Code', 'Cascadia Mono', Consolas, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
  { label: "Source Code Pro", value: "'Source Code Pro', Menlo, Consolas, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
  { label: "Menlo", value: "Menlo, Monaco, Consolas, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
  { label: "Consolas", value: "Consolas, 'Courier New', 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
  { label: "System Monospace", value: "ui-monospace, SFMono-Regular, Menlo, Consolas, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', monospace" },
]

export const FONT_SIZE_MIN = 10
export const FONT_SIZE_MAX = 24
export const SCROLLBACK_MIN = 0
export const SCROLLBACK_MAX = 100000

const DEFAULT_CONFIG: TerminalConfig = {
  fontSize: 13,
  fontFamily: FONT_FAMILY_PRESETS[0]!.value,
  cursorStyle: "block",
  cursorBlink: true,
  rightClickAction: "paste",
  leftClickAction: "copyOnSelect",
  disconnectAction: "keep",
  darkTheme: defaultTerminalTheme("dark"),
  lightTheme: defaultTerminalTheme("light"),
  webglEnabled: true,
  webLinksEnabled: true,
  unicode11Enabled: true,
  searchHotkeyEnabled: true,
  ligaturesEnabled: false,
  progressEnabled: true,
  commandSuggestEnabled: true,
  scrollback: 5000,
}

function clampFontSize(n: unknown): number {
  const v = typeof n === "number" && Number.isFinite(n) ? n : DEFAULT_CONFIG.fontSize
  return Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, Math.round(v)))
}

function isCursorStyle(v: unknown): v is CursorStyle {
  return v === "block" || v === "underline" || v === "bar"
}

function isRightClickAction(v: unknown): v is RightClickAction {
  return v === "paste" || v === "smart" || v === "contextMenu" || v === "none"
}

function isLeftClickAction(v: unknown): v is LeftClickAction {
  return (
    v === "copyOnSelect" ||
    v === "copyAndMiddlePaste" ||
    v === "middlePasteOnly" ||
    v === "none"
  )
}

function isDisconnectAction(v: unknown): v is DisconnectAction {
  return v === "keep" || v === "closeTab" || v === "closeWindow"
}

function loadConfig(): TerminalConfig {
  if (typeof localStorage === "undefined") return { ...DEFAULT_CONFIG }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_CONFIG }
    const parsed = JSON.parse(raw) as Partial<TerminalConfig>
    return {
      fontSize: clampFontSize(parsed.fontSize),
      fontFamily:
        typeof parsed.fontFamily === "string" && parsed.fontFamily.trim().length > 0
          ? parsed.fontFamily
          : DEFAULT_CONFIG.fontFamily,
      cursorStyle: isCursorStyle(parsed.cursorStyle)
        ? parsed.cursorStyle
        : DEFAULT_CONFIG.cursorStyle,
      cursorBlink:
        typeof parsed.cursorBlink === "boolean"
          ? parsed.cursorBlink
          : DEFAULT_CONFIG.cursorBlink,
      rightClickAction: isRightClickAction(parsed.rightClickAction)
        ? parsed.rightClickAction
        : DEFAULT_CONFIG.rightClickAction,
      leftClickAction: isLeftClickAction(parsed.leftClickAction)
        ? parsed.leftClickAction
        : DEFAULT_CONFIG.leftClickAction,
      disconnectAction: isDisconnectAction(parsed.disconnectAction)
        ? parsed.disconnectAction
        : DEFAULT_CONFIG.disconnectAction,
      darkTheme: mergeTerminalTheme("dark", parsed.darkTheme as Partial<ITheme> | undefined),
      lightTheme: mergeTerminalTheme("light", parsed.lightTheme as Partial<ITheme> | undefined),
      webglEnabled:
        typeof parsed.webglEnabled === "boolean"
          ? parsed.webglEnabled
          : DEFAULT_CONFIG.webglEnabled,
      webLinksEnabled:
        typeof parsed.webLinksEnabled === "boolean"
          ? parsed.webLinksEnabled
          : DEFAULT_CONFIG.webLinksEnabled,
      unicode11Enabled:
        typeof parsed.unicode11Enabled === "boolean"
          ? parsed.unicode11Enabled
          : DEFAULT_CONFIG.unicode11Enabled,
      searchHotkeyEnabled:
        typeof parsed.searchHotkeyEnabled === "boolean"
          ? parsed.searchHotkeyEnabled
          : DEFAULT_CONFIG.searchHotkeyEnabled,
      ligaturesEnabled:
        typeof parsed.ligaturesEnabled === "boolean"
          ? parsed.ligaturesEnabled
          : DEFAULT_CONFIG.ligaturesEnabled,
      progressEnabled:
        typeof parsed.progressEnabled === "boolean"
          ? parsed.progressEnabled
          : DEFAULT_CONFIG.progressEnabled,
      commandSuggestEnabled:
        typeof parsed.commandSuggestEnabled === "boolean"
          ? parsed.commandSuggestEnabled
          : DEFAULT_CONFIG.commandSuggestEnabled,
      scrollback:
        typeof parsed.scrollback === "number" &&
        Number.isFinite(parsed.scrollback) &&
        parsed.scrollback >= 0
          ? Math.min(SCROLLBACK_MAX, Math.round(parsed.scrollback))
          : DEFAULT_CONFIG.scrollback,
    }
  } catch {
    return { ...DEFAULT_CONFIG }
  }
}

export const useTerminalStore = defineStore("terminal", () => {
  const initial = loadConfig()
  const fontSize = ref<number>(initial.fontSize)
  const fontFamily = ref<string>(initial.fontFamily)
  const cursorStyle = ref<CursorStyle>(initial.cursorStyle)
  const cursorBlink = ref<boolean>(initial.cursorBlink)
  const rightClickAction = ref<RightClickAction>(initial.rightClickAction)
  const leftClickAction = ref<LeftClickAction>(initial.leftClickAction)
  const disconnectAction = ref<DisconnectAction>(initial.disconnectAction)

  const darkTheme = reactive<ITheme>({ ...initial.darkTheme })
  const lightTheme = reactive<ITheme>({ ...initial.lightTheme })

  const webglEnabled = ref<boolean>(initial.webglEnabled)
  const webLinksEnabled = ref<boolean>(initial.webLinksEnabled)
  const unicode11Enabled = ref<boolean>(initial.unicode11Enabled)
  const searchHotkeyEnabled = ref<boolean>(initial.searchHotkeyEnabled)
  const ligaturesEnabled = ref<boolean>(initial.ligaturesEnabled)
  const progressEnabled = ref<boolean>(initial.progressEnabled)
  const commandSuggestEnabled = ref<boolean>(initial.commandSuggestEnabled)
  const scrollback = ref<number>(initial.scrollback)

  /** 窗口透明度 (0.3 – 1.0)。1 = 完全不透明。控制 WebView 内容层 alpha。 */
  const WINDOW_OPACITY_KEY = "ashell:window-opacity"
  const windowOpacity = ref<number>(loadWindowOpacity())

  /** 毛玻璃模糊开关。true = Acrylic 亚克力，false = 纯透明无模糊。 */
  const WINDOW_BLUR_KEY = "ashell:window-blur"
  const windowBlur = ref<boolean>(loadWindowBlur())

  /** 壁纸透明度 (0 – 1)。1 = 完全不透明，0 = 完全透明。 */
  const WALLPAPER_OPACITY_KEY = "ashell:wallpaper-opacity"
  const wallpaperOpacity = ref<number>(loadWallpaperOpacity())

  function loadWindowOpacity(): number {
    if (typeof localStorage === "undefined") return 1
    const raw = localStorage.getItem(WINDOW_OPACITY_KEY)
    const n = raw ? Number(raw) : NaN
    if (!Number.isFinite(n)) return 1
    return Math.min(1, Math.max(0.3, n))
  }

  function loadWindowBlur(): boolean {
    if (typeof localStorage === "undefined") return true
    return localStorage.getItem(WINDOW_BLUR_KEY) !== "false"
  }

  function loadWallpaperOpacity(): number {
    if (typeof localStorage === "undefined") return 1
    const raw = localStorage.getItem(WALLPAPER_OPACITY_KEY)
    const n = raw ? Number(raw) : NaN
    if (!Number.isFinite(n)) return 1
    return Math.min(1, Math.max(0, n))
  }

  function setWindowOpacity(v: number) {
    const clamped = Math.min(1, Math.max(0.3, v))
    windowOpacity.value = clamped
    try {
      localStorage.setItem(WINDOW_OPACITY_KEY, String(clamped))
    } catch {
      // ignore
    }
    document.documentElement.style.setProperty(
      "--ashell-bg-alpha",
      String(clamped),
    )
    applyWindowEffect()
  }

  function setWindowBlur(v: boolean) {
    windowBlur.value = v
    try {
      localStorage.setItem(WINDOW_BLUR_KEY, String(v))
    } catch {
      // ignore
    }
    applyWindowEffect()
  }

  function setWallpaperOpacity(v: number) {
    const clamped = Math.min(1, Math.max(0, v))
    wallpaperOpacity.value = clamped
    try {
      localStorage.setItem(WALLPAPER_OPACITY_KEY, String(clamped))
    } catch {
      // ignore
    }
    document.documentElement.style.setProperty(
      "--ashell-wallpaper-opacity",
      String(clamped),
    )
  }

  /**
   * 根据当前 blur + opacity 配置应用原生窗口效果。
   *
   * - blur === false：清除效果（纯透明，无模糊）
   * - blur === true：Acrylic 亚克力，color alpha 跟随 opacity
   */
  async function applyWindowEffect() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window")
      const win = getCurrentWindow()
      if (!windowBlur.value) {
        await win.clearEffects()
        return
      }
      const isDark = resolveCurrentTerminalTheme() === "dark"
      const r = isDark ? 15 : 244
      const g = isDark ? 17 : 246
      const b = isDark ? 21 : 251
      await win.setEffects({
        effects: ["acrylic" as never],
        color: [r, g, b, Math.round(windowOpacity.value * 255)],
      })
    } catch {
      // 非 Windows 或不支持 — 静默忽略，CSS 透明仍生效
    }
  }

  /** hex(#rrggbb) → rgba(r, g, b, alpha)。用于终端背景注入透明度。 */
  function hexToRgba(hex: string, alpha: number): string {
    const m = /^#([0-9a-f]{6})$/i.exec(hex)
    if (!m) return hex
    const r = parseInt(m[1]!.slice(0, 2), 16)
    const g = parseInt(m[1]!.slice(2, 4), 16)
    const b = parseInt(m[1]!.slice(4, 6), 16)
    return `rgba(${r}, ${g}, ${b}, ${alpha})`
  }

  /** 通过 Tauri 命令枚举到的系统字体族名（仅家族名，不含 fallback 链）。空数组表示未加载或加载失败。 */
  const systemFonts = ref<string[]>([])
  const systemFontsLoading = ref(false)
  let systemFontsLoaded = false

  async function loadSystemFonts(force = false): Promise<string[]> {
    if (!force && systemFontsLoaded) return systemFonts.value
    if (systemFontsLoading.value) return systemFonts.value
    systemFontsLoading.value = true
    try {
      const list = await invoke<string[]>("list_system_fonts")
      systemFonts.value = Array.isArray(list) ? list : []
      systemFontsLoaded = true
    } catch {
      systemFonts.value = []
    } finally {
      systemFontsLoading.value = false
    }
    return systemFonts.value
  }

  function persist() {
    if (typeof localStorage === "undefined") return
    try {
      const data: TerminalConfig = {
        fontSize: fontSize.value,
        fontFamily: fontFamily.value,
        cursorStyle: cursorStyle.value,
        cursorBlink: cursorBlink.value,
        rightClickAction: rightClickAction.value,
        leftClickAction: leftClickAction.value,
        disconnectAction: disconnectAction.value,
        darkTheme: { ...darkTheme },
        lightTheme: { ...lightTheme },
        webglEnabled: webglEnabled.value,
        webLinksEnabled: webLinksEnabled.value,
        unicode11Enabled: unicode11Enabled.value,
        searchHotkeyEnabled: searchHotkeyEnabled.value,
        ligaturesEnabled: ligaturesEnabled.value,
        progressEnabled: progressEnabled.value,
        commandSuggestEnabled: commandSuggestEnabled.value,
        scrollback: scrollback.value,
      }
      localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
    } catch {
      // ignore
    }
  }

  watch(
    [
      fontSize,
      fontFamily,
      cursorStyle,
      cursorBlink,
      rightClickAction,
      leftClickAction,
      disconnectAction,
      webglEnabled,
      webLinksEnabled,
      unicode11Enabled,
      searchHotkeyEnabled,
      ligaturesEnabled,
      progressEnabled,
      commandSuggestEnabled,
      scrollback,
    ],
    persist,
  )
  watch(darkTheme, persist, { deep: true })
  watch(lightTheme, persist, { deep: true })

  function resetDefaults() {
    fontSize.value = DEFAULT_CONFIG.fontSize
    fontFamily.value = DEFAULT_CONFIG.fontFamily
    cursorStyle.value = DEFAULT_CONFIG.cursorStyle
    cursorBlink.value = DEFAULT_CONFIG.cursorBlink
    rightClickAction.value = DEFAULT_CONFIG.rightClickAction
    leftClickAction.value = DEFAULT_CONFIG.leftClickAction
    disconnectAction.value = DEFAULT_CONFIG.disconnectAction
    webglEnabled.value = DEFAULT_CONFIG.webglEnabled
    webLinksEnabled.value = DEFAULT_CONFIG.webLinksEnabled
    unicode11Enabled.value = DEFAULT_CONFIG.unicode11Enabled
    searchHotkeyEnabled.value = DEFAULT_CONFIG.searchHotkeyEnabled
    ligaturesEnabled.value = DEFAULT_CONFIG.ligaturesEnabled
    progressEnabled.value = DEFAULT_CONFIG.progressEnabled
    commandSuggestEnabled.value = DEFAULT_CONFIG.commandSuggestEnabled
    scrollback.value = DEFAULT_CONFIG.scrollback
    resetTerminalThemes()
  }

  function setFontSize(n: number) {
    fontSize.value = clampFontSize(n)
  }

  function resetTerminalTheme(name: TerminalThemeName) {
    Object.assign(name === "dark" ? darkTheme : lightTheme, defaultTerminalTheme(name))
  }

  function resetTerminalThemes() {
    resetTerminalTheme("dark")
    resetTerminalTheme("light")
  }

  /** 给 TerminalView 用：返回当前已解析主题的快照。背景色注入窗口透明度。 */
  function getActiveTerminalTheme(): ITheme {
    const base = resolveCurrentTerminalTheme() === "dark" ? darkTheme : lightTheme
    const theme = { ...base }
    if (wallpaperUrl.value) {
      // 有壁纸时终端背景完全透明，让壁纸透过显示
      theme.background = "#00000000"
    } else if (windowOpacity.value < 1 && theme.background) {
      theme.background = hexToRgba(theme.background, windowOpacity.value)
    }
    return theme
  }

  /** 窗口背景壁纸 URL（asset protocol），null = 无壁纸 */
  const wallpaperUrl = ref<string | null>(null)

  async function loadWallpaper() {
    try {
      const path = await invoke<string | null>("get_wallpaper")
      wallpaperUrl.value = path ? convertFileSrc(path) : null
    } catch {
      // ignore
    }
  }

  async function setWallpaper(path: string) {
    const filePath = await invoke<string>("set_wallpaper", { sourcePath: path })
    wallpaperUrl.value = convertFileSrc(filePath)
  }

  async function clearWallpaper() {
    await invoke("clear_wallpaper")
    wallpaperUrl.value = null
  }

  return {
    fontSize,
    fontFamily,
    cursorStyle,
    cursorBlink,
    rightClickAction,
    leftClickAction,
    disconnectAction,
    darkTheme,
    lightTheme,
    webglEnabled,
    webLinksEnabled,
    unicode11Enabled,
    searchHotkeyEnabled,
    ligaturesEnabled,
    progressEnabled,
    commandSuggestEnabled,
    scrollback,
    windowOpacity,
    setWindowOpacity,
    windowBlur,
    setWindowBlur,
    wallpaperOpacity,
    setWallpaperOpacity,
    wallpaperUrl,
    loadWallpaper,
    setWallpaper,
    clearWallpaper,
    systemFonts,
    systemFontsLoading,
    loadSystemFonts,
    resetDefaults,
    resetTerminalTheme,
    resetTerminalThemes,
    getActiveTerminalTheme,
    setFontSize,
  }
})
