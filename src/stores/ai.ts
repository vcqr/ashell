import { defineStore } from 'pinia'
import { ref } from 'vue'
import { buildWsUrl, request } from '@/api/client'
import { isTauri } from '@/utils/platform'
import type { ChatMessage, ProcessStep } from '@/types'

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
    const full: ChatMessage = { id, ...msg }
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

  function clearMessages(ssid: string) {
    const s = sessions.value[ssid]
    if (!s) return
    sessions.value = {
      ...sessions.value,
      [ssid]: {
        ...s,
        messages: [],
        seq: 0,
        isApprovalActive: false,
        isTyping: false,
        currentProcessMsgId: null,
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
          sidecarType: args.sidecarType || 'claude',
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
            sidecarType: args.sidecarType || 'claude',
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
    ensure,
    get,
    patch,
    pushMessage,
    updateMessage,
    appendProcessStep,
    finalizeProcess,
    clearMessages,
    hydrate,
    attachFor,
    spawnFor,
    killFor,
    writeTo,
  }
})
