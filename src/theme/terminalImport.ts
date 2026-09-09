import type { ITheme } from "@xterm/xterm"
import {
  TERMINAL_THEME_FIELDS,
  defaultTerminalTheme,
  type TerminalThemeKey,
} from "./terminal"
import type { TerminalThemeVariant } from "./terminalPresets"

/**
 * 终端主题导入 / 导出。
 *
 * 支持两种来源：
 * - iTerm2 的 .itermcolors plist XML（iterm2colorschemes.com 提供的下载格式）
 * - JSON 配色文件（Windows Terminal scheme 或 xterm ITheme 两种键名风格）
 */

const ANSI_KEY_MAP: Record<string, TerminalThemeKey> = {
  black: "black",
  red: "red",
  green: "green",
  yellow: "yellow",
  blue: "blue",
  purple: "magenta",
  magenta: "magenta",
  cyan: "cyan",
  white: "white",
  brightBlack: "brightBlack",
  brightRed: "brightRed",
  brightGreen: "brightGreen",
  brightYellow: "brightYellow",
  brightBlue: "brightBlue",
  brightPurple: "brightMagenta",
  brightMagenta: "brightMagenta",
  brightCyan: "brightCyan",
  brightWhite: "brightWhite",
}

const ITERM_ANSI_MAP: Record<string, TerminalThemeKey> = {
  "Ansi 0 Color": "black",
  "Ansi 1 Color": "red",
  "Ansi 2 Color": "green",
  "Ansi 3 Color": "yellow",
  "Ansi 4 Color": "blue",
  "Ansi 5 Color": "magenta",
  "Ansi 6 Color": "cyan",
  "Ansi 7 Color": "white",
  "Ansi 8 Color": "brightBlack",
  "Ansi 9 Color": "brightRed",
  "Ansi 10 Color": "brightGreen",
  "Ansi 11 Color": "brightYellow",
  "Ansi 12 Color": "brightBlue",
  "Ansi 13 Color": "brightMagenta",
  "Ansi 14 Color": "brightCyan",
  "Ansi 15 Color": "brightWhite",
}

/** 规范化颜色字符串：仅接受 hex(#rgb/#rrggbb/#rrggbbaa) 与 rgb()/rgba()。非法返回 null。 */
export function normalizeThemeColor(input: unknown): string | null {
  if (typeof input !== "string") return null
  const s = input.trim().toLowerCase()
  if (/^#[0-9a-f]{3}$/.test(s)) {
    return `#${s[1]}${s[1]}${s[2]}${s[2]}${s[3]}${s[3]}`
  }
  if (/^#[0-9a-f]{4}$/.test(s)) {
    return `#${s[1]}${s[1]}${s[2]}${s[2]}${s[3]}${s[3]}`
  }
  if (/^#[0-9a-f]{6}$/.test(s) || /^#[0-9a-f]{8}$/.test(s)) return s
  const m = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*([\d.]+)\s*)?\)$/.exec(s)
  if (m) {
    const r = Math.min(255, Number(m[1]))
    const g = Math.min(255, Number(m[2]))
    const b = Math.min(255, Number(m[3]))
    if (m[4] !== undefined) return `rgba(${r}, ${g}, ${b}, ${m[4]})`
    return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`
  }
  return null
}

function hexToRgb01(hex: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{6})/.exec(hex)
  const h = m?.[1]
  if (!h) return null
  return [
    parseInt(h.slice(0, 2), 16) / 255,
    parseInt(h.slice(2, 4), 16) / 255,
    parseInt(h.slice(4, 6), 16) / 255,
  ]
}

/** WCAG 相对亮度。 */
function luminance(hex: string): number {
  const rgb = hexToRgb01(hex)
  if (!rgb) return 0.5
  const [r, g, b] = rgb.map((c) =>
    c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4),
  )
  return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0)
}

/** 根据背景亮度猜测主题明暗归属。 */
export function guessVariant(theme: Partial<ITheme>): TerminalThemeVariant {
  const bg = typeof theme.background === "string" ? theme.background : ""
  const rgb = hexToRgb01(bg)
  if (rgb) {
    const lum = luminance(bg)
    if (lum > 0.55) return "light"
    if (lum < 0.4) return "dark"
  }
  return "dark"
}

/** 解析 iTerm2 .itermcolors（plist XML）为 Partial<ITheme>。解析失败抛错。 */
export function parseItermColors(text: string): Partial<ITheme> {
  const doc = new DOMParser().parseFromString(text, "application/xml")
  if (doc.querySelector("parsererror")) {
    throw new Error("itermcolors XML parse error")
  }
  const root = doc.querySelector("plist > dict")
  if (!root) throw new Error("not a .itermcolors plist")

  const colorDicts = new Map<string, Element>()
  const children = Array.from(root.children)
  for (let i = 0; i < children.length - 1; i++) {
    const key = children[i]
    const next = children[i + 1]
    if (key?.tagName === "key" && next?.tagName === "dict") {
      colorDicts.set(key.textContent?.trim() ?? "", next)
    }
  }
  if (colorDicts.size === 0) throw new Error("no color entries in .itermcolors")

  const readComponent = (dict: Element, name: string): number | null => {
    const kids = Array.from(dict.children)
    for (let i = 0; i < kids.length - 1; i++) {
      if (
        kids[i]?.tagName === "key" &&
        (kids[i]?.textContent?.trim() ?? "") === name
      ) {
        const v = Number(kids[i + 1]?.textContent)
        return Number.isFinite(v) ? v : null
      }
    }
    return null
  }

  const readColor = (dict: Element): string | null => {
    let r = readComponent(dict, "Red Component")
    let g = readComponent(dict, "Green Component")
    let b = readComponent(dict, "Blue Component")
    if (r === null || g === null || b === null) return null
    // 兼容 0-255 取值范围的非标准导出
    if (r > 1 || g > 1 || b > 1) {
      r /= 255
      g /= 255
      b /= 255
    }
    const clamp = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255)
    const to2 = (v: number) => v.toString(16).padStart(2, "0")
    return `#${to2(clamp(r))}${to2(clamp(g))}${to2(clamp(b))}`
  }

  const partial: Partial<ITheme> = {}
  for (const [key, dict] of colorDicts) {
    const color = readColor(dict)
    if (!color) continue
    if (key === "Background Color") partial.background = color
    else if (key === "Foreground Color") partial.foreground = color
    else if (key === "Cursor Color") partial.cursor = color
    else if (key === "Cursor Text Color") partial.cursorAccent = color
    else if (key === "Selection Color") partial.selectionBackground = color
    else if (key === "Bold Color") continue
    else if (key === "Selected Text Color") continue
    else {
      const mapped = ITERM_ANSI_MAP[key]
      if (mapped) partial[mapped] = color
    }
  }
  return partial
}

/** 解析 JSON 配色（Windows Terminal scheme 或 xterm ITheme）。非法键忽略。 */
export function parseSchemeJson(obj: unknown): { name?: string; theme: Partial<ITheme> } {
  if (typeof obj !== "object" || obj === null || Array.isArray(obj)) {
    throw new Error("JSON root must be an object")
  }
  const raw = obj as Record<string, unknown>
  const partial: Partial<ITheme> = {}
  const baseKeys = ["background", "foreground", "cursor", "cursorAccent", "selectionBackground"]
  for (const key of baseKeys) {
    const v = normalizeThemeColor(raw[key])
    if (v) partial[key as TerminalThemeKey] = v
  }
  for (const [src, dst] of Object.entries(ANSI_KEY_MAP)) {
    const v = normalizeThemeColor(raw[src])
    if (v) partial[dst] = v
  }
  if (!partial.background) throw new Error("missing background color")
  const name = typeof raw.name === "string" && raw.name.trim() ? raw.name.trim() : undefined
  return { name, theme: partial }
}

export interface ParsedTerminalTheme {
  name: string
  variant: TerminalThemeVariant
  theme: ITheme
}

/**
 * 解析导入的文本（.itermcolors XML 或 JSON），合并默认主题补全缺失键。
 * name 依次取：显式传入 → 方案内 name → 文件名（去扩展名）→ "Custom"。
 */
export function parseTerminalThemeInput(
  text: string,
  fileName?: string,
  explicitName?: string,
): ParsedTerminalTheme {
  const trimmed = text.trim()
  if (!trimmed) throw new Error("empty input")

  let partial: Partial<ITheme>
  let schemeName: string | undefined
  if (trimmed.startsWith("<")) {
    partial = parseItermColors(trimmed)
  } else {
    let obj: unknown
    try {
      obj = JSON.parse(trimmed)
    } catch {
      throw new Error("invalid JSON or .itermcolors content")
    }
    const parsed = parseSchemeJson(obj)
    partial = parsed.theme
    schemeName = parsed.name
  }

  if (!partial.background) throw new Error("missing background color")
  const variant = guessVariant(partial)
  // 用对应明暗的默认主题补全未提供的颜色键，保证应用后一定是完整主题
  const theme: ITheme = { ...defaultTerminalTheme(variant), ...partial }

  const fallbackName =
    fileName?.replace(/\.(itermcolors|json|txt)$/i, "").trim() || "Custom"
  const name = explicitName?.trim() || schemeName || fallbackName

  // 选区色与前景色对比度过低时降级为半透明，避免选中区域文字不可读
  if (theme.selectionBackground && theme.foreground) {
    const ls = luminance(theme.selectionBackground)
    const lf = luminance(theme.foreground)
    const contrast = (Math.max(ls, lf) + 0.05) / (Math.min(ls, lf) + 0.05)
    if (contrast < 2.5 && /^#[0-9a-f]{6}$/.test(theme.selectionBackground)) {
      const m = /^#([0-9a-f]{6})$/.exec(theme.selectionBackground)
      const h = m?.[1]
      if (h) {
        const r = parseInt(h.slice(0, 2), 16)
        const g = parseInt(h.slice(2, 4), 16)
        const b = parseInt(h.slice(4, 6), 16)
        theme.selectionBackground = `rgba(${r}, ${g}, ${b}, 0.35)`
      }
    }
  }

  return { name, variant, theme }
}

/** 导出为 Windows Terminal 风格 scheme JSON（与 iterm2colorschemes.com 下载的 JSON 同构）。 */
export function toSchemeJson(name: string, theme: ITheme): string {
  const to6 = (v: string): string => {
    const normalized = normalizeThemeColor(v)
    if (normalized && /^#[0-9a-f]{6}$/.test(normalized)) return normalized
    if (normalized && /^#[0-9a-f]{8}$/.test(normalized)) return normalized.slice(0, 7)
    const m = /^rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*[\d.]+\s*\)$/.exec(v)
    if (m) {
      const hex = (n: number) => n.toString(16).padStart(2, "0")
      return `#${hex(Number(m[1]))}${hex(Number(m[2]))}${hex(Number(m[3]))}`
    }
    return "#000000"
  }
  const get = (key: TerminalThemeKey): string => to6(String(theme[key] ?? "#000000"))
  const scheme = {
    name,
    black: get("black"),
    red: get("red"),
    green: get("green"),
    yellow: get("yellow"),
    blue: get("blue"),
    purple: get("magenta"),
    cyan: get("cyan"),
    white: get("white"),
    brightBlack: get("brightBlack"),
    brightRed: get("brightRed"),
    brightGreen: get("brightGreen"),
    brightYellow: get("brightYellow"),
    brightBlue: get("brightBlue"),
    brightPurple: get("brightMagenta"),
    brightCyan: get("brightCyan"),
    brightWhite: get("brightWhite"),
    background: get("background"),
    foreground: get("foreground"),
    cursorColor: get("cursor"),
    selectionBackground: get("selectionBackground"),
  }
  return JSON.stringify(scheme, null, 2)
}

/** 主题指纹：按固定键序序列化，用于判断当前配色是否等于某个预设。 */
export function terminalThemeSignature(theme: ITheme): string {
  return TERMINAL_THEME_FIELDS.map((f) => theme[f.key] ?? "").join("|")
}
