import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
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
  /** stdout 监听句柄 */
  unlistenStdout: UnlistenFn | null
  /** stderr 监听句柄 */
  unlistenStderr: UnlistenFn | null
}

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
 * 一个 SSH 终端会话（ssid）对应一个独立的 sidecar 进程与对话历史；
 * 切换终端 tab 时只是切换显示哪个 ssid 的状态，不重启进程。
 * 仅在 SSH session 真正断开时（终端 status 变为 closed/error）才 kill sidecar。
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
   * 启动指定 ssid 的 sidecar；若已运行先 kill 旧进程。
   * 同时按 ssid 注册 stdout/stderr 监听器。
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
      const pid = await invoke<number>('spawn_sidecar', {
        ssid,
        workspace: args.workspace,
        token: args.token,
        addr: args.addr,
        sidecarType: args.sidecarType || 'claude',
      })

      const unlistenStdout = await listen<string>(
        `sidecar-stdout-${ssid}`,
        (event) => onStdout(event.payload),
      )
      const unlistenStderr = await listen<string>(
        `sidecar-stderr-${ssid}`,
        () => {
          /* stderr ignored in UI */
        },
      )

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

  /** 终止指定 ssid 的 sidecar 并从 store 移除会话（含监听器清理） */
  async function killFor(ssid: string) {
    if (!ssid) return
    const s = sessions.value[ssid]
    if (!s) return

    if (s.unlistenStdout) s.unlistenStdout()
    if (s.unlistenStderr) s.unlistenStderr()

    if (s.sidecarPid !== null) {
      try {
        await invoke('kill_sidecar', { ssid })
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
    await invoke('write_to_sidecar', { ssid, data })
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
    spawnFor,
    killFor,
    writeTo,
  }
})
