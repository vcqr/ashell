import { defineStore } from 'pinia'
import { ref } from 'vue'
import { buildWsUrl, request } from '@/api/client'
import { isTauri } from '@/utils/platform'
import type { AiUsageStats, ChatMessage, ProcessStep } from '@/types'

/** 单个 kind（text/thinking）的流式段状态 */
export interface StreamSeg {
  /** 该段对应的气泡消息 id */
  msgId: number
  /** 已收到但尚未"打"出来的增量文本（打字机缓冲） */
  pending: string
  /** 已逐字"打"出来的文本（显示层协议：经 liveText 上屏，段收尾时一次性
   *  提交到 messages[].content 并渲染 markdown） */
  typed: string
  /** 分段已 FINAL（全文帧已到）：动画追平后立即收尾气泡 */
  finishing: boolean
}

/** 流式显示层协议：当前打字中的段内容。独立于 messages 存放——打字机每拍的
 *  更新只触发这一个文本节点重渲染，整表 messages/分段计算不受每拍扰动 */
export interface LiveStreamText {
  ssid: string
  msgId: number
  kind: 'text' | 'thinking'
  text: string
}

/** 单个 ssid 对应的 AI 会话状态 */
export interface AiSession {
  ssid: string
  messages: ChatMessage[]
  seq: number
  isTyping: boolean
  isSessionActive: boolean
  isApprovalActive: boolean
  sidecarPid: number | null
  /** 当前正在累积的"中间过程"消息 id；为 null 表示当前无活跃过程块 */
  currentProcessMsgId: number | null
  /**
   * 按 kind 独立跟踪的流式段。pi 的事件流是交错的（thinking_end 晚于
   * text_start/text_delta），FINAL 必须路由到自己的段而非"当前活跃气泡"，
   * 否则 thinking 全文会被错推成新气泡造成内容双份。
   */
  streamSegs: { text?: StreamSeg; thinking?: StreamSeg }
  /** 本轮已结束（END/STOPPED 已到）：各段动画追平后自动收尾 */
  streamTurnOver: boolean
  /** 最近一次上报的上下文占用（[AI_USAGE] 帧更新；null 表示未知，如刚压缩完） */
  contextUsage: Pick<AiUsageStats, 'contextTokens' | 'contextWindow' | 'contextPercent'> | null
  /** stdout 监听句柄（Web 端为关闭 sidecar 流 WS 的函数） */
  unlistenStdout: UnlistenFn | null
  /** stderr 监听句柄（Web 端为关闭 sidecar 流 WS 的函数） */
  unlistenStderr: UnlistenFn | null
}

/** Web 端 sidecar 输出流行 */
interface SidecarStreamMessage {
  stream: 'stdout' | 'stderr' | 'lagged'
  line?: string
  dropped?: number
}

type UnlistenFn = () => void

function emptySession(ssid: string): AiSession {
  return {
    ssid,
    messages: [],
    seq: 0,
    isTyping: false,
    isSessionActive: false,
    isApprovalActive: false,
    sidecarPid: null,
    currentProcessMsgId: null,
    streamSegs: {},
    streamTurnOver: false,
    contextUsage: null,
    unlistenStdout: null,
    unlistenStderr: null,
  }
}

/**
 * 按 ssid 维护 AI 助手会话状态。
 *
 * 一个 SSH 终端会话（ssid）对应一个独立的 AI 会话与对话历史：daemon 化后
 * 常驻 daemon 进程内按 ssid 多路复用（sidecarPid 语义为宿主侧会话标识）；
 * 切换终端 tab 时只是切换显示哪个 ssid 的状态，不重建会话。
 * 仅在 SSH session 真正断开时（终端 status 变为 closed/error）才 kill。
 */
export const useAiStore = defineStore('ai', () => {
  /** ssid -> session */
  const sessions = ref<Record<string, AiSession>>({})
  /** 流式显示层：当前打字中的段文本（每拍更新只重渲染该文本节点） */
  const liveText = ref<LiveStreamText | null>(null)

  function ensure(ssid: string): AiSession {
    if (!sessions.value[ssid]) {
      sessions.value = { ...sessions.value, [ssid]: emptySession(ssid) }
    }
    return sessions.value[ssid]!
  }

  function get(ssid: string): AiSession | undefined {
    return sessions.value[ssid]
  }

  function patch(ssid: string, partial: Partial<AiSession>) {
    const s = sessions.value[ssid]
    if (!s) return
    sessions.value = { ...sessions.value, [ssid]: { ...s, ...partial } }
  }

  function pushMessage(ssid: string, msg: Omit<ChatMessage, 'id'>): ChatMessage {
    const s = ensure(ssid)
    const id = s.seq + 1
    const full: ChatMessage = { id, ts: Date.now(), ...msg }
    sessions.value = {
      ...sessions.value,
      [ssid]: { ...s, seq: id, messages: [...s.messages, full] },
    }
    return full
  }

  function updateMessage(ssid: string, id: number, patch: Partial<ChatMessage>) {
    const s = sessions.value[ssid]
    if (!s) return
    const idx = s.messages.findIndex((m) => m.id === id)
    if (idx < 0) return
    const next = [...s.messages]
    next[idx] = { ...s.messages[idx]!, ...patch }
    sessions.value = { ...sessions.value, [ssid]: { ...s, messages: next } }
  }

  /**
   * 追加一条"中间过程"步骤（AITOOL/TOOL_RET）。
   * 若当前无活跃过程块，先新建一条 isProcess 消息并记为 currentProcessMsgId，
   * 再把 step 追加进去。不修改 isTyping —— 过程进行中仍然算"AI 正在打字"。
   */
  function appendProcessStep(ssid: string, step: ProcessStep) {
    const s = ensure(ssid)
    let msgId = s.currentProcessMsgId
    if (msgId === null) {
      const id = s.seq + 1
      const msg: ChatMessage = {
        id,
        role: 'assistant',
        content: '',
        time: step.time,
        ts: Date.now(),
        isProcess: true,
        processSteps: [step],
      }
      sessions.value = {
        ...sessions.value,
        [ssid]: { ...s, seq: id, messages: [...s.messages, msg], currentProcessMsgId: id },
      }
      msgId = id
    } else {
      const idx = s.messages.findIndex((m) => m.id === msgId)
      if (idx < 0) {
        // id 失效，重建
        sessions.value = { ...sessions.value, [ssid]: { ...s, currentProcessMsgId: null } }
        return appendProcessStep(ssid, step)
      }
      const next = [...s.messages]
      next[idx] = {
        ...s.messages[idx]!,
        processSteps: [...(s.messages[idx]!.processSteps ?? []), step],
      }
      sessions.value = { ...sessions.value, [ssid]: { ...s, messages: next } }
    }
  }

  /** 结束当前过程块：清空 currentProcessMsgId，后续 AITOOL/TOOL_RET 会开新块 */
  function finalizeProcess(ssid: string) {
    const s = sessions.value[ssid]
    if (!s || s.currentProcessMsgId === null) return
    sessions.value = { ...sessions.value, [ssid]: { ...s, currentProcessMsgId: null } }
  }

  let streamTimer: ReturnType<typeof setInterval> | null = null;
  /**
   * 打字机动画循环：50ms 一拍，把各段 streamSegs[kind].pending 按自适应速率
   * 逐字渲染进气泡。步长 = max(3, pending/6)：上游真流式时近乎实时，大块突发
   * （如 openrouter 免费档的缓冲响应）约 0.4s 内平滑追平。
   * 循环常驻（空闲拍开销可忽略），首轮增量时惰性启动。
   */
  function ensureStreamAnimator() {
    if (streamTimer !== null) return;
    streamTimer = setInterval(() => {
      for (const s of Object.values(sessions.value)) {
        // 顺序化打字：thinking 段存在期间（含 FINAL 后的收尾排空阶段），正文一律
        // 不打——有的模型交替发送 thinking/text delta，「FINAL 才放行」仍会在
        // thinking 尾巴未打完时放正文进场，形成尾部并行。段关闭（排空 + FINAL
        // 都完成）后正文才开始；end 事件总会到达，不会饿死。
        // 无 thinking 段的轮次正文照常打字，不受影响。
        const kinds: Array<'text' | 'thinking'> = s.streamSegs.thinking
          ? ["thinking"]
          : ["thinking", "text"];
        if (!kinds.some((k) => s.streamSegs[k]?.pending)) continue;
        let cur = s;
        for (const kind of kinds) {
          const seg = cur.streamSegs[kind];
          if (!seg?.pending) continue;
          const step = Math.max(3, Math.ceil(seg.pending.length / 6));
          const shown = seg.pending.slice(0, step);
          const rest = seg.pending.slice(step);
          seg.typed = (seg.typed ?? "") + shown;
          // 显示层：只更新 liveText，该文本节点之外的 UI 不受每拍扰动
          liveText.value = { ssid: s.ssid, msgId: seg.msgId, kind, text: seg.typed };
          const segs = { ...cur.streamSegs };
          const finished = rest === "";
          // 分段 FINAL 或轮末已到：追平后收尾该段气泡——此刻才把全文一次性
          // 提交到 messages[].content（markdown 渲染只发生这一次）
          const close = finished && (seg.finishing || cur.streamTurnOver);
          if (close) {
            const idx = cur.messages.findIndex((m) => m.id === seg.msgId);
            if (idx >= 0) {
              const next = [...cur.messages];
              next[idx] = { ...cur.messages[idx]!, content: seg.typed, isStreaming: false };
              cur = { ...cur, messages: next };
            }
            delete segs[kind];
            if (liveText.value?.msgId === seg.msgId) liveText.value = null;
            // 全部段收尾完毕：轮末延迟收尾状态复位（否则残留的 turnOver 会把
            // 下一轮的段在首次追平时误关闭，造成内容错段/双份）
            if (!segs.text && !segs.thinking) cur = { ...cur, streamTurnOver: false };
          } else {
            segs[kind] = { ...seg, pending: rest };
          }
          cur = { ...cur, streamSegs: segs };
        }
        if (cur !== s) sessions.value = { ...sessions.value, [s.ssid]: cur };
      }
    }, 50);
  }

  /** 段收尾时把全文一次性提交到消息（流式期间 content 保持空，渲染走 liveText） */
  function commitSegContent(s: AiSession, seg: StreamSeg, content: string): AiSession {
    const idx = s.messages.findIndex((m) => m.id === seg.msgId)
    let out = s
    if (idx >= 0) {
      const next = [...s.messages]
      next[idx] = { ...s.messages[idx]!, content, isStreaming: false }
      out = { ...s, messages: next }
    }
    if (liveText.value?.msgId === seg.msgId) liveText.value = null
    return out
  }

  /** 关闭某个 kind 的流式段：冲入未渲染的 pending 防丢字，气泡置为已收尾 */
  function closeStreamingSeg(s: AiSession, kind: 'text' | 'thinking'): AiSession {
    const seg = s.streamSegs[kind]
    if (!seg) return s
    const segs = { ...s.streamSegs }
    delete segs[kind]
    const s2 = commitSegContent(s, seg, (seg.typed ?? "") + seg.pending)
    return { ...s2, streamSegs: segs }
  }

  /**
   * 新开一个 kind 的流式段（[AI_TEXT_START]/[AI_THINKING_START] 帧）：该 kind
   * 上一段仍在时先收尾（冲刷缓冲），创建空的 isStreaming 气泡。其他 kind 的
   * 活跃段不受影响——pi 的事件流交错，thinking 段可能跨着 text 段才结束。
   */
  function startStreamingBubble(ssid: string, kind: 'text' | 'thinking', time: string) {
    let s = ensure(ssid)
    if (s.streamSegs[kind]) {
      s = closeStreamingSeg(s, kind)
      sessions.value = { ...sessions.value, [ssid]: s }
    }
    const id = s.seq + 1
    const msg: ChatMessage = {
      id,
      role: 'assistant',
      content: '',
      time,
      ts: Date.now(),
      thinking: kind === 'thinking',
      isStreaming: true,
    }
    sessions.value = {
      ...sessions.value,
      [ssid]: {
        ...s,
        seq: id,
        messages: [...s.messages, msg],
        // 新段开始即本轮活跃：清掉上一轮可能残留的延迟收尾标记
        streamTurnOver: false,
        streamSegs: {
          ...s.streamSegs,
          [kind]: { msgId: id, pending: '', typed: '', finishing: false },
        },
      },
    }
  }

  /**
   * 流式累积增量（[AIMSG_DELTA]/[AI_THINKING_DELTA] 帧）：压入对应 kind 段的
   * 打字机缓冲（动画循环渲染）。段不存在时隐式新建（引擎正常都先发 START）。
   */
  function appendStreamingMessage(
    ssid: string,
    kind: 'text' | 'thinking',
    text: string,
    time: string,
  ) {
    if (!text) return
    let s = ensure(ssid)
    let seg = s.streamSegs[kind]
    if (seg && seg.finishing) {
      // 已 FINAL 的段又来了增量（理论不出现）：收尾旧段另起新段
      s = closeStreamingSeg(s, kind)
      sessions.value = { ...sessions.value, [ssid]: s }
      seg = undefined
    }
    if (!seg) {
      startStreamingBubble(ssid, kind, time)
      s = ensure(ssid)
      seg = s.streamSegs[kind]
    }
    sessions.value = {
      ...sessions.value,
      [ssid]: {
        ...s,
        streamSegs: { ...s.streamSegs, [kind]: { ...seg!, pending: seg!.pending + text } },
      },
    }
    ensureStreamAnimator()
  }

  /**
   * 分段 FINAL（[AIMSG]/[AI_THINKING] 全文帧，块结束时引擎立即发出）：路由到
   * 该 kind 自己的段——全文是已打内容的延续时把剩余部分接进打字机缓冲（突发
   * 上游下 FINAL 与最后 delta 同拍到达，动画继续打完）；前缀不连续（增量丢帧）
   * 时整体覆盖。缓冲已空立即收尾，否则动画追平后自动收尾。
   * 无对应段（block 模式）返回 'push'，由调用方按普通消息入列。
   */
  function finalizeStreamingSegment(
    ssid: string,
    kind: 'text' | 'thinking',
    fullText: string,
  ): 'consumed' | 'push' {
    const s = sessions.value[ssid]
    if (!s) return 'push'
    const seg = s.streamSegs[kind]
    if (!seg) return 'push'
    const idx = s.messages.findIndex((m) => m.id === seg.msgId)
    if (idx < 0) {
      sessions.value = { ...sessions.value, [ssid]: closeStreamingSeg(s, kind) }
      return 'push'
    }
    // 已"打"出来的前缀 = 段的 typed（流式期间 message.content 保持空，显示走 liveText）
    const typed = seg.typed ?? ''
    const continuous = fullText.startsWith(typed)
    if (!continuous) {
      // 前缀不连续（增量丢帧）：整体以权威全文立即提交收尾
      const out = commitSegContent(s, seg, fullText)
      const segs = { ...out.streamSegs }
      delete segs[kind]
      sessions.value = { ...sessions.value, [ssid]: { ...out, streamSegs: segs } }
      return 'consumed'
    }
    const pending = fullText.slice(typed.length)
    if (!pending) {
      // 缓冲已全部显示：立即以权威全文提交收尾
      const out = commitSegContent(s, seg, fullText)
      const segs = { ...out.streamSegs }
      delete segs[kind]
      sessions.value = { ...sessions.value, [ssid]: { ...out, streamSegs: segs } }
      return 'consumed'
    }
    // 动画未追平：FINAL 后动画继续打剩余部分，追平自动提交收尾
    const segs = { ...s.streamSegs }
    segs[kind] = { ...seg, pending, finishing: true }
    sessions.value = { ...sessions.value, [ssid]: { ...s, streamSegs: segs } }
    ensureStreamAnimator()
    return 'consumed'
  }

  /**
   * 轮末清理：仍有未打完的缓冲时默认延迟收尾（动画追平后各段自动关泡）；
   * snap=true（用户手动中断）立即定格为已收到的全文。
   */
  function endStreamingTurn(ssid: string, snap = false) {
    const s = sessions.value[ssid]
    if (!s) return
    const hasOpenSeg = !!(s.streamSegs.text || s.streamSegs.thinking)
    if (!hasOpenSeg && !s.streamTurnOver) return
    const pendingLeft = (['text', 'thinking'] as const).some((k) => s.streamSegs[k]?.pending)
    if (!snap && pendingLeft) {
      sessions.value = { ...sessions.value, [ssid]: { ...s, streamTurnOver: true } }
      return
    }
    let cur = s
    for (const k of ['text', 'thinking'] as const) {
      cur = closeStreamingSeg(cur, k)
    }
    sessions.value = { ...sessions.value, [ssid]: { ...cur, streamTurnOver: false } }
  }

  /**
   * 把本轮 token 用量附着到最后一条 assistant 消息（[AI_USAGE] 帧在轮末到达，
   * 此时该轮的 AIMSG 已推送；跳过过程块向前找，找不到就丢弃）。
   * 同时更新会话级上下文占用（输入框旁的状态位展示）。
   */
  function attachUsage(ssid: string, usage: AiUsageStats) {
    const s = ensure(ssid)
    if (usage.contextPercent != null) {
      patch(ssid, {
        contextUsage: {
          contextTokens: usage.contextTokens,
          contextWindow: usage.contextWindow,
          contextPercent: usage.contextPercent,
        },
      })
    }
    for (let i = s.messages.length - 1; i >= 0; i--) {
      const m = s.messages[i]!
      if (m.role === 'assistant' && !m.isProcess) {
        updateMessage(ssid, m.id, { usage })
        return
      }
    }
  }

  function clearMessages(ssid: string) {
    const s = sessions.value[ssid]
    if (!s) return
    if (liveText.value?.ssid === ssid) liveText.value = null
    sessions.value = {
      ...sessions.value,
      [ssid]: {
        ...s,
        messages: [],
        seq: 0,
        isApprovalActive: false,
        isTyping: false,
        currentProcessMsgId: null,
        streamSegs: {},
        streamTurnOver: false,
        contextUsage: null,
      },
    }
  }

  /**
   * 打开 Web 端 sidecar 输出流 WS，返回关闭函数。
   * stdout 行回调 onStdout；stderr/lagged 仅记日志（与桌面端 UI 行为一致）。
   */
  async function openSidecarStream(
    ssid: string,
    onStdout: (line: string) => void,
  ): Promise<UnlistenFn> {
    const url = await buildWsUrl(`/api/ai/sidecar/${encodeURIComponent(ssid)}/stream`)
    const ws = new WebSocket(url)
    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data as string) as SidecarStreamMessage
        if (msg.stream === 'stdout' && typeof msg.line === 'string') {
          onStdout(msg.line)
        } else if (msg.stream === 'lagged') {
          console.warn(`[AI store] sidecar stream lagged, dropped ${msg.dropped} lines`)
        }
      } catch {
        // 非 JSON 帧忽略
      }
    }
    return () => {
      ws.close()
    }
  }

  /**
   * 启动指定 ssid 的 sidecar；若已运行先 kill 旧进程。
   * 同时按 ssid 注册 stdout/stderr 监听器（桌面 Tauri event / Web WS 广播）。
   */
  async function spawnFor(
    ssid: string,
    args: { workspace: string; token: string; addr: string; sidecarType?: string },
    onStdout: (line: string) => void,
  ): Promise<number | null> {
    if (!ssid) return null
    const s = ensure(ssid)

    // 清理旧监听
    if (s.unlistenStdout) {
      s.unlistenStdout()
    }
    if (s.unlistenStderr) {
      s.unlistenStderr()
    }

    try {
      let pid: number
      let unlistenStdout: UnlistenFn
      let unlistenStderr: UnlistenFn

      if (isTauri) {
        const { invoke } = await import('@tauri-apps/api/core')
        const { listen } = await import('@tauri-apps/api/event')
        pid = await invoke<number>('spawn_sidecar', {
          ssid,
          workspace: args.workspace,
          token: args.token,
          addr: args.addr,
          sidecarType: args.sidecarType || 'pi',
        })
        unlistenStdout = await listen<string>(
          `sidecar-stdout-${ssid}`,
          (event) => onStdout(event.payload),
        )
        unlistenStderr = await listen<string>(
          `sidecar-stderr-${ssid}`,
          () => {
            /* stderr ignored in UI */
          },
        )
      } else {
        // Web 端：进程存活于服务端，输出经 /stream WS 广播
        const res = await request<{ pid: number }>('/api/ai/sidecar/spawn', {
          method: 'POST',
          json: {
            ssid,
            workspace: args.workspace,
            token: args.token,
            addr: args.addr,
            sidecarType: args.sidecarType || 'pi',
          },
        })
        pid = res?.pid ?? 0
        unlistenStdout = await openSidecarStream(ssid, onStdout)
        unlistenStderr = () => {
          /* 与 stdout 共用一条 WS，由 unlistenStdout 统一关闭 */
        }
      }

      patch(ssid, {
        sidecarPid: pid,
        isSessionActive: true,
        isTyping: false,
        unlistenStdout,
        unlistenStderr,
      })
      return pid
    } catch (error) {
      console.error('[AI store] spawn failed:', error)
      patch(ssid, {
        sidecarPid: null,
        isSessionActive: false,
      })
      return null
    }
  }

  /**
   * 用移交的历史消息填充会话（独立窗口弹出时经 localStorage 移交）。
   * seq 取最大消息 id，保证后续 pushMessage 的 id 不冲突；不动 sidecar 相关状态。
   */
  function hydrate(ssid: string, messages: ChatMessage[]) {
    if (!ssid || messages.length === 0) return
    const s = ensure(ssid)
    if (s.messages.length > 0) return
    const seq = messages.reduce((max, m) => Math.max(max, m.id), 0)
    sessions.value = {
      ...sessions.value,
      [ssid]: { ...s, messages: [...messages], seq },
    }
  }

  /**
   * 附着到后端已在运行的 sidecar（不 spawn）。
   *
   * 独立 AI 窗口 / 多窗口共享同一 ssid 的场景：spawn 会先 kill 同 ssid 的
   * 旧进程，直接调会打断别的窗口正在进行的对话。这里先查运行状态，进程
   * 存在时只订阅输出流（桌面 Tauri emit 广播到所有窗口 / Web WS 广播，
   * 多窗口各自订阅互不干扰）并回填 pid。
   *
   * 返回附着到的 pid；后端无该 ssid 的进程时返回 null（调用方走 spawn）。
   */
  async function attachFor(
    ssid: string,
    onStdout: (line: string) => void,
  ): Promise<number | null> {
    if (!ssid) return null

    const s0 = ensure(ssid)
    if (s0.unlistenStdout) {
      s0.unlistenStdout()
    }
    if (s0.unlistenStderr) {
      s0.unlistenStderr()
    }

    try {
      let running = false
      let unlistenStdout: UnlistenFn
      let unlistenStderr: UnlistenFn

      if (isTauri) {
        const { invoke } = await import('@tauri-apps/api/core')
        const { listen } = await import('@tauri-apps/api/event')
        running = await invoke<boolean>('has_sidecar', { ssid })
        if (!running) return null
        unlistenStdout = await listen<string>(
          `sidecar-stdout-${ssid}`,
          (event) => onStdout(event.payload),
        )
        unlistenStderr = await listen<string>(
          `sidecar-stderr-${ssid}`,
          () => {
            /* stderr ignored in UI */
          },
        )
      } else {
        // Web 端：状态查询 + WS 订阅输出流
        const status = await request<{ running: boolean; pid: number | null }>(
          `/api/ai/sidecar/${encodeURIComponent(ssid)}`,
        )
        running = status?.running ?? false
        if (!running) return null
        unlistenStdout = await openSidecarStream(ssid, onStdout)
        unlistenStderr = () => {
          /* 与 stdout 共用一条 WS */
        }
      }

      let pid: number | null = null
      try {
        if (isTauri) {
          const { invoke } = await import('@tauri-apps/api/core')
          pid = await invoke<number | null>('get_sidecar_pid', { ssid })
        }
      } catch {
        pid = null
      }
      if (!isTauri) {
        try {
          const status = await request<{ pid: number | null }>(
            `/api/ai/sidecar/${encodeURIComponent(ssid)}`,
          )
          pid = status?.pid ?? null
        } catch {
          pid = null
        }
      }

      patch(ssid, {
        sidecarPid: pid ?? null,
        isSessionActive: true,
        isTyping: false,
        unlistenStdout,
        unlistenStderr,
      })
      return pid ?? null
    } catch (error) {
      console.error('[AI store] attach failed:', error)
      return null
    }
  }

  /** 终止指定 ssid 的 sidecar 并从 store 移除会话（含监听器清理） */
  async function killFor(ssid: string) {
    if (!ssid) return
    const s = sessions.value[ssid]
    if (!s) return

    if (s.unlistenStdout) s.unlistenStdout()
    if (s.unlistenStderr) s.unlistenStderr()

    if (s.sidecarPid !== null) {
      try {
        if (isTauri) {
          const { invoke } = await import('@tauri-apps/api/core')
          await invoke('kill_sidecar', { ssid })
        } else {
          await request('/api/ai/sidecar/kill', {
            method: 'POST',
            json: { ssid },
          })
        }
      } catch (error) {
        console.error('[AI store] kill failed:', error)
      }
    }

    const next = { ...sessions.value }
    delete next[ssid]
    sessions.value = next
  }

  /** 仅向指定 ssid 的 sidecar 写数据（前端 sendMessage / approval / __QUIT__ 都走这里） */
  async function writeTo(ssid: string, data: string) {
    if (!ssid) return
    if (isTauri) {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('write_to_sidecar', { ssid, data })
      return
    }
    await request('/api/ai/sidecar/write', {
      method: 'POST',
      json: { ssid, data },
    })
  }

  return {
    sessions,
    liveText,
    ensure,
    get,
    patch,
    pushMessage,
    updateMessage,
    appendProcessStep,
    finalizeProcess,
    startStreamingBubble,
    appendStreamingMessage,
    finalizeStreamingSegment,
    endStreamingTurn,
    attachUsage,
    clearMessages,
    hydrate,
    attachFor,
    spawnFor,
    killFor,
    writeTo,
  }
})
