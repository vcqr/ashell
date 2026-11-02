import { watch } from "vue"
import type { Terminal } from "@xterm/xterm"
import { useTerminalStore } from "@/stores/terminal"

/**
 * 终端左侧行时间戳栏（gutter）。
 *
 * 在终端区域最左侧按行显示该行内容写入缓冲区的完整时间：每行指令回显或每行
 * 输出各自带一个时间戳，随缓冲区滚动/换行/重排保持对齐。时间戳渲染在独立的
 * gutter 层，不写入终端缓冲区本身，因此不影响复制文本、命令解析与 ANSI 定位。
 *
 * 实现要点：
 * - xterm 没有公开的"每行写入时间"元数据，这里通过 time[] 数组与 normal
 *   buffer 的 CircularList 对齐来维护：数组末元素对应缓冲区最后一行。
 * - 订阅 CircularList 的 onTrim/onDelete/onInsert 内部事件精确跟踪行的增删
 *   （scrollback 满后的顶部裁剪、CSI L/M 的插删行）；每次 write 解析完成后用
 *   "缓冲区行数 - 数组长度" 差值兜底补齐剩余的新增行。
 * - resize 触发 reflow 后按比例重映射（近似）；仅行数变化（无重排）时在数组
 *   尾部补空/裁剪。
 * - alternate screen（vim/less 等 TUI）下隐藏 gutter 并暂停记录，normal
 *   buffer 在此期间不变，切回后无需补算。
 *
 * 通过 term._core 访问 xterm 内部 bufferService / renderService（仓库内已有
 * 先例：TerminalView.installAltScreenScrollFix）。xterm 升级后若内部结构变化，
 * 相关访问都会被 try/catch 兜住并退化为启发式（按换行计数估计裁剪行数）。
 */

/** 格式串记号。注意大小写：MM 月 / mm 分。 */
const TOKEN_RE = /YYYY|SSS|MM|DD|HH|mm|ss/g

/** 把毫秒时间按格式串格式化。未知记号原样保留。 */
export function formatLineTime(ms: number, fmt: string): string {
  const d = new Date(ms)
  const pad2 = (n: number) => (n < 10 ? `0${n}` : String(n))
  return fmt.replace(TOKEN_RE, (tok) => {
    switch (tok) {
      case "YYYY":
        return String(d.getFullYear())
      case "MM":
        return pad2(d.getMonth() + 1)
      case "DD":
        return pad2(d.getDate())
      case "HH":
        return pad2(d.getHours())
      case "mm":
        return pad2(d.getMinutes())
      case "ss":
        return pad2(d.getSeconds())
      case "SSS":
        return String(d.getMilliseconds()).padStart(3, "0")
      default:
        return tok
    }
  })
}

/** xterm 内部 buffer.lines（CircularList）用到的最小结构。 */
interface CircularListLike {
  length: number
  onTrimEmitter?: { event: (cb: (n: number) => void) => { dispose(): void } }
  onDeleteEmitter?: { event: (cb: (e: { index: number; amount: number }) => void) => { dispose(): void } }
  onInsertEmitter?: { event: (cb: (e: { index: number; amount: number }) => void) => { dispose(): void } }
}

interface TerminalTimestampsOptions {
  getTerm: () => Terminal | null
  /** .terminal-host：gutter 挂载点 */
  hostEl: () => HTMLElement | null
  /** 包裹 .xterm 的 fitbox：通过 margin-left 给 gutter 让位 */
  fitboxEl: () => HTMLElement | null
  /** gutter 宽度变化后请求重新 fit 终端 */
  requestFit: () => void
}

/** gutter 左/右内边距（右留 10px 间隔让时间与终端内容分开） */
const GUTTER_PAD_LEFT = 6
const GUTTER_PAD_RIGHT = 10

export function useTerminalTimestamps(options: TerminalTimestampsOptions) {
  const store = useTerminalStore()

  // ===== 行时间记录状态 =====
  /** 与 normal buffer 底部对齐的行时间数组：末元素 = 缓冲区最后一行 */
  let times: (number | undefined)[] = []
  /** trim 事件累计计数（区分 resize 期间的基线） */
  let cumTrim = 0
  /** 上次同步时的 cumTrim 基线 */
  let trimBase = 0
  /** 当前订阅的 CircularList 实例（clear/reset 后 buffer.lines 会被替换，需重订阅） */
  let subbedLines: CircularListLike | null = null
  /** 内部 trim 事件是否可用；不可用时退化为按换行计数估计 */
  let trimSupported = false
  type Unsub = { dispose(): void }
  let unsubTrim: Unsub | null = null
  let unsubDelete: Unsub | null = null
  let unsubInsert: Unsub | null = null

  // ===== 渲染状态 =====
  let gutterEl: HTMLDivElement | null = null
  let rowEls: HTMLDivElement[] = []
  let lastCellH = -1
  let lastGutterW = -1
  let paintRaf: number | null = null
  let lastCols = -1

  // ===== write 包装 =====
  let writePatched = false
  let origWrite: ((data: string | Uint8Array, cb?: () => void) => void) | null = null

  const watches: ReturnType<typeof watch>[] = []
  const termDisposers: { dispose(): void }[] = []

  // ===== xterm 内部结构访问（全部 try/catch 兜底）=====

  function normalLines(): CircularListLike | null {
    const t = options.getTerm()
    if (!t) return null
    try {
      const core = (t as unknown as { _core?: { bufferService?: { buffers?: { normal?: { lines?: CircularListLike } } } } })._core
      return core?.bufferService?.buffers?.normal?.lines ?? null
    } catch {
      return null
    }
  }

  function readCellHeight(t: Terminal): number {
    try {
      const h = (t as unknown as {
        _core?: { _renderService?: { dimensions?: { css?: { cell?: { height?: number } } } } }
      })._core?._renderService?.dimensions?.css?.cell?.height
      if (typeof h === "number" && h > 0) return h
    } catch {
      // ignore
    }
    const host = options.hostEl()
    if (host && t.rows > 0) return host.clientHeight / t.rows
    return 16
  }

  // ===== 缓冲区行增删跟踪 =====

  function onTrimEvent(n: number) {
    cumTrim += n
  }

  /** CSI M 删行：CircularList 在长度更新后触发，index 为自顶起算的删除位置 */
  function onDeleteEvent(e: { index: number; amount: number }) {
    if (!store.lineTimestampEnabled) return
    const start = Math.max(0, Math.min(e.index, times.length))
    times.splice(start, Math.min(e.amount, times.length - start))
  }

  /** CSI L 插行：新行按当前时间记录 */
  function onInsertEvent(e: { index: number; amount: number }) {
    if (!store.lineTimestampEnabled) return
    const now = Date.now()
    const inserted = new Array<number | undefined>(e.amount).fill(now)
    const start = Math.max(0, Math.min(e.index, times.length))
    times.splice(start, 0, ...inserted)
  }

  function unsubscribeAll() {
    for (const u of [unsubTrim, unsubDelete, unsubInsert]) {
      try {
        u?.dispose()
      } catch {
        // ignore
      }
    }
    unsubTrim = unsubDelete = unsubInsert = null
    subbedLines = null
    trimSupported = false
  }

  function resubscribe(lines: CircularListLike) {
    unsubscribeAll()
    try {
      unsubTrim = lines.onTrimEmitter?.event(onTrimEvent) ?? null
      unsubDelete = lines.onDeleteEmitter?.event(onDeleteEvent) ?? null
      unsubInsert = lines.onInsertEmitter?.event(onInsertEvent) ?? null
      trimSupported = !!unsubTrim
      subbedLines = lines
      trimBase = cumTrim
    } catch {
      // 内部结构变化：保持 unsubscribed 状态，走启发式
    }
  }

  /** 统计一段待写入数据里的换行数（仅内部 trim 事件不可用时的启发式需要） */
  function countNewlines(data: string | Uint8Array): number {
    let n = 0
    if (typeof data === "string") {
      let i = data.indexOf("\n")
      while (i !== -1) {
        n++
        i = data.indexOf("\n", i + 1)
      }
    } else {
      for (let i = 0; i < data.length; i++) {
        if (data[i] === 10) n++
      }
    }
    return n
  }

  /**
   * 每次 write 解析完成后的同步：把缓冲区行数与 times 数组长度的差值补齐。
   * 差值为正 → 追加新行时间；为负 → 缓冲区整体重置（clear/reset）。
   */
  function onBatchParsed(data: string | Uint8Array | null) {
    const t = options.getTerm()
    if (!t || !store.lineTimestampEnabled) return
    let active: Terminal["buffer"]["active"]
    try {
      active = t.buffer.active
    } catch {
      return
    }
    if (active.type === "alternate") return
    const lines = normalLines()
    if (!lines) return
    if (lines !== subbedLines) resubscribe(lines)

    if (!trimSupported) {
      // 退化路径：scrollback 满后行数不变，无法从长度差发现裁剪。
      // 光标停在底行且本批数据含换行 → 认为发生了滚动，按换行数平移。
      try {
        const scrollback = typeof t.options.scrollback === "number" ? t.options.scrollback : 0
        const atCap = lines.length >= scrollback + t.rows
        if (atCap && data && active.cursorY >= t.rows - 1) {
          const nl = countNewlines(data)
          if (nl > 0) times.splice(0, Math.min(nl, times.length))
        }
      } catch {
        // ignore
      }
    } else {
      const trims = cumTrim - trimBase
      if (trims > 0) {
        times.splice(0, Math.min(trims, times.length))
        trimBase = cumTrim
      }
    }

    const appended = lines.length - times.length
    if (appended > 0) {
      const now = Date.now()
      for (let i = 0; i < appended; i++) times.push(now)
    } else if (appended < 0) {
      // 行数骤减且无对应 trim 事件：clear()/reset() 换了新 lines，全部行按新建记录
      times = new Array<number | undefined>(lines.length).fill(Date.now())
      trimBase = cumTrim
    }
    schedulePaint()
  }

  // ===== resize / reflow =====

  function onTermResized() {
    const t = options.getTerm()
    if (!t || !store.lineTimestampEnabled) return
    try {
      if (t.buffer.active.type === "alternate") return
    } catch {
      return
    }
    const lines = normalLines()
    if (!lines) return
    if (lines !== subbedLines) resubscribe(lines)

    // windowsPty(conpty) 下 xterm 关闭了 reflow（本地 Windows 终端），列数变化
    // 不会重排行内容，按"仅行数变化"处理；其余情况列数变化视为 reflow
    let reflowDisabled = false
    try {
      reflowDisabled = t.options.windowsPty?.backend === "conpty"
    } catch {
      // ignore
    }

    if (lastCols !== -1 && lastCols !== t.cols && !reflowDisabled) {
      // 列数变化触发 reflow，行与旧数组的对应关系按比例重映射（近似）
      const old = times
      const next = new Array<number | undefined>(lines.length)
      if (old.length > 0 && lines.length > 0) {
        for (let i = 0; i < lines.length; i++) {
          const src = Math.min(old.length - 1, Math.floor((i * old.length) / lines.length))
          next[i] = old[src]
        }
      }
      times = next
    } else if (lines.length !== times.length) {
      // 仅行数变化（无重排）：内容行不变，尾部补空/裁剪
      while (times.length > lines.length) times.pop()
      while (times.length < lines.length) times.push(undefined)
    }
    lastCols = t.cols
    trimBase = cumTrim
    schedulePaint()
  }

  // ===== gutter 渲染 =====

  function setFitboxMargin(w: number) {
    const fitbox = options.fitboxEl()
    if (!fitbox) return
    fitbox.style.marginLeft = w > 0 ? `${w}px` : ""
  }

  /** gutter 宽度测量用的离屏 canvas（缓存，避免每次测量重建） */
  let measureCtx: CanvasRenderingContext2D | null | undefined

  /**
   * gutter 允许的最大宽度 = 终端区域的 40%。格式串最长 64 字符，不限宽的话
   * 自定义长格式会把终端内容挤掉半屏。容器不可测（隐藏 tab）时退化为按上次
   * fit 的列宽估算，两者都拿不到则不限制。
   */
  function maxGutterWidth(): number {
    const host = options.hostEl()
    if (host && host.clientWidth > 0) {
      return Math.max(36, Math.floor(host.clientWidth * 0.4))
    }
    const t = options.getTerm()
    try {
      const cellW = (t as unknown as {
        _core?: { _renderService?: { dimensions?: { css?: { cell?: { width?: number } } } } }
      })._core?._renderService?.dimensions?.css?.cell?.width
      if (t && typeof cellW === "number" && cellW > 0 && t.cols > 0) {
        return Math.max(36, Math.floor(t.cols * cellW * 0.4))
      }
    } catch {
      // ignore
    }
    return Number.MAX_SAFE_INTEGER
  }

  function measureWidth(): number {
    const fmt = store.lineTimestampFormat
    const sample = fmt.replace(TOKEN_RE, (tok: string) =>
      tok === "YYYY" ? "8888" : tok === "SSS" ? "888" : "88",
    )
    let textW = sample.length * 8
    try {
      if (measureCtx === undefined) {
        measureCtx = document.createElement("canvas").getContext("2d")
      }
      if (measureCtx) {
        measureCtx.font = `${store.fontSize}px ${store.fontFamily}`
        textW = measureCtx.measureText(sample).width
      }
    } catch {
      // ignore
    }
    const natural = Math.max(36, Math.ceil(textW) + GUTTER_PAD_LEFT + GUTTER_PAD_RIGHT)
    return Math.min(natural, maxGutterWidth())
  }

  function applyGutterStyle() {
    if (!gutterEl) return
    gutterEl.style.color = store.lineTimestampColor
    gutterEl.style.fontFamily = store.fontFamily
    gutterEl.style.fontSize = `${store.fontSize}px`
  }

  function ensureGutter(): boolean {
    if (gutterEl) return true
    const host = options.hostEl()
    if (!host) return false
    const el = document.createElement("div")
    el.className = "ashell-ts-gutter"
    el.style.position = "absolute"
    el.style.top = "0"
    el.style.bottom = "0"
    el.style.left = "0"
    el.style.overflow = "hidden"
    el.style.pointerEvents = "none"
    el.style.userSelect = "none"
    el.style.whiteSpace = "pre"
    el.style.textAlign = "right"
    el.style.zIndex = "1"
    host.appendChild(el)
    gutterEl = el
    applyGutterStyle()
    return true
  }

  function removeGutter() {
    try {
      gutterEl?.remove()
    } catch {
      // ignore
    }
    gutterEl = null
    rowEls = []
    lastCellH = -1
    lastGutterW = -1
  }

  /** 可见行数或行高变化时重建行元素池 */
  function rebuildPool(rows: number, cellH: number) {
    if (!gutterEl) return
    if (rowEls.length === rows && lastCellH === cellH) return
    for (const el of rowEls) el.remove()
    rowEls = []
    for (let r = 0; r < rows; r++) {
      const el = document.createElement("div")
      el.style.position = "absolute"
      el.style.left = `${GUTTER_PAD_LEFT}px`
      el.style.right = `${GUTTER_PAD_RIGHT}px`
      el.style.top = `${r * cellH}px`
      el.style.height = `${cellH}px`
      el.style.lineHeight = `${cellH}px`
      el.style.overflow = "hidden"
      el.style.whiteSpace = "pre"
      el.style.textAlign = "right"
      gutterEl.appendChild(el)
      rowEls.push(el)
    }
    lastCellH = cellH
  }

  function paint() {
    const t = options.getTerm()
    if (!t || !gutterEl || !store.lineTimestampEnabled) return
    let active: Terminal["buffer"]["active"]
    try {
      active = t.buffer.active
    } catch {
      return
    }
    if (active.type === "alternate") {
      gutterEl.style.display = "none"
      return
    }
    gutterEl.style.display = ""
    const cellH = readCellHeight(t)
    rebuildPool(t.rows, cellH)
    const fmt = store.lineTimestampFormat
    // times 覆盖不足时（启用前的历史行）顶部偏移
    const off = active.length - times.length
    const top = active.viewportY
    for (let r = 0; r < rowEls.length; r++) {
      const el = rowEls[r]!
      const idx = top + r - off
      const v = idx >= 0 && idx < times.length ? times[idx] : undefined
      const txt = v === undefined ? "" : formatLineTime(v, fmt)
      if (el.textContent !== txt) el.textContent = txt
    }
  }

  function schedulePaint() {
    if (paintRaf !== null) return
    paintRaf = requestAnimationFrame(() => {
      paintRaf = null
      paint()
    })
  }

  // ===== 开关 =====

  function applyEnabled(on: boolean) {
    if (on) {
      const lines = normalLines()
      if (lines) resubscribe(lines)
      times = []
      if (!ensureGutter()) return
      lastGutterW = -1
      updateWidth()
      schedulePaint()
      options.requestFit()
    } else {
      unsubscribeAll()
      times = []
      removeGutter()
      setFitboxMargin(0)
      options.requestFit()
    }
  }

  /** 按当前格式/字号测量 gutter 宽度并同步 fitbox 让位；宽度变化时请求重新 fit */
  function updateWidth() {
    if (!store.lineTimestampEnabled || !gutterEl) return
    const w = measureWidth()
    if (w === lastGutterW) return
    lastGutterW = w
    gutterEl.style.width = `${w}px`
    setFitboxMargin(w)
    options.requestFit()
  }

  /**
   * 重新测量并收敛 gutter 宽度。给容器尺寸可能已变化的时机调用（tab 激活、
   * 窗口尺寸变化）：隐藏期间 host.clientWidth 为 0，宽度上限会走估算值，
   * 回到可见状态后按实际宽度重新收紧或放宽。
   */
  function refreshWidth() {
    if (!store.lineTimestampEnabled || !gutterEl) return
    updateWidth()
    schedulePaint()
  }

  // ===== 安装 / 卸载 =====

  function patchWrite() {
    const t = options.getTerm()
    if (!t || writePatched) return
    const orig = t.write.bind(t)
    origWrite = orig
    writePatched = true
    t.write = (data: string | Uint8Array, cb?: () => void) => {
      return orig(data, () => {
        try {
          onBatchParsed(data)
        } catch {
          // ignore
        }
        cb?.()
      })
    }
  }

  function install() {
    patchWrite()
    const t = options.getTerm()
    if (t) {
      try {
        lastCols = t.cols
      } catch {
        // ignore
      }
      try {
        termDisposers.push(t.onScroll(() => schedulePaint()))
        termDisposers.push(t.onResize(() => onTermResized()))
      } catch {
        // ignore
      }
    }

    watches.push(
      watch(
        () => store.lineTimestampEnabled,
        (on) => applyEnabled(on),
        { flush: "post" },
      ),
    )
    watches.push(
      watch(
        () => [store.lineTimestampFormat, store.lineTimestampColor, store.fontSize, store.fontFamily] as const,
        () => {
          if (!store.lineTimestampEnabled || !gutterEl) return
          applyGutterStyle()
          updateWidth()
          schedulePaint()
        },
        { flush: "post" },
      ),
    )

    // 安装时已是开启状态则立即生效
    if (store.lineTimestampEnabled) applyEnabled(true)
  }

  function dispose() {
    for (const w of watches) w.stop()
    watches.length = 0
    for (const d of termDisposers) {
      try {
        d.dispose()
      } catch {
        // ignore
      }
    }
    termDisposers.length = 0
    unsubscribeAll()
    if (writePatched && origWrite) {
      const t = options.getTerm()
      if (t) {
        try {
          t.write = origWrite
        } catch {
          // ignore
        }
      }
      writePatched = false
      origWrite = null
    }
    if (paintRaf !== null) {
      cancelAnimationFrame(paintRaf)
      paintRaf = null
    }
    removeGutter()
    setFitboxMargin(0)
    times = []
  }

  return { install, dispose, refreshWidth }
}
