import { onBeforeUnmount, ref } from "vue"
import type { Terminal } from "@xterm/xterm"
import { ProgressAddon, type IProgressState } from "@xterm/addon-progress"
import { getCurrentWindow, ProgressBarStatus } from "@tauri-apps/api/window"
import { useTerminalStore } from "@/stores/terminal"
import {
  incompleteEscapeStart,
  matchTextProgress,
  stripAnsiForProgress,
  type TextProgressMatch,
} from "@/utils/textProgress"

interface TerminalProgressOptions {
  getTerm: () => Terminal | null
  isActive: () => boolean
}

type ProgressState = IProgressState["state"]

/** OSC 9;4 进度状态 → Tauri 任务栏进度条状态。 */
const TAURI_PROGRESS_STATUS: Record<ProgressState, ProgressBarStatus> = {
  0: ProgressBarStatus.None,
  1: ProgressBarStatus.Normal,
  2: ProgressBarStatus.Error,
  3: ProgressBarStatus.Indeterminate,
  4: ProgressBarStatus.Paused,
}

/**
 * 终端进度子系统：OSC 9;4（ProgressAddon）与文本型进度解析共用同一对
 * progressState / progressValue，同时驱动两个出口——TerminalView 顶部
 * 3px 进度条与 Tauri 任务栏角标（只有激活 tab 写任务栏）。
 *
 * state: 0=None / 1=Normal / 2=Error / 3=Indeterminate / 4=Paused
 * value: 0..100，indeterminate 时被忽略
 */
export function useTerminalProgress({ getTerm, isActive }: TerminalProgressOptions) {
  const termStore = useTerminalStore()

  const progressState = ref<ProgressState>(0)
  const progressValue = ref(0)

  let progressAddon: ProgressAddon | null = null
  let textProgressDecoder = new TextDecoder()
  let textLineBuffer = ""
  // 被 chunk 边界截断的不完整 ESC 序列，留到下一个 chunk 拼接后再处理（见 feedDecodedText）
  let ansiTail = ""
  let textProgressActive = false
  let textProgressClearTimer: number | null = null

  /**
   * Tauri 窗口任务栏进度条句柄。多个 tab 都开了 progress 时，只有最后一次设置生效——
   * Windows 任务栏图标只有一个。激活的 tab 设值；非激活 tab 一律不写入，避免后台 tab
   * 抢占任务栏角标显示。
   */
  function applyTauriProgress(state: IProgressState) {
    if (!isActive()) return
    const status = TAURI_PROGRESS_STATUS[state.state] ?? ProgressBarStatus.None
    void getCurrentWindow()
      .setProgressBar({ status, progress: state.value })
      .catch(() => {
        // 平台不支持任务栏进度（如 Linux 部分桌面环境）—忽略
      })
  }

  function clearTauriProgress() {
    void getCurrentWindow()
      .setProgressBar({ status: ProgressBarStatus.None })
      .catch(() => {
        // ignore
      })
  }

  function loadProgress() {
    const term = getTerm()
    if (!term || progressAddon) return
    const addon = new ProgressAddon()
    term.loadAddon(addon)
    addon.onChange((state) => {
      // 同时驱动两个出口：Tauri 任务栏角标 + TerminalView 顶部进度条
      progressState.value = state.state
      progressValue.value = state.value
      applyTauriProgress(state)
    })
    progressAddon = addon
  }

  function disposeProgress() {
    if (!progressAddon) return
    try {
      progressAddon.dispose()
    } catch {
      // ignore
    }
    progressAddon = null
    progressState.value = 0
    progressValue.value = 0
    clearTauriProgress()
  }

  function clearTextProgressTimer() {
    if (textProgressClearTimer !== null) {
      window.clearTimeout(textProgressClearTimer)
      textProgressClearTimer = null
    }
  }

  function clearTextProgressUi() {
    clearTextProgressTimer()
    textProgressActive = false
    // OSC 进度还在跑时不要清 UI
    if (progressAddon && progressAddon.progress.state !== 0) return
    if (progressState.value !== 0) {
      progressState.value = 0
      progressValue.value = 0
      if (isActive()) clearTauriProgress()
    }
  }

  function resetTextProgress() {
    clearTextProgressUi()
    textLineBuffer = ""
    ansiTail = ""
    textProgressDecoder = new TextDecoder()
  }

  function applyTextProgress(match: TextProgressMatch) {
    clearTextProgressTimer()
    // OSC 进度在跑，让 OSC 主导
    if (progressAddon && progressAddon.progress.state !== 0) return
    textProgressActive = true
    if (match.indeterminate) {
      // state=3 indeterminate：value 被 CSS 忽略，传 0 即可
      progressState.value = 3
      progressValue.value = 0
      if (isActive()) applyTauriProgress({ state: 3, value: 0 })
      return
    }
    progressState.value = 1
    progressValue.value = match.percent
    if (isActive()) applyTauriProgress({ state: 1, value: match.percent })
    // 完成（n/n 或 100%）：短暂亮 100% 再收掉，给用户一个"完成"的视觉反馈
    if (match.done) {
      textProgressClearTimer = window.setTimeout(() => {
        textProgressClearTimer = null
        textProgressActive = false
        if (progressAddon && progressAddon.progress.state !== 0) return
        progressState.value = 0
        progressValue.value = 0
        if (isActive()) clearTauriProgress()
      }, 1200)
    }
  }

  function feedTextProgress(text: string) {
    if (!termStore.progressEnabled) return
    // 按 \r / \n 切分（保留分隔符），逐段处理：
    // - \r：清空当前行缓冲（工具要重写这一行）
    // - \n：整行完成，若该行不含进度模式且此前文本进度在跑，收掉进度条
    //   （spinner 用 \r 重写当前行，\n 说明它已结束；具体进度条同理）
    // - 其他：累积到行缓冲，并尝试匹配最新一次进度
    const parts = text.split(/([\r\n])/)
    for (const part of parts) {
      if (part === "\r") {
        textLineBuffer = ""
      } else if (part === "\n") {
        const line = textLineBuffer
        textLineBuffer = ""
        if (textProgressActive && !matchTextProgress(line)) {
          clearTextProgressUi()
        }
      } else if (part) {
        textLineBuffer += part
        if (textLineBuffer.length > 512) {
          textLineBuffer = textLineBuffer.slice(-512)
        }
        const m = matchTextProgress(textLineBuffer)
        if (m) applyTextProgress(m)
      }
    }
  }

  /**
   * 拼接跨 chunk 的 ANSI 尾巴、剥离控制序列后再喂给文本进度解析。
   * 尾部不完整的 ESC 序列暂存 ansiTail，等下一个 chunk 到来拼成完整序列，
   * 避免半截序列污染行缓冲。尾巴超过 256 字符说明对端永远不结束该序列，
   * 直接冲刷（stripper 会把它吞到串尾）。
   */
  function feedDecodedText(text: string) {
    const combined = ansiTail ? ansiTail + text : text
    const holdFrom = incompleteEscapeStart(combined)
    if (holdFrom >= 0 && combined.length - holdFrom <= 256) {
      ansiTail = combined.slice(holdFrom)
      if (holdFrom > 0) feedTextProgress(stripAnsiForProgress(combined.slice(0, holdFrom)))
      return
    }
    ansiTail = ""
    feedTextProgress(stripAnsiForProgress(combined))
  }

  /** 把远端输出同时喂给 xterm 和文本进度解析器。 */
  function writeToTerm(data: Uint8Array | string) {
    const term = getTerm()
    if (!term) return
    const progressEnabled = termStore.progressEnabled
    if (typeof data === "string") {
      term.write(data)
      if (progressEnabled) feedDecodedText(data)
      return
    }
    term.write(data)
    if (!progressEnabled) return
    const text = textProgressDecoder.decode(data, { stream: true })
    if (text) feedDecodedText(text)
  }

  /**
   * 切回本 tab 时把当前进度状态重新推到任务栏（前一个 tab 切走时已清零）。
   * 读 progressState.value 而非 progressAddon.progress，是为了兼容文本型
   * 进度解析设置的值——后者不会经过 ProgressAddon。
   */
  function syncTaskbar() {
    if (progressState.value !== 0) {
      applyTauriProgress({ state: progressState.value, value: progressValue.value })
    }
  }

  onBeforeUnmount(() => {
    resetTextProgress()
    // 卸载前确保任务栏角标不会残留
    if (isActive()) clearTauriProgress()
  })

  return {
    progressState,
    progressValue,
    loadProgress,
    disposeProgress,
    writeToTerm,
    resetTextProgress,
    syncTaskbar,
    clearTaskbar: clearTauriProgress,
  }
}
