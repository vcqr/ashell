import type { ITheme } from "@xterm/xterm"

export const terminalDarkTheme: ITheme = {
  background: "#0f1115",
  foreground: "#e6e6e6",
  cursor: "#80b5ff",
  cursorAccent: "#0f1115",
  selectionBackground: "rgba(128, 181, 255, 0.3)",
  black: "#000000",
  red: "#ff6b6b",
  green: "#7ed491",
  yellow: "#f5d97a",
  blue: "#80b5ff",
  magenta: "#d49bff",
  cyan: "#7adfd9",
  white: "#e6e6e6",
  brightBlack: "#5c6370",
  brightRed: "#ff8a8a",
  brightGreen: "#a3e0ad",
  brightYellow: "#fae29c",
  brightBlue: "#a3c9ff",
  brightMagenta: "#dfb8ff",
  brightCyan: "#a3eae5",
  brightWhite: "#ffffff",
}

export const terminalLightTheme: ITheme = {
  background: "#f4f6fb",
  foreground: "#3a3f4b",
  cursor: "#3b82f6",
  cursorAccent: "#f4f6fb",
  selectionBackground: "rgba(59, 130, 246, 0.18)",
  black: "#3a3f4b",
  red: "#e05561",
  green: "#5a9f68",
  yellow: "#c79530",
  blue: "#3b82f6",
  magenta: "#a857c2",
  cyan: "#3a9ca8",
  white: "#d8dde5",
  brightBlack: "#8a92a3",
  brightRed: "#ec6c77",
  brightGreen: "#6fb27d",
  brightYellow: "#dba94a",
  brightBlue: "#5e9bff",
  brightMagenta: "#bc6fd2",
  brightCyan: "#4fb3bf",
  brightWhite: "#ffffff",
}

export type TerminalThemeName = "dark" | "light"

/** 18 个可配置颜色键，按设置面板显示顺序。 */
export type TerminalThemeKey =
  | "background"
  | "foreground"
  | "cursor"
  | "cursorAccent"
  | "selectionBackground"
  | "black"
  | "red"
  | "green"
  | "yellow"
  | "blue"
  | "magenta"
  | "cyan"
  | "white"
  | "brightBlack"
  | "brightRed"
  | "brightGreen"
  | "brightYellow"
  | "brightBlue"
  | "brightMagenta"
  | "brightCyan"
  | "brightWhite"

export interface TerminalThemeField {
  key: TerminalThemeKey
  label: string
  group: "base" | "ansi" | "ansi-bright"
}

export const TERMINAL_THEME_FIELDS: TerminalThemeField[] = [
  { key: "background", label: "settings.terminalColors.background", group: "base" },
  { key: "foreground", label: "settings.terminalColors.foreground", group: "base" },
  { key: "cursor", label: "settings.terminalColors.cursor", group: "base" },
  { key: "cursorAccent", label: "settings.terminalColors.cursorAccent", group: "base" },
  { key: "selectionBackground", label: "settings.terminalColors.selectionBackground", group: "base" },
  { key: "black", label: "settings.terminalColors.black", group: "ansi" },
  { key: "red", label: "settings.terminalColors.red", group: "ansi" },
  { key: "green", label: "settings.terminalColors.green", group: "ansi" },
  { key: "yellow", label: "settings.terminalColors.yellow", group: "ansi" },
  { key: "blue", label: "settings.terminalColors.blue", group: "ansi" },
  { key: "magenta", label: "settings.terminalColors.magenta", group: "ansi" },
  { key: "cyan", label: "settings.terminalColors.cyan", group: "ansi" },
  { key: "white", label: "settings.terminalColors.white", group: "ansi" },
  { key: "brightBlack", label: "settings.terminalColors.brightBlack", group: "ansi-bright" },
  { key: "brightRed", label: "settings.terminalColors.brightRed", group: "ansi-bright" },
  { key: "brightGreen", label: "settings.terminalColors.brightGreen", group: "ansi-bright" },
  { key: "brightYellow", label: "settings.terminalColors.brightYellow", group: "ansi-bright" },
  { key: "brightBlue", label: "settings.terminalColors.brightBlue", group: "ansi-bright" },
  { key: "brightMagenta", label: "settings.terminalColors.brightMagenta", group: "ansi-bright" },
  { key: "brightCyan", label: "settings.terminalColors.brightCyan", group: "ansi-bright" },
  { key: "brightWhite", label: "settings.terminalColors.brightWhite", group: "ansi-bright" },
]

export function defaultTerminalTheme(name: TerminalThemeName): ITheme {
  return name === "dark" ? { ...terminalDarkTheme } : { ...terminalLightTheme }
}

/** 用 partial overrides 合并出完整主题，保证未配置项落回默认。 */
export function mergeTerminalTheme(
  name: TerminalThemeName,
  overrides?: Partial<ITheme> | null,
): ITheme {
  const base = defaultTerminalTheme(name)
  if (!overrides) return base
  return { ...base, ...overrides }
}

/**
 * 当前已解析的终端主题：跟随 App.vue 写入的 `documentElement.dataset.ashellTheme`
 * （由全局主题设置维护：system / dark / light → 解析为 dark/light）。
 * 兜底依次尝试：localStorage('ashell:theme-mode') → prefers-color-scheme。
 */
export function resolveCurrentTerminalTheme(): TerminalThemeName {
  if (typeof document !== "undefined") {
    const v = document.documentElement.dataset.ashellTheme
    if (v === "dark" || v === "light") return v
  }
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem("ashell:theme-mode")
    if (stored === "dark") return "dark"
    if (stored === "light") return "light"
  }
  if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light"
  }
  return "dark"
}
