import { computed, onBeforeUnmount, ref } from "vue"
import type { Ref } from "vue"
import type { Terminal } from "@xterm/xterm"
import {
  COMMAND_DICT,
  extractFirstWord,
  getCommandDesc,
  isBuiltinCommand,
  type Locale,
} from "@/data/commandDict"
import { CommandTrie } from "@/data/commandTrie"

const VISIBLE_COUNT = 5
const MAX_HISTORY = 500
const MAX_HISTORY_MATCHES = 3
const STORAGE_KEY = "ashell-command-history"
/** 旧版本黑名单数据（已废弃），加载时一次性清理 */
const LEGACY_BLOCKLIST_KEY = "ashell-command-blocklist"

export interface SuggestItem {
  cmd: string
  desc: string
  source: "history" | "dict" | "learned"
  /** 历史与学习项可单条移除；字典内置项不可 */
  deletable: boolean
}

// ---------------------------------------------------------------------------
// 字典命令名列表（静态）
// ---------------------------------------------------------------------------

const COMMON_COMMANDS = Object.keys(COMMAND_DICT)

// ---------------------------------------------------------------------------
// 模块级共享历史（所有终端 tab 共用同一份）
// ---------------------------------------------------------------------------

let commandHistory: string[] = []
let historySet = new Set<string>()
let loaded = false

function ensureLoaded() {
  if (loaded) return
  loaded = true
  try {
    localStorage.removeItem(LEGACY_BLOCKLIST_KEY)
  } catch {
    // ignore
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const arr = JSON.parse(raw) as unknown
      if (Array.isArray(arr)) {
        commandHistory = arr.filter(
          (s): s is string => typeof s === "string" && s.trim().length > 0,
        )
        historySet = new Set(commandHistory)
        for (const cmd of commandHistory) {
          learnCommand(cmd)
        }
      }
    }
  } catch {
    // ignore
  }
}

let saveTimer: number | null = null

function scheduleSave() {
  if (saveTimer !== null) return
  saveTimer = window.setTimeout(() => {
    saveTimer = null
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(commandHistory))
    } catch {
      // ignore
    }
  }, 1000)
}

function addToHistory(cmd: string) {
  ensureLoaded()
  const trimmed = cmd.trim()
  if (!trimmed) return
  if (historySet.has(trimmed)) {
    commandHistory = commandHistory.filter((c) => c !== trimmed)
  }
  commandHistory.push(trimmed)
  historySet.add(trimmed)
  if (commandHistory.length > MAX_HISTORY) {
    const removed = commandHistory.shift()
    if (removed !== undefined) {
      historySet.delete(removed)
      pruneLearnedWord(extractFirstWord(removed))
    }
  }
  learnCommand(trimmed)
  scheduleSave()
}

// ---------------------------------------------------------------------------
// 字典树：字典命令（静态）+ 从历史中动态学习的首词，前缀查找 O(m + k)
// ---------------------------------------------------------------------------

const dictTrie = new CommandTrie()
const trieCommands = new Set<string>(COMMON_COMMANDS)

for (const cmd of COMMON_COMMANDS) {
  dictTrie.insert(cmd)
}

/**
 * 从一条完整命令中提取首词（命令名），不在 Trie 中则插入。
 * trieCommands Set 去重，同一首词只 insert 一次。
 * ensureLoaded 时从 localStorage 历史回放重建，无需额外持久化。
 */
function learnCommand(fullCmd: string) {
  const firstWord = extractFirstWord(fullCmd)
  if (!firstWord || trieCommands.has(firstWord)) return
  dictTrie.insert(firstWord)
  trieCommands.add(firstWord)
}

/** 首词不再被任何历史引用且非内置时从 Trie 移除，与历史的 LRU 生命周期保持一致。 */
function pruneLearnedWord(word: string) {
  if (!word || isBuiltinCommand(word) || !trieCommands.has(word)) return
  if (commandHistory.some((c) => extractFirstWord(c) === word)) return
  trieCommands.delete(word)
  dictTrie.delete(word)
}

/** 删除一条历史记录：首词不再被其他历史引用时同步清出 Trie。 */
function removeHistoryItem(cmd: string) {
  ensureLoaded()
  commandHistory = commandHistory.filter((c) => c !== cmd)
  historySet.delete(cmd)
  pruneLearnedWord(extractFirstWord(cmd))
  scheduleSave()
}

/** 删除一个学习到的首词：仅从 Trie 移除；历史保留，再次执行会重新学习。 */
function removeLearnedWord(word: string) {
  dictTrie.delete(word)
  trieCommands.delete(word)
}

/**
 * 清空全部命令历史与学习的首词（设置页"清空命令历史"入口）。
 * Trie 重置为仅内置命令。
 */
export function clearCommandSuggestData() {
  commandHistory = []
  historySet = new Set()
  for (const word of [...trieCommands]) {
    if (!isBuiltinCommand(word)) {
      trieCommands.delete(word)
      dictTrie.delete(word)
    }
  }
  try {
    localStorage.removeItem(STORAGE_KEY)
  } catch {
    // ignore
  }
}

/**
 * 返回最近执行的命令列表（倒序，去重）。
 * 供模板命令"从历史选择"等功能使用。
 */
export function getRecentCommands(limit = 20): string[] {
  ensureLoaded()
  const result: string[] = []
  const seen = new Set<string>()
  for (let i = commandHistory.length - 1; i >= 0 && result.length < limit; i--) {
    const cmd = commandHistory[i]
    if (cmd && !seen.has(cmd)) {
      seen.add(cmd)
      result.push(cmd)
    }
  }
  return result
}

/**
 * 匹配策略：
 * - 历史命令：关键字子串匹配（includes），最近使用优先
 * - 字典命令：前缀匹配（Trie），字符序
 * 合并去重后返回。
 */
function matchCommands(prefix: string, locale: Locale): SuggestItem[] {
  ensureLoaded()
  const lower = prefix.toLowerCase()
  const result: SuggestItem[] = []
  const seen = new Set<string>()
  let historyCount = 0

  // 历史：子串匹配，倒序 = 最近使用优先，上限 MAX_HISTORY_MATCHES 条
  for (let i = commandHistory.length - 1; i >= 0; i--) {
    if (historyCount >= MAX_HISTORY_MATCHES) break
    const cmd = commandHistory[i]
    if (!cmd || seen.has(cmd) || cmd === prefix) continue
    if (cmd.toLowerCase().includes(lower)) {
      result.push({
        cmd,
        desc: getCommandDesc(cmd, locale),
        source: "history",
        deletable: true,
      })
      seen.add(cmd)
      historyCount++
    }
  }

  // 字典 + 学习：前缀匹配（Trie DFS 字符序）
  const dictMatches = dictTrie.search(prefix)
  for (const cmd of dictMatches) {
    if (seen.has(cmd) || cmd === prefix) continue
    const isBuiltin = isBuiltinCommand(cmd)
    result.push({
      cmd,
      desc: getCommandDesc(cmd, locale),
      source: isBuiltin ? "dict" : "learned",
      deletable: !isBuiltin,
    })
    seen.add(cmd)
  }

  return result
}

// ---------------------------------------------------------------------------
// 组合式函数
// ---------------------------------------------------------------------------

export function useCommandSuggest(options: {
  getTerm: () => Terminal | null
  containerRef: Ref<HTMLDivElement | null>
  sendData: (data: string) => boolean
  getLocale: () => Locale
  isEnabled: () => boolean
}) {
  const { getTerm, containerRef, sendData, getLocale, isEnabled } = options

  const suggestVisible = ref(false)
  const suggestX = ref(0)
  const suggestY = ref(0)
  /** 在 allMatches 中的全局选中下标 */
  const selectedIndex = ref(0)
  /** 可视窗口第一条在 allMatches 中的下标 */
  const scrollOffset = ref(0)
  /** 全部匹配结果（响应式，驱动 visibleItems 重算） */
  const allMatches = ref<SuggestItem[]>([])

  let inputBuffer = ""
  /** inputBuffer 是否可靠（escape 序列 / 方向键后变为 false） */
  let inputBufferReliable = true

  const visibleItems = computed(() =>
    allMatches.value.slice(scrollOffset.value, scrollOffset.value + VISIBLE_COUNT),
  )
  const totalMatches = computed(() => allMatches.value.length)
  /** 可视窗口内的选中下标（0-based） */
  const visibleSelectedIndex = computed(() => selectedIndex.value - scrollOffset.value)

  // ---- 定位 ----

  function updatePosition() {
    const t = getTerm()
    const container = containerRef.value
    if (!t || !container) return

    const screenEl = container.querySelector(".xterm-screen")
    if (!screenEl) return

    const containerRect = container.getBoundingClientRect()
    const screenRect = screenEl.getBoundingClientRect()

    const core = (t as unknown as {
      _core?: {
        _renderService?: {
          dimensions?: { css?: { cell?: { width: number; height: number } } }
        }
      }
    })._core
    const cell = core?._renderService?.dimensions?.css?.cell
    if (!cell) return

    const offsetX = screenRect.left - containerRect.left
    const offsetY = screenRect.top - containerRect.top
    const cursorX = t.buffer.active.cursorX
    const cursorY = t.buffer.active.cursorY

    let x = offsetX + cursorX * cell.width
    let y = offsetY + (cursorY + 1) * cell.height

    // 实际弹窗高度 = 实际可见项数 × 行高 + footer + 容器 padding
    const visibleCount = Math.min(allMatches.value.length, VISIBLE_COUNT)
    const hasFooter = allMatches.value.length > VISIBLE_COUNT
    const popupH = visibleCount * 28 + (hasFooter ? 20 : 0) + 8

    // 超出底部时翻到光标上方，底边贴齐光标行
    if (y + popupH > containerRect.height) {
      y = offsetY + cursorY * cell.height - popupH
      if (y < 0) y = 4
    }
    // 右侧溢出
    const popupW = 360
    if (x + popupW > containerRect.width) {
      x = containerRect.width - popupW - 8
    }
    if (x < 4) x = 4

    suggestX.value = x
    suggestY.value = y
  }

  // ---- 核心逻辑 ----

  let suggestRaf: number | null = null

  function cancelSuggestRaf() {
    if (suggestRaf !== null) {
      cancelAnimationFrame(suggestRaf)
      suggestRaf = null
    }
  }

  /**
   * rAF 节流的建议刷新入口。快速打字时每帧只做一次 Trie 匹配 + DOM 定位，
   * 避免每次按键都同步执行 matchCommands + getBoundingClientRect 阻塞主线程。
   */
  function updateSuggestions() {
    if (suggestRaf !== null) return
    suggestRaf = requestAnimationFrame(() => {
      suggestRaf = null
      doUpdateSuggestions()
    })
  }

  function doUpdateSuggestions() {
    if (!isEnabled()) {
      dismiss()
      return
    }
    const t = getTerm()
    // 备用屏幕（vim / htop 等 TUI）不弹建议
    if (t && t.buffer.active.type === "alternate") {
      dismiss()
      return
    }
    // shell 惯例：前导空格 = 不记入历史、不弹建议
    if (!inputBuffer || inputBuffer.startsWith(" ")) {
      dismiss()
      return
    }

    allMatches.value = matchCommands(inputBuffer, getLocale())
    if (allMatches.value.length === 0) {
      dismiss()
      return
    }

    selectedIndex.value = 0
    scrollOffset.value = 0
    suggestVisible.value = true
    updatePosition()
  }

  function dismiss() {
    cancelSuggestRaf()
    suggestVisible.value = false
    allMatches.value = []
    selectedIndex.value = 0
    scrollOffset.value = 0
  }

  /**
   * 从 xterm buffer 读取当前行，尝试剥离 shell 提示符后返回实际命令。
   * 用于 inputBuffer 不可靠时（方向键/光标移动后）的 Enter 回退学习。
   */
  function readCommandFromTerminal(): string {
    const t = getTerm()
    if (!t) return ""
    const buffer = t.buffer.active
    const line = buffer.getLine(buffer.cursorY)
    if (!line) return ""
    const fullLine = line.translateToString(true)
    if (!fullLine) return ""

    // 快速路径：inputBuffer 匹配行尾 -> 直接用
    if (inputBuffer && fullLine.endsWith(inputBuffer)) {
      return inputBuffer
    }

    // 尝试剥离 shell 提示符：查找 $ # % ❯ ➜ 等提示符尾字符（行首或非空格字符之后）
    const match = fullLine.match(/(?<=^|\S)[\$#%❯➜]\s+(.+)$/)
    if (match && match[1]) {
      return match[1].trim()
    }

    return ""
  }

  function moveSelection(delta: number) {
    if (allMatches.value.length === 0) return
    let next = selectedIndex.value + delta
    if (next < 0) next = allMatches.value.length - 1
    if (next >= allMatches.value.length) next = 0
    selectedIndex.value = next

    if (next < scrollOffset.value) {
      scrollOffset.value = next
    } else if (next >= scrollOffset.value + VISIBLE_COUNT) {
      scrollOffset.value = next - VISIBLE_COUNT + 1
    }
  }

  /**
   * 接受当前选中项，把补全内容发送到 shell。
   *
   * - 前缀精确匹配（含大小写）：只补后缀，如 "gi" -> "git" 发送 "t"
   * - 其他情况（子串匹配 / 大小写不同）：发退格删掉已输入内容，再发完整命令
   */
  function acceptSelected(): boolean {
    if (!suggestVisible.value || allMatches.value.length === 0) return false
    const selected = allMatches.value[selectedIndex.value]
    if (selected === undefined) return false

    if (selected.cmd.startsWith(inputBuffer)) {
      const suffix = selected.cmd.slice(inputBuffer.length)
      if (suffix) sendData(suffix)
    } else {
      const backspaces = "\x7f".repeat(inputBuffer.length)
      sendData(backspaces + selected.cmd)
    }
    inputBuffer = selected.cmd
    dismiss()
    return true
  }

  /** 鼠标点击指定可视项 */
  function acceptAt(visibleIdx: number) {
    const globalIdx = scrollOffset.value + visibleIdx
    if (globalIdx >= 0 && globalIdx < allMatches.value.length) {
      selectedIndex.value = globalIdx
      acceptSelected()
      getTerm()?.focus()
    }
  }

  /** 鼠标悬停高亮 */
  function hoverAt(visibleIdx: number) {
    const globalIdx = scrollOffset.value + visibleIdx
    if (globalIdx >= 0 && globalIdx < allMatches.value.length) {
      selectedIndex.value = globalIdx
    }
  }

  /**
   * 移除单条建议：历史项仅删该条记录，学习项仅从 Trie 删该首词。
   * 学习项的首词若因历史清理而离开 Trie，一并从列表移除。
   */
  function removeSuggestion(item: SuggestItem) {
    if (item.source === "history") {
      removeHistoryItem(item.cmd)
    } else if (item.source === "learned") {
      removeLearnedWord(item.cmd)
    } else {
      return
    }
    allMatches.value = allMatches.value.filter(
      (m) => m.cmd !== item.cmd && (m.source !== "learned" || trieCommands.has(m.cmd)),
    )
    if (allMatches.value.length === 0) {
      dismiss()
    } else if (selectedIndex.value >= allMatches.value.length) {
      selectedIndex.value = allMatches.value.length - 1
    }
    getTerm()?.focus()
  }

  // ---- 事件处理 ----

  /**
   * 在 term.onData 中调用，跟踪用户输入以维护 inputBuffer。
   * 仅观察，不修改数据流。
   *
   * inputBuffer 可靠性：线性输入时可靠；遇到 escape 序列（方向键 / Home / End 等）
   * 标记为不可靠，Enter 时回退到从 xterm buffer 读取实际命令行。
   */
  function handleOnData(data: string) {
    // TUI 应用（top/htop/vim 等）使用备用屏幕，其按键（如 q 退出）不应记入 inputBuffer
    const t = getTerm()
    if (t && t.buffer.active.type === "alternate") {
      inputBuffer = ""
      inputBufferReliable = true
      dismiss()
      return
    }

    // Escape 序列（方向键 / Home / End / bracketed paste 等）- inputBuffer 不可靠
    if (data.startsWith("\x1b")) {
      inputBufferReliable = false
      inputBuffer = ""
      dismiss()
      return
    }

    // 多行内容（粘贴含换行符）- 每行作为独立命令学习，末行作为新 inputBuffer
    if (data.length > 1 && /[\r\n]/.test(data)) {
      const lines = data.split(/[\r\n]+/)

      // 首行可能是当前 inputBuffer 的延续
      const firstLine = lines[0] ?? ""
      if (inputBufferReliable && inputBuffer) {
        const fullCmd = (inputBuffer + firstLine).trim()
        if (fullCmd && !fullCmd.startsWith(" ")) {
          addToHistory(fullCmd)
        }
      } else if (firstLine.trim()) {
        const cmd = firstLine.trim()
        if (!cmd.startsWith(" ")) {
          addToHistory(cmd)
        }
      }

      // 中间行是完整命令
      for (let i = 1; i < lines.length - 1; i++) {
        const cmd = (lines[i] ?? "").trim()
        if (cmd && !cmd.startsWith(" ")) {
          addToHistory(cmd)
        }
      }

      // 末行是新的当前输入
      inputBuffer = (lines[lines.length - 1] ?? "").replace(/[\x00-\x1f\x7f]/g, "")
      inputBufferReliable = true
      if (inputBuffer) {
        updateSuggestions()
      } else {
        dismiss()
      }
      return
    }

    // Enter
    if (data === "\r" || data === "\n") {
      let cmd = ""
      if (inputBufferReliable && inputBuffer && !inputBuffer.startsWith(" ")) {
        cmd = inputBuffer
      } else {
        // inputBuffer 不可靠，从 xterm buffer 读取实际命令
        cmd = readCommandFromTerminal()
      }
      if (cmd && !cmd.trimStart().startsWith(" ")) {
        addToHistory(cmd.trim())
      }
      inputBuffer = ""
      inputBufferReliable = true
      dismiss()
      return
    }
    // Backspace
    if (data === "\x7f" || data === "\b") {
      inputBuffer = inputBuffer.slice(0, -1)
      updateSuggestions()
      return
    }
    // Ctrl+C / Ctrl+U - 清行
    if (data === "\x03" || data === "\x15") {
      inputBuffer = ""
      inputBufferReliable = true
      dismiss()
      return
    }
    // Ctrl+W - 删除前一个单词
    if (data === "\x17") {
      inputBuffer = inputBuffer.replace(/\S+\s*$/, "")
      updateSuggestions()
      return
    }
    // Tab - 由 keydown 拦截器处理
    if (data === "\t") return
    // 其余控制字符 - 关闭弹窗
    if (data.length === 1 && data.charCodeAt(0) < 32) {
      dismiss()
      return
    }
    // 可打印字符（含粘贴）
    const printable = data.replace(/[\x00-\x1f\x7f]/g, "")
    if (printable) {
      inputBuffer += printable
      updateSuggestions()
    }
  }

  /**
   * 容器捕获阶段 keydown。弹窗可见时拦截导航 / 接受 / 关闭按键。
   * 返回 true 表示事件已消费（调用方无需再处理）。
   */
  function handleKeydown(e: KeyboardEvent): boolean {
    if (!suggestVisible.value) return false

    switch (e.key) {
      case "ArrowUp":
        e.preventDefault()
        e.stopPropagation()
        moveSelection(-1)
        return true
      case "ArrowDown":
        e.preventDefault()
        e.stopPropagation()
        moveSelection(1)
        return true
      case "Tab":
        e.preventDefault()
        e.stopPropagation()
        acceptSelected()
        return true
      case "Escape":
        e.preventDefault()
        e.stopPropagation()
        dismiss()
        return true
      case "Enter":
        // 回车直接执行用户已输入的内容，同时关闭弹窗
        dismiss()
        return false
      default:
        return false
    }
  }

  /** 鼠标点击终端区域时关闭弹窗 */
  function onMousedown() {
    dismiss()
  }

  onBeforeUnmount(() => {
    cancelSuggestRaf()
  })

  return {
    suggestVisible,
    suggestX,
    suggestY,
    visibleItems,
    visibleSelectedIndex,
    totalMatches,
    selectedIndex,
    handleOnData,
    handleKeydown,
    acceptSelected,
    acceptAt,
    hoverAt,
    removeSuggestion,
    dismissSuggest: dismiss,
    onSuggestMousedown: onMousedown,
  }
}
