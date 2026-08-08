<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"
import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import { WebglAddon } from "@xterm/addon-webgl"
import { WebLinksAddon } from "@xterm/addon-web-links"
import { Unicode11Addon } from "@xterm/addon-unicode11"
import { LigaturesAddon } from "@xterm/addon-ligatures"
import { SerializeAddon } from "@xterm/addon-serialize"
import { NIcon, NInput, NTooltip, NButton } from "naive-ui"
import {
  ArrowDownOutline as ArrowDownIcon,
  ArrowUpOutline as ArrowUpIcon,
  CloseOutline as CloseIcon,
  RefreshOutline,
  SparklesOutline as SparklesIcon,
  SendOutline as SendIcon,
  TimeOutline as HistoryIcon,
  BookOutline as DictIcon,
} from "@vicons/ionicons5"
import { openUrl } from "@tauri-apps/plugin-opener"
import {
  readText as tauriReadText,
  writeText as tauriWriteText,
} from "@tauri-apps/plugin-clipboard-manager"
import { useI18n } from "vue-i18n"
import { useApiStore } from "@/stores/api"
import { useTerminalStore } from "@/stores/terminal"
import { useBroadcastStore } from "@/stores/broadcast"
import { useStartupStore } from "@/stores/startup"
import { useSudoFill } from "@/composables/useSudoFill"
import { useAiSelection } from "@/composables/useAiSelection"
import { useTerminalSearch } from "@/composables/useTerminalSearch"
import { useTerminalProgress } from "@/composables/useTerminalProgress"
import { useCommandSuggest } from "@/composables/useCommandSuggest"
import type { Locale } from "@/data/commandDict"
import { buildTerminalWsUrl } from "@/api/sftp"
import { buildLocalTerminalWsUrl } from "@/api/local"
import { buildTelnetTerminalWsUrl } from "@/api/telnet"
import { buildSerialTerminalWsUrl } from "@/api/serial"
import type { TerminalTab } from "@/types"

interface Props {
  tab: TerminalTab
  active: boolean
  autoConnect?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  autoConnect: true,
})

type TermStatus = "connecting" | "connected" | "closed" | "error"

const emit = defineEmits<{
  (e: "sid-ready", tabKey: string, sid: string): void
  (e: "status-change", tabKey: string, status: TermStatus): void
  (e: "title-change", tabKey: string, title: string): void
  (e: "send-to-ai", tabKey: string, text: string): void
  (e: "close-tab", tabKey: string): void
}>()

const containerRef = ref<HTMLDivElement | null>(null)

let term: Terminal | null = null
let fitAddon: FitAddon | null = null
let webglAddon: WebglAddon | null = null
let webLinksAddon: WebLinksAddon | null = null
let unicode11Addon: Unicode11Addon | null = null
let ligaturesAddon: LigaturesAddon | null = null
// SerializeAddon 是按需加载（导出会话时才用），所以叫 ensure 拿，不在这里持久化
let serializeAddon: SerializeAddon | null = null
let ws: WebSocket | null = null
let resizeObserver: ResizeObserver | null = null
let themeObserver: MutationObserver | null = null
let heartbeatTimer: number | null = null
let disposed = false

const { t, locale } = useI18n()
const apiStore = useApiStore()
const termStore = useTerminalStore()
const broadcastStore = useBroadcastStore()
const startupStore = useStartupStore()

const { sudoArmed, armSudo, disarmSudo } = useSudoFill()

const {
  aiButtonVisible,
  aiButtonX,
  aiButtonY,
  aiPromptVisible,
  aiPromptText,
  onMouseUp,
  openAiPrompt,
  submitAiPrompt,
  cancelAiPrompt,
  onAiPromptKeydown,
} = useAiSelection({
  getTerm: () => term,
  containerRef,
  onSend: (text) => emit("send-to-ai", props.tab.key, text),
  isEnabled: () => startupStore.aiAssistantEnabled,
})

const {
  searchOpen,
  searchKeyword,
  searchResultText,
  searchToggles,
  openSearchBar,
  closeSearchBar,
  runSearch,
  onSearchInput,
  onSearchKeydown,
  toggleSearchFlag,
} = useTerminalSearch(() => term)

const {
  progressState,
  progressValue,
  loadProgress,
  disposeProgress,
  writeToTerm,
  resetTextProgress,
  syncTaskbar,
  clearTaskbar,
} = useTerminalProgress({
  getTerm: () => term,
  isActive: () => props.active,
})

const {
  suggestVisible,
  suggestX,
  suggestY,
  visibleItems: suggestVisibleItems,
  visibleSelectedIndex: suggestVisibleSelectedIndex,
  totalMatches: suggestTotalMatches,
  selectedIndex: suggestSelectedIndex,
  handleOnData: onSuggestData,
  handleKeydown: onSuggestKeydown,
  acceptAt: acceptSuggestAt,
  hoverAt: hoverSuggestAt,
  removeSuggestion,
  onSuggestMousedown,
} = useCommandSuggest({
  getTerm: () => term,
  containerRef,
  sendData: (data) => sendJson({ kind: "cmd", data }),
  getLocale: () => (locale.value === "zh-CN" ? "zh-CN" : "en-US") as Locale,
  isEnabled: () => termStore.commandSuggestEnabled,
})

/** 发送 JSON 控制帧；连接不可用或发送失败时返回 false。 */
function sendJson(msg: unknown): boolean {
  if (!ws || ws.readyState !== WebSocket.OPEN) return false
  try {
    ws.send(JSON.stringify(msg))
    return true
  } catch {
    return false
  }
}

/**
 * 把任意输入字节注入本 tab 的 ws（不经过 term.onData）。
 * 给 broadcast fanout 用：作为目标 tab 收外部源 tab 的输入。
 */
function sendInputToWs(data: string) {
  sendJson({ kind: "cmd", data })
}

/**
 * 向终端发送一条命令并执行（追加 \r）。
 * 供模板命令等外部调用方使用。
 */
function sendCommand(cmd: string) {
  sendJson({ kind: "cmd", data: cmd + "\r" })
}

// ===== 离线恢复状态浮层按钮 =====
const showReconnectBtn = ref(false)

// ===== 连接中状态浮层 =====
const currentStatus = ref<TermStatus>("connecting")
const showConnectingOverlay = ref(false)
let connectingOverlayTimer: number | null = null

function applyTheme() {
  if (!term) return
  term.options.theme = termStore.getActiveTerminalTheme()
}

function setStatus(status: TermStatus) {
  currentStatus.value = status
  emit("status-change", props.tab.key, status)
  if (status === "connecting") {
    if (connectingOverlayTimer === null) {
      connectingOverlayTimer = window.setTimeout(() => {
        if (currentStatus.value === "connecting") {
          showConnectingOverlay.value = true
        }
        connectingOverlayTimer = null
      }, 300)
    }
  } else {
    if (connectingOverlayTimer !== null) {
      window.clearTimeout(connectingOverlayTimer)
      connectingOverlayTimer = null
    }
    showConnectingOverlay.value = false
  }
}

const connectingLabel = computed(() => {
  const kind = props.tab.kind
  if (kind === "local") return t("terminal.connectingLocal")
  if (kind === "serial") return t("terminal.connectingSerial")
  return t("terminal.connecting")
})

const connectingTarget = computed(() => {
  const tab = props.tab
  if (tab.kind === "local") {
    return tab.shell ? `Local · ${tab.shell}` : "Local"
  }
  if (tab.kind === "serial") return null
  const info = tab.hostInfo
  if (!info) return tab.title
  const user = info.username ? `${info.username}@` : ""
  const port = info.port && info.port !== "22" ? `:${info.port}` : ""
  return `${user}${info.addr}${port}`
})

function clearHeartbeat() {
  if (heartbeatTimer !== null) {
    window.clearInterval(heartbeatTimer)
    heartbeatTimer = null
  }
}

function startHeartbeat() {
  clearHeartbeat()
  heartbeatTimer = window.setInterval(() => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      try {
        ws.send(JSON.stringify({ kind: "ping" }))
      } catch {
        // ignore
      }
    }
  }, 30_000)
}

function safeFit() {
  if (!fitAddon || !term) return
  const prevRows = term.rows
  try {
    fitAddon.fit()
  } catch {
    // 容器尺寸为 0 时 fit 会抛错，忽略
    return
  }
  // rows 增加（窗口最大化 / 还原到大尺寸）时主动滚到底。
  // 背景：开启 windowsPty 后 xterm 关闭自身 reflow，rows 增加时不会把 scrollback
  // 里的旧内容拉回 viewport，而是在 viewport 底部留出空白行——视觉上就是"最大化
  // 后底部多了一大片空、光标卡在中间偏上"。手动 scrollToBottom 把空白挤到顶部，
  // 光标贴底，这才是用户期望的"展开向下扩"行为。
  if (term.rows > prevRows) {
    try {
      term.scrollToBottom()
    } catch {
      // ignore
    }
  }
}

let lastSentCols = 0
let lastSentRows = 0

function sendResize() {
  if (!term) return
  const cols = term.cols
  const rows = term.rows
  // 去重：同样的尺寸不重复发。ConPTY 对反复传同尺寸/震荡尺寸敏感，PowerShell 一侧
  // 会出现 prompt 重绘与 xterm 渲染缓冲脱节、最大化-还原后光标错位。
  if (cols === lastSentCols && rows === lastSentRows) return
  if (sendJson({ kind: "resize", cols, rows })) {
    lastSentCols = cols
    lastSentRows = rows
  }
}

let fitRaf: number | null = null
let fitFocusAfter = false

/**
 * 统一的 rAF 节流 fit 入口：ResizeObserver / window.resize / Tauri onResized
 * 三条路径在一帧内无论触发多少次，最多只执行一次 fit。
 *
 * 最大化/还原动画期间这些事件源高频触发，若不收敛，终端会在一帧内被 fit 多次
 * （DOM 测量 + xterm reflow + WebGL 画布重建），是窗口缩放卡顿的主因。
 *
 * 只调用 fit；让 fit 内部的 cols/rows 变化驱动 term.onResize → sendResize。
 * 注意 *不要* 在这里强制 sendResize：ConPTY 对短时间内反复传同一尺寸/震荡尺寸非常敏感，
 * 重复或冗余的 resize 会让 PowerShell 的 prompt 重绘与 xterm 已渲染缓冲脱节，
 * 出现 "最大化-还原后输入位置错乱"。
 */
function scheduleFit(focusAfter = false) {
  if (focusAfter) fitFocusAfter = true
  if (fitRaf !== null) return
  fitRaf = window.requestAnimationFrame(() => {
    fitRaf = null
    safeFit()
    // 最大化/最小化/还原后重新聚焦当前 tab，避免窗口控件抢走光标导致下一次输入无响应。
    // 仅激活 tab 才聚焦，否则后台 tab 的 TerminalView 会把焦点抢走。
    if (fitFocusAfter && props.active) term?.focus()
    fitFocusAfter = false
  })
}

/** 窗口最大化/还原/全屏/DPI 切换时调用（浏览器 window.resize 兜底路径）。 */
function onWindowResize() {
  scheduleFit(true)
}

async function readClipboard(): Promise<string> {
  // 优先走 Tauri clipboard-manager 插件（Rust 端直读 NSPasteboard / Win clipboard），
  // 这样 macOS WKWebView 不会弹"允许粘贴"系统提示，触摸板右键也能拿到内容
  // （WKWebView 下 navigator.clipboard.readText 对触摸板右键不算 user activation，会静默返回空串）。
  try {
    const text = await tauriReadText()
    if (typeof text === "string") return text
  } catch {
    // 插件不可用或权限被拒，降级到浏览器 API
  }
  try {
    if (navigator.clipboard?.readText) {
      return await navigator.clipboard.readText()
    }
  } catch {
    // ignore
  }
  return ""
}

async function writeClipboard(text: string) {
  if (!text) return
  try {
    await tauriWriteText(text)
    return
  } catch {
    // ignore
  }
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
    }
  } catch {
    // ignore
  }
}

function pasteToTerminal(text: string) {
  if (!text || !term) return
  if (ws && ws.readyState === WebSocket.OPEN) {
    sendJson({ kind: "cmd", data: text })
  } else {
    term.paste(text)
  }
}

function onSelectionChange() {
  if (!term) return
  const sel = term.getSelection()
  if (!sel) {
    aiButtonVisible.value = false
  }
  const action = termStore.leftClickAction
  if (action !== "copyOnSelect" && action !== "copyAndMiddlePaste") return
  if (sel) void writeClipboard(sel)
}

async function onContextMenu(e: MouseEvent) {
  const action = termStore.rightClickAction
  if (action === "contextMenu") {
    // 放行 webview 默认菜单；stopPropagation 阻止 main.ts 里的全局 preventDefault
    e.stopPropagation()
    return
  }
  e.preventDefault()
  if (action === "none") return
  if (!term) return
  if (action === "smart") {
    const sel = term.getSelection()
    if (sel) {
      await writeClipboard(sel)
      term.clearSelection()
      return
    }
    const text = await readClipboard()
    pasteToTerminal(text)
    return
  }
  // action === "paste"
  const text = await readClipboard()
  pasteToTerminal(text)
}

async function onAuxClick(e: MouseEvent) {
  if (e.button !== 1) return
  const action = termStore.leftClickAction
  if (action !== "copyAndMiddlePaste" && action !== "middlePasteOnly") return
  e.preventDefault()
  const text = await readClipboard()
  pasteToTerminal(text)
}

function onWheel(e: WheelEvent) {
  if (!(e.ctrlKey || e.metaKey)) return
  if (e.deltaY === 0) return
  e.preventDefault()
  const delta = e.deltaY < 0 ? 1 : -1
  termStore.setFontSize(termStore.fontSize + delta)
}

function applyTermOptions() {
  if (!term) return
  term.options.fontSize = termStore.fontSize
  term.options.fontFamily = termStore.fontFamily
  term.options.cursorStyle = termStore.cursorStyle
  term.options.cursorBlink = termStore.cursorBlink
  installCursorBlinkGuard()
  safeFit()
}

/**
 * 拦截 shell 发出的关闭光标闪烁的转义序列。
 *
 * 本地 shell（尤其是 zsh + p10k / oh-my-zsh 主题）启动时常发
 * DECSCUSR（[2 q = steady block）或 DEC private mode（[?12l
 * = disable blink）来关闭光标闪烁，覆盖用户在设置中
 * 开启的 cursorBlink。这里在 xterm parser 层拦截这些序列：
 *
 * - [?12l（关闭闪烁）：当 cursorBlink=true 时吞掉
 * - [N q（DECSCUSR）：将 steady 变体（2/4/6）转为
 *   对应的 blinking 变体（1/3/5），保留形状选择但强制闪烁
 *
 * vim 等应用在 cursorBlink=false 时仍可正常设置 steady 光标。
 *
 * 只注册一次：xterm 的 registerCsiHandler 是追加进数组、不去重的，而
 * applyTermOptions 会随字体设置变化高频触发（Ctrl+滚轮缩放），重复注册
 * 会让 parser handler 列表无界增长。回调内部动态读取 termStore.cursorBlink，
 * 设置变化无需重注册。
 */
let cursorGuardInstalled = false

function installCursorBlinkGuard() {
  if (!term || cursorGuardInstalled) return
  cursorGuardInstalled = true
  const t = term

  // 拦截 [?12l（disable cursor blink）
  t.parser.registerCsiHandler(
    { intermediates: "", prefix: "?", final: "l" },
    (params) => {
      if (params[0] === 12 && termStore.cursorBlink) {
        return true // swallow
      }
      return false
    },
  )

  // 拦截 DECSCUSR [{n} q：将 steady 变体转为 blinking
  // 必须返回 true 吞掉原序列，否则 xterm 默认 handler 会用
  // 原始参数（steady）再次关闭 blink。
  t.parser.registerCsiHandler(
    { intermediates: "", prefix: "", final: "q" },
    (params) => {
      if (!termStore.cursorBlink) return false
      const n = params[0] ?? 0
      if (n === 2) {
        t.options.cursorStyle = "block"
        t.options.cursorBlink = true
        return true
      }
      if (n === 4) {
        t.options.cursorStyle = "underline"
        t.options.cursorBlink = true
        return true
      }
      if (n === 6) {
        t.options.cursorStyle = "bar"
        t.options.cursorBlink = true
        return true
      }
      return false // 0/1/3/5 已是 blinking 变体，交给默认 handler
    },
  )
}

/**
 * 覆盖 xterm Viewport._sync：在 alternate screen 上对 onScroll 触发的 _sync 加 rAF 节流。
 *
 * 问题：bufferService.onScroll 直接调用 _sync()（onResize / onBufferActivate 走
 * queueSync 的 rAF 节流，唯独 onScroll 不走），TUI 底部频繁输出时每次 buffer scroll
 * 都立即触发 setScrollDimensions -> Scrollable.onScroll -> 滚动条 render/reveal 循环，
 * 造成垂直抖动。
 *
 * 方案：alternate screen 上，ydisp === undefined（来自 onScroll 无参调用）时改走
 * 自定义 rAF 节流，一帧内多次 scroll 合并为一次；ydisp !== undefined（来自 queueSync
 * 的 rAF 回调）时直接执行，不阻断正常同步。normal screen 走原始 _sync。
 *
 * 与 onCompositionStart 一样通过 term._core 访问 xterm 内部 API。
 * 升级 xterm 7.x 后需检查 _viewport._sync 的内部结构是否变化。
 */
function installAltScreenScrollFix() {
  if (!term) return
  const core = (term as unknown as {
    _core?: {
      _viewport?: {
        _sync: (ydisp?: number) => void
      }
    }
  })._core

  const viewport = core?._viewport
  if (!viewport) return

  const originalSync = viewport._sync.bind(viewport)
  let scrollRaf: number | null = null

  // 安全包装：_renderService 在终端初始化/销毁期间可能处于不一致状态，
  // dimensions getter 会抛 TypeError。原始 _sync 的 !this._renderService guard
  // 只检查引用是否为 falsy，不覆盖对象存在但内部 renderer 未就绪的情况。
  function safeSync(ydisp?: number) {
    try {
      originalSync(ydisp)
    } catch {
      // ignore
    }
  }

  viewport._sync = function (ydisp?: number) {
    if (!term || term.buffer.active.type !== "alternate") {
      safeSync(ydisp)
      return
    }
    // ydisp !== undefined: 来自 queueSync 的 rAF 回调，直接执行
    if (ydisp !== undefined) {
      if (scrollRaf !== null) {
        cancelAnimationFrame(scrollRaf)
        scrollRaf = null
      }
      safeSync(ydisp)
      return
    }
    // ydisp === undefined: 来自 bufferService.onScroll，走 rAF 节流
    if (scrollRaf !== null) return
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = null
      safeSync()
    })
  }
}

// ===== Addon 生命周期 =====

function loadWebgl() {
  if (!term || webglAddon) return
  try {
    const addon = new WebglAddon()
    // GPU 上下文丢失（如外接显示器切换、长时间休眠、驱动崩溃）时及时 dispose，
    // 让 xterm 自动回退到 DOM 渲染器；不处理会留一片黑画布直到刷新页面。
    addon.onContextLoss(() => {
      try {
        addon.dispose()
      } catch {
        // ignore
      }
      if (webglAddon === addon) webglAddon = null
    })
    term.loadAddon(addon)
    webglAddon = addon
  } catch (e) {
    // 老 WebKit / 受限 GPU 环境会抛错，无声回退
    console.warn("[ashell] failed to enable WebGL renderer:", e)
    webglAddon = null
  }
}

function disposeWebgl() {
  if (!webglAddon) return
  try {
    webglAddon.dispose()
  } catch {
    // ignore
  }
  webglAddon = null
}

function loadWebLinks() {
  if (!term || webLinksAddon) return
  // 链接被点击时统一走 Tauri opener 插件 → 系统默认浏览器，
  // 避免 Tauri/wry 默认 window.open 在当前 scope 下被静默吞掉。
  webLinksAddon = new WebLinksAddon((event, uri) => {
    event.preventDefault()
    void openUrl(uri).catch((err) => console.error(t("common.openLinkFailed"), err))
  })
  term.loadAddon(webLinksAddon)
}

function disposeWebLinks() {
  if (!webLinksAddon) return
  try {
    webLinksAddon.dispose()
  } catch {
    // ignore
  }
  webLinksAddon = null
}

function loadUnicode11() {
  if (!term || unicode11Addon) return
  unicode11Addon = new Unicode11Addon()
  term.loadAddon(unicode11Addon)
  term.unicode.activeVersion = "11"
}

function disposeUnicode11() {
  if (!unicode11Addon || !term) return
  // 切回默认 Unicode 6 表后再 dispose addon
  term.unicode.activeVersion = "6"
  try {
    unicode11Addon.dispose()
  } catch {
    // ignore
  }
  unicode11Addon = null
}

function loadLigatures() {
  if (!term || ligaturesAddon) return
  ligaturesAddon = new LigaturesAddon()
  term.loadAddon(ligaturesAddon)
  // ligatures 启用时 webgl 的字形 atlas 已经按"无连字"生成；必须重建 webgl 才能让
  // OpenType 连字 feature 落到贴图上（这个交互在 xterm 官方 demo 里也是同样处理）。
  if (webglAddon) {
    disposeWebgl()
    loadWebgl()
  }
}

function disposeLigatures() {
  if (!ligaturesAddon) return
  try {
    ligaturesAddon.dispose()
  } catch {
    // ignore
  }
  ligaturesAddon = null
  // 同样需要让 webgl 重建一次贴图，回退到无连字的字形 atlas
  if (webglAddon) {
    disposeWebgl()
    loadWebgl()
  }
}

/** 懒加载 SerializeAddon。仅在导出会话时调用。 */
function ensureSerializeAddon(): SerializeAddon | null {
  if (!term) return null
  if (serializeAddon) return serializeAddon
  const addon = new SerializeAddon()
  term.loadAddon(addon)
  serializeAddon = addon
  return addon
}

/** 由父组件触发：把当前终端缓冲区导出为带 ANSI 颜色的字符串。 */
function serializeSession(): string {
  const addon = ensureSerializeAddon()
  if (!addon) return ""
  return addon.serialize()
}

function onContainerKeydown(e: KeyboardEvent) {
  // 命令建议弹窗的导航/接受/关闭按键拦截
  if (onSuggestKeydown(e)) return
  // Ctrl+F / Cmd+F 唤起搜索条；放在容器层级捕获，避免被 xterm 拦走。
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "f") {
    if (!termStore.searchHotkeyEnabled) return
    e.preventDefault()
    e.stopPropagation()
    openSearchBar()
  }
}

/**
 * 输入法 composition 开始前强制重定位 helper textarea。
 *
 * 这是 xterm.js #5734 / PR #5759 的 backport：xterm 的 `_syncTextArea()` 在
 * `isComposing === true` 时会提前返回，composition 一旦开始 textarea 位置即被冻结。
 * 当 TUI（典型是 Claude Code 这类 agentic CLI 画占位提示文本）在用户首次上屏
 * composition 之前刚好重绘/移动光标，IME 候选窗会锚定到过期的光标坐标，落在
 * 「占位文本末尾」而非真实光标处。官方修复（xterm 7.0.0）在 `compositionstart`
 * 回调里、置 isComposing 之前补一次 `_syncTextArea()`，之后再 `updateCompositionElements()`。
 *
 * 本项目用 6.0.0，没有该修复：这里在容器上用捕获阶段监听抢在 xterm 自身的
 * `compositionstart` 监听之前跑 `_syncTextArea()`；`updateCompositionElements()` 需
 * isComposing 为 true 才生效，故推迟到微任务里（此时 xterm 的 compositionstart 已执行完）。
 * 升级到 xterm 7.x 后此 backport 变成无害冗余，可移除。
 */
function onCompositionStart() {
  if (!term) return
  const core = (term as unknown as {
    _core?: {
      _syncTextArea?: () => void
      _compositionHelper?: { updateCompositionElements?: () => void }
    }
  })._core
  if (!core) return
  core._syncTextArea?.()
  const helper = core._compositionHelper
  if (helper) queueMicrotask(() => helper.updateCompositionElements?.())
}

/** 断开当前 ws 并置空事件回调，避免被替换的旧连接再触发 onclose 副作用或写终端。 */
function teardownWs() {
  if (!ws) return
  try {
    ws.onmessage = null
    ws.onerror = null
    ws.onclose = null
    ws.onopen = null
    ws.close()
  } catch {
    // ignore
  }
  ws = null
}

async function connectWs(opts: { newSession?: boolean } = {}) {
  if (disposed) return
  // 防御性清理：正常调用方（onMounted / reconnect）此时 ws 应为 null
  teardownWs()
  showReconnectBtn.value = false
  const isLocal = props.tab.kind === "local"
  const isTelnet = props.tab.kind === "telnet"
  const isSerial = props.tab.kind === "serial"
  if (!isLocal && (props.tab.hostId === undefined || props.tab.hostId === null)) {
    term?.writeln("\x1b[31m[ashell] missing hostId; cannot connect.\x1b[0m")
    setStatus("error")
    showReconnectBtn.value = true
    return
  }

  // 等 ApiInfo 就绪
  if (!apiStore.ready) {
    try {
      await apiStore.init()
    } catch {
      // ignore，下方 buildXxxWsUrl 会再次尝试 getApiInfo
    }
  }

  const cols = term?.cols ?? 80
  const rows = term?.rows ?? 24

  let url: string
  try {
    if (isLocal) {
      url = await buildLocalTerminalWsUrl({
        sid: opts.newSession ? undefined : props.tab.sid,
        shell: props.tab.shell ?? undefined,
      })
    } else if (isTelnet) {
      url = await buildTelnetTerminalWsUrl(props.tab.hostId as number, {
        sid: opts.newSession ? undefined : props.tab.sid,
      })
    } else if (isSerial) {
      url = await buildSerialTerminalWsUrl(props.tab.hostId as number, {
        sid: opts.newSession ? undefined : props.tab.sid,
      })
    } else {
      url = await buildTerminalWsUrl(props.tab.hostId as number, {
        sid: opts.newSession ? undefined : props.tab.sid,
        cols,
        rows,
      })
    }
  } catch (e) {
    term?.writeln(`\x1b[31m[ashell] failed to build ws url: ${String(e)}\x1b[0m`)
    setStatus("error")
    showReconnectBtn.value = true
    return
  }

  if (disposed) return

  setStatus("connecting")
  let socket: WebSocket
  try {
    socket = new WebSocket(url)
  } catch (e) {
    term?.writeln(`\x1b[31m[ashell] failed to open ws: ${String(e)}\x1b[0m`)
    setStatus("error")
    showReconnectBtn.value = true
    return
  }
  socket.binaryType = "arraybuffer"
  ws = socket

  socket.onopen = () => {
    // 等待服务端推 ready
  }

  socket.onmessage = (event: MessageEvent) => {
    if (!term) return
    const data = event.data
    if (data instanceof ArrayBuffer) {
      writeToTerm(new Uint8Array(data))
      return
    }
    if (typeof data === "string") {
      try {
        const msg = JSON.parse(data) as { kind?: string; sid?: string }
        if (msg.kind === "ready" && typeof msg.sid === "string") {
          emit("sid-ready", props.tab.key, msg.sid)
          setStatus("connected")
          // 服务端可能采用了客户端建议的尺寸，确保再同步一次
          sendResize()
          startHeartbeat()
          if (props.active) {
            requestAnimationFrame(() => term?.focus())
          }
          return
        }
        if (msg.kind === "pong") {
          return
        }
        if (msg.kind === "sudo_prompt") {
          armSudo()
          return
        }
        // 兜底：未知 JSON 文本帧也写入终端
        writeToTerm(data)
      } catch {
        writeToTerm(data)
      }
      return
    }
    // Blob 等其他类型，尝试转 ArrayBuffer
    if (data instanceof Blob) {
      void data
        .arrayBuffer()
        .then((buf) => {
          if (term) writeToTerm(new Uint8Array(buf))
        })
        .catch(() => {
          // ignore
        })
    }
  }

  socket.onerror = () => {
    setStatus("error")
    showReconnectBtn.value = true
  }

  socket.onclose = (ev: CloseEvent) => {
    clearHeartbeat()
    resetTextProgress()
    disarmSudo()
    setStatus("closed")
    // local 终端 shell 进程已退出，无法重连，直接关闭 tab
    if (props.tab.kind === "local") {
      emit("close-tab", props.tab.key)
      return
    }
    showReconnectBtn.value = true
    if (term) {
      const reason = ev.reason ? `: ${ev.reason}` : ""
      term.writeln(`\r\n\x1b[31m[ashell] connection closed (code=${ev.code})${reason}\x1b[0m`)
      term.writeln(`\x1b[33m[ashell] ${t("terminal.sessionClosed")}\x1b[0m`)
    }
  }
}

onMounted(() => {
  if (!containerRef.value) return

  // 仅当此 tab 是本地 PTY 且宿主是 Windows 时启用 windowsPty。
  // 这会让 xterm.js 关闭自身的 reflow，并修正 ConPTY 的 scrollback/rows 增长行为，
  // 解决 "小窗输入 → 最大化 → 还原 → 再最大化" 后光标位置与渲染缓冲漂移、错位的问题。
  // SSH tab 不要开：远端是 Linux/Mac PTY，xterm 自带 reflow 才是对的。
  const isLocalOnWindows =
    props.tab.kind === "local" &&
    typeof navigator !== "undefined" &&
    /Win/i.test(navigator.platform || "")

  term = new Terminal({
    fontFamily: termStore.fontFamily,
    fontSize: termStore.fontSize,
    cursorBlink: termStore.cursorBlink,
    cursorStyle: termStore.cursorStyle,
    scrollback: termStore.scrollback,
    allowProposedApi: true,
    theme: termStore.getActiveTerminalTheme(),
    ...(isLocalOnWindows
      ? { windowsPty: { backend: "conpty" as const } }
      : {}),
  })
  fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(containerRef.value)

  // 按用户设置加载可选 addon。webgl 必须在 term.open 之后才能 loadAddon。
  // ligatures 在 webgl 之后加载：先 webgl 建好渲染器，再 ligatures 触发一次 webgl 重建
  // 让贴图按连字 feature 重新出。
  if (termStore.webglEnabled) loadWebgl()
  if (termStore.ligaturesEnabled) loadLigatures()
  if (termStore.webLinksEnabled) loadWebLinks()
  if (termStore.unicode11Enabled) loadUnicode11()
  if (termStore.progressEnabled) loadProgress()

  // WebGL 渲染器加载（尤其是 ligatures 触发 dispose+reload）后会重建渲染器，
  // 新渲染器可能未正确继承构造函数里的 cursorBlink/cursorStyle 选项。
  // 显式重新设置一次，强制渲染器刷新光标状态。
  applyTermOptions()

  installAltScreenScrollFix()

  term.onData((data: string) => {
    if (sudoArmed.value) {
      if (data === "\r") {
        // 回车确认 -> 发送 sudo_fill，后端注入密码
        disarmSudo()
        sendJson({ kind: "sudo_fill" })
        return
      }
      // 用户输入了其他字符 -> 放弃自动填充，恢复正常输入
      disarmSudo()
    }

    onSuggestData(data)

    sendJson({ kind: "cmd", data })
    // 广播：当本 tab 是当前 source（且广播已激活）时，把同一份输入复制到所有 target tab。
    // 经 ws 注入而非 term.onData，所以不会触发目标 tab 的回环。
    if (broadcastStore.enabled) {
      const source = broadcastStore.effectiveSource(
        props.active ? props.tab.key : null,
        props.active ? broadcastStore.windowId : null,
      )
      if (source === broadcastStore.globalKey(props.tab.key)) {
        broadcastStore.fanout(props.tab.key, data)
      }
    }
  })

  term.onResize(() => {
    sendResize()
  })

  // shell 通过 OSC 0/2 设置的窗口标题同步到 tab 标题
  term.onTitleChange((title) => {
    const trimmed = title.trim()
    if (trimmed) emit("title-change", props.tab.key, trimmed)
  })

  term.onSelectionChange(onSelectionChange)

  containerRef.value.addEventListener("contextmenu", onContextMenu)
  containerRef.value.addEventListener("auxclick", onAuxClick)
  containerRef.value.addEventListener("mouseup", onMouseUp)
  containerRef.value.addEventListener("mousedown", onSuggestMousedown)
  containerRef.value.addEventListener("wheel", onWheel, { passive: false })
  // Ctrl+F 在捕获阶段拦截：xterm 内部会处理一部分快捷键，必须比它先动手。
  containerRef.value.addEventListener("keydown", onContainerKeydown, true)
  // compositionstart 在捕获阶段抢在 xterm 自身监听之前重定位 helper textarea，
  // 修复 TUI 占位文本下 IME 候选窗锚定到过期光标的问题（见 onCompositionStart）。
  containerRef.value.addEventListener("compositionstart", onCompositionStart, true)

  themeObserver = new MutationObserver(() => applyTheme())
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-ashell-theme"],
  })

  resizeObserver = new ResizeObserver(() => {
    // 所有 tab 都要 fit：窗口最大化/还原/外接显示器切换时，非激活 tab 的容器尺寸也会跟着改，
    // 切回去后必须是已经 fit 过的状态，否则光标位置与服务端实际行列不一致。
    // 走 scheduleFit 的 rAF 节流，避免动画期间高频 fit。
    scheduleFit()
  })
  resizeObserver.observe(containerRef.value)

  // 兜底：window 的 resize 事件覆盖最大化/还原/全屏/DPI 切换 — Chromium 在某些路径下
  // 容器尺寸变化时不一定会触发 ResizeObserver（尤其是 visibility 切换的瞬间）。
  window.addEventListener("resize", onWindowResize)

  if (props.active) {
    // rAF: 等浏览器完成首次 layout/paint 后再 focus，否则 xterm textarea
    // 未完成布局，focus 无效，cursor blink 不会启动。
    requestAnimationFrame(() => term?.focus())
  }

  // 把"向本 tab ws 注入字节"的能力注册到 broadcast store；所有 tab 都注册（不只是 target）
  // 是因为 source/target 分配可以在运行时切换，提前注册避免抖动时漏接首字节。
  broadcastStore.registerSender(props.tab.key, sendInputToWs)

  if (props.autoConnect) {
    void connectWs()
  } else {
    // 由持久化恢复出来的骨架 tab：先不发起 ws，提示用户手动连接。
    setStatus("closed")
    showReconnectBtn.value = true
    term.writeln(`\x1b[33m[ashell] ${t("terminal.sessionRestored")}\x1b[0m`)
  }
})

/** 用户取消正在建立的连接。 */
function cancelConnect() {
  teardownWs()
  clearHeartbeat()
  resetTextProgress()
  disarmSudo()
  setStatus("closed")
  showReconnectBtn.value = true
  term?.writeln(`\r\n\x1b[33m[ashell] ${t("terminal.connectingCancelled")}\x1b[0m`)
}

/** 关闭当前 ws；由右键菜单"断开连接"调用。保留 xterm 实例和缓冲区。 */
function disconnect() {
  clearHeartbeat()
  resetTextProgress()
  disarmSudo()
  teardownWs()
  setStatus("closed")
  term?.writeln("\r\n\x1b[33m[ashell] disconnected by user\x1b[0m")
}

/** 重新建立 ws 与 ssh 会话。保留 xterm 输出缓冲区与历史。 */
async function reconnect() {
  showReconnectBtn.value = false
  // 先优雅断开旧连接（不要触发 onclose 的 setStatus 误覆盖）
  clearHeartbeat()
  resetTextProgress()
  disarmSudo()
  teardownWs()
  // 新会话需要再发一次 resize，重置去重状态
  lastSentCols = 0
  lastSentRows = 0
  term?.writeln("\r\n\x1b[36m[ashell] reconnecting...\x1b[0m")
  await connectWs({ newSession: true })
}

/** 由 App.vue 在 Tauri 窗口尺寸/状态变化（最大化/最小化/还原）时调用：
 * 重新 fit、并把焦点找回当前激活的终端。
 *
 * 单独走这条路径是因为 Tauri/wry 在最大化/还原时 *不一定* 派发浏览器 window.resize
 * 事件——只有 Tauri 自己的 appWindow.onResized 才是权威事件源。
 */
function relayout() {
  scheduleFit(true)
}

defineExpose({ disconnect, reconnect, relayout, serializeSession, sendCommand })

watch(
  () => props.active,
  (active) => {
    if (active) {
      // 切回当前 tab 时重新 fit + focus；fit 内部如果 cols/rows 变了会自动触发
      // term.onResize → sendResize，不要在这里手动重发，避免给 ConPTY 重复尺寸。
      // flush:'post' 确保 v-show 已 patch DOM；rAF 等浏览器完成 layout/paint 后再 fit/focus，
      // 否则容器从 display:none 切回后 xterm textarea 未完成布局，focus 无效。
      requestAnimationFrame(() => {
        safeFit()
        term?.focus()
      })
      // 切回当前 tab：把它当前的进度状态重新推到任务栏（前一个 tab 切走时已清零）
      syncTaskbar()
    } else {
      // 切走时清任务栏进度，避免后台 tab 的旧进度残留在 Windows 任务栏图标上
      clearTaskbar()
    }
  },
  { flush: "post" },
)

watch(
  () => [
    termStore.fontSize,
    termStore.fontFamily,
    termStore.cursorStyle,
    termStore.cursorBlink,
  ],
  () => {
    applyTermOptions()
  },
)

watch(
  [
    () => termStore.darkTheme,
    () => termStore.lightTheme,
    () => termStore.windowOpacity,
    () => termStore.wallpaperUrl,
  ],
  () => {
    applyTheme()
  },
  { deep: true },
)

watch(
  () => termStore.webglEnabled,
  (enabled) => {
    if (enabled) loadWebgl()
    else disposeWebgl()
  },
)

watch(
  () => termStore.webLinksEnabled,
  (enabled) => {
    if (enabled) loadWebLinks()
    else disposeWebLinks()
  },
)

watch(
  () => termStore.unicode11Enabled,
  (enabled) => {
    if (enabled) loadUnicode11()
    else disposeUnicode11()
  },
)

watch(
  () => termStore.ligaturesEnabled,
  (enabled) => {
    if (enabled) loadLigatures()
    else disposeLigatures()
  },
)

watch(
  () => termStore.progressEnabled,
  (enabled) => {
    if (enabled) loadProgress()
    else disposeProgress()
  },
)

onBeforeUnmount(() => {
  disposed = true
  clearHeartbeat()
  if (connectingOverlayTimer !== null) {
    window.clearTimeout(connectingOverlayTimer)
    connectingOverlayTimer = null
  }
  // 撤销 broadcast 注册，让广播配置面板里残留的 key 失效（store.purgeKey 也会顺带由 App.vue 调）
  broadcastStore.unregisterSender(props.tab.key)
  window.removeEventListener("resize", onWindowResize)
  if (fitRaf !== null) {
    window.cancelAnimationFrame(fitRaf)
    fitRaf = null
  }
  if (containerRef.value) {
    containerRef.value.removeEventListener("contextmenu", onContextMenu)
    containerRef.value.removeEventListener("auxclick", onAuxClick)
    containerRef.value.removeEventListener("mouseup", onMouseUp)
    containerRef.value.removeEventListener("mousedown", onSuggestMousedown)
    containerRef.value.removeEventListener("wheel", onWheel)
    containerRef.value.removeEventListener("keydown", onContainerKeydown, true)
    containerRef.value.removeEventListener("compositionstart", onCompositionStart, true)
  }
  if (resizeObserver) {
    try {
      resizeObserver.disconnect()
    } catch {
      // ignore
    }
    resizeObserver = null
  }
  if (themeObserver) {
    try {
      themeObserver.disconnect()
    } catch {
      // ignore
    }
    themeObserver = null
  }
  teardownWs()
  if (term) {
    // term.dispose 会释放 fit/search/webgl/webLinks/unicode11 等已加载的 addon。
    // 这里把模块级 ref 主动置 null，避免重新挂载时复用到陈旧引用。
    try {
      term.dispose()
    } catch {
      // ignore
    }
    term = null
  }
  fitAddon = null
  webglAddon = null
  webLinksAddon = null
  unicode11Addon = null
  ligaturesAddon = null
  serializeAddon = null
})
</script>

<template>
  <div class="terminal-wrap">
    <!--
      OSC 9;4 进度条覆盖在终端区域顶部 3px 高位置。
      - state=0 None：整条隐藏
      - state=1 Normal：accent 色填充到 value%
      - state=2 Error：红色填充
      - state=4 Paused/Warning：琥珀色填充
      - state=3 Indeterminate：渐变条左右循环（CSS keyframes）
      数据源：优先 ProgressAddon 解析的 OSC 9;4；无 OSC 时回退到文本进度解析
      （tqdm / pip / cargo / git / pnpm / apt / dnf / yum / apk / docker / curl 等），每个 tab 独立。
    -->
    <div
      v-if="progressState !== 0"
      class="terminal-progress"
      :class="[
        `state-${progressState}`,
        progressState === 3 ? 'is-indeterminate' : '',
      ]"
      :aria-valuenow="progressState === 3 ? undefined : progressValue"
      aria-valuemin="0"
      aria-valuemax="100"
      role="progressbar"
    >
      <div
        class="terminal-progress-fill"
        :style="progressState === 3 ? undefined : { width: `${progressValue}%` }"
      />
    </div>
    <div ref="containerRef" class="terminal-host"></div>
    <Transition name="search-fade">
      <div v-if="searchOpen" class="search-bar" @keydown.stop>
        <NInput
          ref="searchInputRef"
          v-model:value="searchKeyword"
          size="small"
          :placeholder="t('terminal.search.placeholder')"
          clearable
          class="search-input"
          @keydown="onSearchKeydown"
          @update:value="onSearchInput"
        />
        <span class="search-counter">{{ searchResultText }}</span>
        <NTooltip v-for="opt in searchToggles" :key="opt.key" trigger="hover">
          <template #trigger>
            <button
              type="button"
              class="search-toggle"
              :class="{ active: opt.active }"
              @click="toggleSearchFlag(opt.key)"
            >
              {{ opt.label }}
            </button>
          </template>
          {{ opt.tooltip }}
        </NTooltip>
        <NTooltip trigger="hover">
          <template #trigger>
            <button type="button" class="search-icon-btn" @click="runSearch('previous')">
              <NIcon :size="14"><ArrowUpIcon /></NIcon>
            </button>
          </template>
          {{ t('terminal.search.previous') }}
        </NTooltip>
        <NTooltip trigger="hover">
          <template #trigger>
            <button type="button" class="search-icon-btn" @click="runSearch('next')">
              <NIcon :size="14"><ArrowDownIcon /></NIcon>
            </button>
          </template>
          {{ t('terminal.search.next') }}
        </NTooltip>
        <NTooltip trigger="hover">
          <template #trigger>
            <button type="button" class="search-icon-btn" @click="closeSearchBar">
              <NIcon :size="14"><CloseIcon /></NIcon>
            </button>
          </template>
          {{ t('terminal.search.close') }}
        </NTooltip>
      </div>
    </Transition>

    <Transition name="ai-btn-fade">
      <NTooltip v-if="aiButtonVisible && !aiPromptVisible" placement="top">
        <template #trigger>
          <button
            type="button"
            class="ai-send-btn"
            :style="{ left: `${aiButtonX}px`, top: `${aiButtonY}px` }"
            @mousedown.prevent
            @click="openAiPrompt"
          >
            <NIcon :size="14">
              <SparklesIcon />
            </NIcon>
          </button>
        </template>
        {{ t('terminal.sendToAi') }}
      </NTooltip>
    </Transition>

    <Transition name="ai-btn-fade">
      <div
        v-if="aiPromptVisible"
        class="ai-prompt-popover"
        :style="{ left: `${aiButtonX}px`, top: `${aiButtonY}px` }"
        @keydown.stop
      >
        <NInput
          v-model:value="aiPromptText"
          size="small"
          :placeholder="t('terminal.aiPromptPlaceholder')"
          clearable
          class="ai-prompt-input"
          @keydown="onAiPromptKeydown"
        />
        <NTooltip placement="top">
          <template #trigger>
            <NButton
              size="small"
              quaternary
              class="ai-prompt-send"
              @click="submitAiPrompt"
            >
              <NIcon :size="14">
                <SendIcon />
              </NIcon>
            </NButton>
          </template>
          {{ t('terminal.sendToAi') }}
        </NTooltip>
        <NTooltip placement="top">
          <template #trigger>
            <NButton
              size="small"
              quaternary
              class="ai-prompt-close"
              @click="cancelAiPrompt"
            >
              <NIcon :size="14">
                <CloseIcon />
              </NIcon>
            </NButton>
          </template>
          {{ t('terminal.search.close') }}
        </NTooltip>
      </div>
    </Transition>

    <Transition name="suggest-fade">
      <div
        v-if="suggestVisible"
        class="cmd-suggest"
        :style="{ left: `${suggestX}px`, top: `${suggestY}px` }"
        @mousedown.prevent
      >
        <div
          v-for="(item, idx) in suggestVisibleItems"
          :key="item.cmd"
          class="cmd-suggest-item"
          :class="{ active: idx === suggestVisibleSelectedIndex }"
          @mousedown.prevent="acceptSuggestAt(idx)"
          @mouseenter="hoverSuggestAt(idx)"
        >
          <NIcon :size="12" class="cmd-suggest-icon" :class="item.source">
            <HistoryIcon v-if="item.source === 'history'" />
            <DictIcon v-else />
          </NIcon>
          <span class="cmd-suggest-cmd" :style="{ fontFamily: termStore.fontFamily }">{{ item.cmd }}</span>
          <span class="cmd-suggest-desc" :class="{ muted: !item.desc }">{{ item.desc || (item.source === 'learned' ? t('terminal.commandSuggest.learned') : '') }}</span>
          <button
            v-if="item.deletable"
            type="button"
            class="cmd-suggest-block"
            :title="t('terminal.commandSuggest.block')"
            @mousedown.prevent.stop="removeSuggestion(item)"
          >×</button>
        </div>
        <div v-if="suggestTotalMatches > 5" class="cmd-suggest-footer">
          {{ suggestSelectedIndex + 1 }}/{{ suggestTotalMatches }}
        </div>
      </div>
    </Transition>

    <Transition name="sudo-fade">
      <div v-if="sudoArmed" class="sudo-hint">
        {{ t('terminal.sudoHint') }}
      </div>
    </Transition>

    <Transition name="reconnect-fade">
      <button
        v-if="showReconnectBtn"
        type="button"
        class="reconnect-btn"
        @click="reconnect"
      >
        <NIcon :size="14">
          <RefreshOutline />
        </NIcon>
        {{ t('terminal.tabBar.reconnect') }}
      </button>
    </Transition>

    <Transition name="connecting-fade">
      <div v-if="showConnectingOverlay" class="connecting-overlay">
        <div class="connecting-spinner">
          <div class="connecting-spinner-ring"></div>
          <div class="connecting-spinner-dot"></div>
        </div>
        <div class="connecting-info">
          <span class="connecting-label">{{ connectingLabel }}</span>
          <span v-if="connectingTarget" class="connecting-target">{{ connectingTarget }}</span>
        </div>
        <div class="connecting-dots">
          <span></span>
          <span></span>
          <span></span>
        </div>
        <button type="button" class="connecting-cancel" @click="cancelConnect">
          {{ t('terminal.connectingCancel') }}
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.terminal-wrap {
  position: relative;
  height: 100%;
  width: 100%;
  display: flex;
  flex-direction: column;
  background: transparent;
  box-sizing: border-box;
  overflow: hidden;
}

.terminal-host {
  flex: 1;
  width: 100%;
  min-height: 0;
  overflow: hidden;
  padding-left: 8px;
  box-sizing: border-box;
  contain: layout paint;
}

.terminal-host :deep(.xterm) {
  height: 100%;
  width: 100%;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

.terminal-host :deep(.xterm canvas) {
  image-rendering: auto;
  will-change: transform;
}

.terminal-host :deep(.xterm-viewport) {
  background: transparent !important;
}

/* TUI 应用（.enable-mouse-events 状态）下隐藏 xterm 自定义 overlay 滚动条。
   滚动条是 position:absolute overlay，显隐仅通过 opacity 控制（100ms 出 / 800ms 隐 transition），
   不直接影响终端布局，但其 transition 动画和鼠标事件拦截会加剧 TUI 全屏重绘时的视觉抖动。
   opacity:0 中和 transition，pointer-events:none 防止滚动条拦截本应发给 TUI 的鼠标事件。 */
.terminal-host :deep(.xterm.enable-mouse-events .xterm-scrollable-element > .scrollbar) {
  opacity: 0 !important;
  pointer-events: none !important;
}

/* TUI 模式下隐藏 .xterm-viewport 的原生滚动条。
   .xterm-viewport 有 overflow-y:scroll，会显示原生滚动条；TUI 频繁触发 scroll 时
   原生滚动条的 thumb 上下跳动是视觉抖动的直接来源之一。 */
.terminal-host :deep(.xterm.enable-mouse-events .xterm-viewport) {
  overflow: hidden !important;
}

/* 隐藏 xterm 的内联 composition 浮层（.composition-view）。
   它在输入法 composition 期间显示在光标处，但 white-space:nowrap + 绝对定位 + 不设 width，
   文本会向右无限溢出；某些 TUI（光标偏右 / 长 composition 串）下会把整个布局撑开，
   composition 结束后浮层变回 display:none、宽度消失，UI 又向左缩回，造成左右抖动。
   现代 OS 输入法（Windows 微软拼音/搜狗、macOS 简拼）自带候选窗已显示 composition 文本，
   这层浮层是冗余（xterm 自带 CSS 里都留了 "TODO: Composition position got messed up somewhere"）。
   隐藏后输入仍经 helper textarea + compositionend 正常提交，不受影响。 */
.terminal-host :deep(.composition-view) {
  display: none !important;
}

.search-bar {
  position: absolute;
  top: 10px;
  right: 18px;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-border, #3a3f4b);
  border-radius: 6px;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.25);
}

.search-input {
  width: 220px;
}

.search-counter {
  min-width: 56px;
  font-size: 12px;
  color: var(--ashell-text-secondary, #98a2b3);
  text-align: center;
  user-select: none;
}

.search-toggle,
.search-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 4px;
  color: var(--ashell-text-secondary, #98a2b3);
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease, border-color 120ms ease;
}

.search-toggle:hover,
.search-icon-btn:hover {
  background: var(--ashell-bg-hover, rgba(128, 181, 255, 0.12));
  color: var(--ashell-text-strong, #e6e6e6);
}

.search-toggle.active {
  background: var(--ashell-accent-soft, rgba(128, 181, 255, 0.18));
  border-color: var(--ashell-accent, #80b5ff);
  color: var(--ashell-accent, #80b5ff);
}

.search-fade-enter-active,
.search-fade-leave-active {
  transition: opacity 120ms ease, transform 120ms ease;
}
.search-fade-enter-from,
.search-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

/* ===== OSC 9;4 顶部进度条 ===== */
.terminal-progress {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 3px;
  /* 跨在 padding 区上方，紧贴 .terminal-wrap 顶 */
  z-index: 4;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.05);
  pointer-events: none;
}

.terminal-progress-fill {
  height: 100%;
  width: 0;
  /* 进度数值的过渡：让 25 → 50 这种步进看起来平滑，不抖 */
  transition: width 180ms ease-out, background-color 180ms ease;
  background: var(--ashell-accent, #80b5ff);
}

.terminal-progress.state-1 .terminal-progress-fill {
  background: var(--ashell-accent, #80b5ff);
}
.terminal-progress.state-2 .terminal-progress-fill {
  background: #ef4444;
}
.terminal-progress.state-4 .terminal-progress-fill {
  background: #f59e0b;
}

/* Indeterminate：固定宽度 + 横向往返动画（参照 Material indeterminate） */
.terminal-progress.is-indeterminate .terminal-progress-fill {
  width: 40%;
  background: linear-gradient(
    90deg,
    transparent,
    var(--ashell-accent, #80b5ff),
    transparent
  );
  /* 关掉宽度过渡，避免和 keyframes 打架 */
  transition: none;
  animation: terminal-progress-indet 1.4s ease-in-out infinite;
}

@keyframes terminal-progress-indet {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(250%);
  }
}

/* ===== 选中文字悬浮"发送给 AI"按钮 ===== */
.ai-send-btn {
  position: absolute;
  z-index: 6;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-border, #3a3f4b);
  border-radius: 6px;
  color: var(--ashell-accent, #80b5ff);
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
}

.ai-send-btn:hover {
  background: var(--ashell-accent-soft, rgba(128, 181, 255, 0.18));
  border-color: var(--ashell-accent, #80b5ff);
}

.ai-btn-fade-enter-active,
.ai-btn-fade-leave-active {
  transition: opacity 120ms ease, transform 120ms ease;
}
.ai-btn-fade-enter-from,
.ai-btn-fade-leave-to {
  opacity: 0;
  transform: scale(0.8);
}

.ai-prompt-popover {
  position: absolute;
  z-index: 6;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-border, #3a3f4b);
  border-radius: 6px;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.25);
}

.ai-prompt-input {
  width: 260px;
}

.ai-prompt-send,
.ai-prompt-close {
  flex-shrink: 0;
}

/* ===== 离线恢复"重新连接"按钮 ===== */
.reconnect-btn {
  position: absolute;
  top: 12px;
  right: 12px;
  z-index: 6;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-border, #3a3f4b);
  border-radius: 6px;
  color: var(--ashell-accent, #80b5ff);
  font-size: 13px;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  transition: background 120ms ease, border-color 120ms ease;
}

.reconnect-btn:hover {
  background: var(--ashell-accent-soft, rgba(128, 181, 255, 0.18));
  border-color: var(--ashell-accent, #80b5ff);
}

.reconnect-fade-enter-active,
.reconnect-fade-leave-active {
  transition: opacity 200ms ease, transform 200ms ease;
}
.reconnect-fade-enter-from,
.reconnect-fade-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

/* ===== sudo 密码自动填充提示条 ===== */
.sudo-hint {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 6;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-accent, #80b5ff);
  border-radius: 6px;
  color: var(--ashell-accent, #80b5ff);
  font-size: 13px;
  white-space: nowrap;
  pointer-events: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
}

.sudo-fade-enter-active,
.sudo-fade-leave-active {
  transition: opacity 200ms ease, transform 200ms ease;
}
.sudo-fade-enter-from,
.sudo-fade-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}

/* ===== 命令建议浮层 ===== */
.cmd-suggest {
  position: absolute;
  z-index: 7;
  min-width: 200px;
  max-width: 360px;
  background: var(--ashell-bg-elevated, #2c2f36);
  border: 1px solid var(--ashell-border, #3a3f4b);
  border-radius: 6px;
  box-shadow: 0 4px 14px var(--ashell-shadow, rgba(0, 0, 0, 0.35));
  overflow: hidden;
  padding: 4px 0;
  font-size: 13px;
  user-select: none;
}

.cmd-suggest-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  cursor: pointer;
  transition: background 80ms ease;
}

.cmd-suggest-icon {
  flex-shrink: 0;
  opacity: 0.55;
  transition: opacity 80ms ease, color 80ms ease;
}

.cmd-suggest-icon.history {
  color: var(--ashell-accent, #80b5ff);
}

.cmd-suggest-icon.dict {
  color: var(--ashell-text-muted, rgba(255, 255, 255, 0.55));
}

.cmd-suggest-icon.learned {
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
}

.cmd-suggest-item.active .cmd-suggest-icon {
  opacity: 1;
}

.cmd-suggest-item:hover {
  background: var(--ashell-hover, rgba(255, 255, 255, 0.06));
}

.cmd-suggest-item.active {
  background: var(--ashell-accent-soft, rgba(128, 181, 255, 0.18));
}

.cmd-suggest-cmd {
  color: var(--ashell-text-strong, #e6e6e6);
  white-space: nowrap;
  flex-shrink: 0;
  min-width: 60px;
  font-size: 13px;
}

.cmd-suggest-item.active .cmd-suggest-cmd {
  color: var(--ashell-accent, #80b5ff);
}

.cmd-suggest-desc {
  color: var(--ashell-text-muted, rgba(255, 255, 255, 0.55));
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  min-width: 0;
}

.cmd-suggest-desc.muted {
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
  font-style: italic;
}

.cmd-suggest-block {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  margin-left: 4px;
  background: var(--ashell-hover, rgba(255, 255, 255, 0.06));
  border: none;
  border-radius: 4px;
  color: var(--ashell-text-muted, rgba(255, 255, 255, 0.55));
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
  opacity: 0.85;
  transition: opacity 80ms ease, background 80ms ease, color 80ms ease;
}

.cmd-suggest-item:hover .cmd-suggest-block,
.cmd-suggest-item.active .cmd-suggest-block {
  opacity: 1;
}

.cmd-suggest-block:hover {
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
}

.cmd-suggest-footer {
  display: flex;
  justify-content: flex-end;
  padding: 2px 12px 0;
  margin-top: 2px;
  font-size: 11px;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
  border-top: 1px solid var(--ashell-border-soft, rgba(255, 255, 255, 0.06));
}

.suggest-fade-enter-active,
.suggest-fade-leave-active {
  transition: opacity 100ms ease, transform 100ms ease;
}
.suggest-fade-enter-from,
.suggest-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

/* ===== 连接中浮层 ===== */
.connecting-overlay {
  position: absolute;
  inset: 0;
  z-index: 8;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 20px;
  background: rgba(0, 0, 0, 0.38);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

.connecting-spinner {
  position: relative;
  width: 56px;
  height: 56px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.connecting-spinner-ring {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 2px solid transparent;
  border-top-color: var(--ashell-accent, #80b5ff);
  border-right-color: var(--ashell-accent, #80b5ff);
  animation: connecting-spin 0.9s linear infinite;
}

.connecting-spinner-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--ashell-accent, #80b5ff);
  opacity: 0.35;
  animation: connecting-pulse 1.8s ease-in-out infinite;
}

@keyframes connecting-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes connecting-pulse {
  0%, 100% {
    opacity: 0.15;
    transform: scale(0.75);
  }
  50% {
    opacity: 0.5;
    transform: scale(1.15);
  }
}

.connecting-info {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 5px;
}

.connecting-label {
  font-size: 13px;
  color: var(--ashell-text-muted, rgba(255, 255, 255, 0.55));
  letter-spacing: 0.4px;
}

.connecting-target {
  font-size: 14px;
  font-family: "SF Mono", "Cascadia Code", "Fira Code", "JetBrains Mono",
    "Menlo", monospace;
  color: var(--ashell-text-strong, #e6e6e6);
  font-weight: 500;
}

.connecting-dots {
  display: flex;
  gap: 6px;
  height: 8px;
  align-items: center;
}

.connecting-dots span {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--ashell-accent, #80b5ff);
  animation: connecting-wave 1.4s ease-in-out infinite;
}

.connecting-dots span:nth-child(2) {
  animation-delay: 0.2s;
}

.connecting-dots span:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes connecting-wave {
  0%, 60%, 100% {
    opacity: 0.3;
    transform: translateY(0);
  }
  30% {
    opacity: 1;
    transform: translateY(-4px);
  }
}

.connecting-cancel {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 5px 16px;
  background: transparent;
  border: 1px solid var(--ashell-border, rgba(255, 255, 255, 0.12));
  border-radius: 6px;
  color: var(--ashell-text-muted, rgba(255, 255, 255, 0.55));
  font-size: 12px;
  cursor: pointer;
  transition: background 120ms ease, border-color 120ms ease,
    color 120ms ease;
}

.connecting-cancel:hover {
  border-color: var(--ashell-accent, #80b5ff);
  color: var(--ashell-accent, #80b5ff);
  background: var(--ashell-accent-soft, rgba(128, 181, 255, 0.1));
}

.connecting-fade-enter-active,
.connecting-fade-leave-active {
  transition: opacity 250ms ease;
}
.connecting-fade-enter-from,
.connecting-fade-leave-to {
  opacity: 0;
}
</style>
