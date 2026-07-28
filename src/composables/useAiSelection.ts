import { ref, type Ref } from "vue"
import type { Terminal } from "@xterm/xterm"

interface AiSelectionOptions {
  getTerm: () => Terminal | null
  containerRef: Ref<HTMLDivElement | null>
  onSend: (text: string) => void
}

/**
 * 选中文本"发送给 AI"浮层：mouseup 时若有选区，在鼠标附近显示 ✨ 按钮
 * （限制在终端容器内），点击弹出提示词输入框，Enter 发送、Escape 取消。
 */
export function useAiSelection({ getTerm, containerRef, onSend }: AiSelectionOptions) {
  const aiButtonVisible = ref(false)
  const aiButtonX = ref(0)
  const aiButtonY = ref(0)
  const aiPromptVisible = ref(false)
  const aiPromptText = ref("")
  const aiSelectionText = ref("")

  function onMouseUp(e: MouseEvent) {
    const term = getTerm()
    if (!term) return
    if (aiPromptVisible.value) {
      cancelAiPrompt()
      return
    }
    const sel = term.getSelection()
    if (sel && sel.trim()) {
      const rect = containerRef.value?.getBoundingClientRect()
      if (!rect) return
      // ✨ 按钮 28x28，限制在容器内
      const BTN = 28
      const rawX = e.clientX - rect.left + 10
      const rawY = e.clientY - rect.top - 36
      aiButtonX.value = Math.max(4, Math.min(rawX, rect.width - BTN - 4))
      aiButtonY.value = Math.max(4, Math.min(rawY, rect.height - BTN - 4))
      aiButtonVisible.value = true
    } else {
      aiButtonVisible.value = false
    }
  }

  function openAiPrompt() {
    const term = getTerm()
    if (!term) return
    const sel = term.getSelection()
    if (!sel || !sel.trim()) return
    aiSelectionText.value = sel
    term.clearSelection()
    // 提示词输入框约 360x40，重新限制在容器内
    const rect = containerRef.value?.getBoundingClientRect()
    if (rect) {
      const PW = 360
      const PH = 40
      aiButtonX.value = Math.max(4, Math.min(aiButtonX.value, rect.width - PW - 4))
      aiButtonY.value = Math.max(4, Math.min(aiButtonY.value, rect.height - PH - 4))
    }
    aiButtonVisible.value = false
    aiPromptText.value = ""
    aiPromptVisible.value = true
  }

  function submitAiPrompt() {
    const sel = aiSelectionText.value
    if (!sel.trim()) {
      cancelAiPrompt()
      return
    }
    const prompt = aiPromptText.value.trim()
    const combined = prompt ? `${prompt}\n\n${sel}` : sel
    aiPromptVisible.value = false
    aiPromptText.value = ""
    aiSelectionText.value = ""
    onSend(combined)
  }

  function cancelAiPrompt() {
    aiPromptVisible.value = false
    aiPromptText.value = ""
    aiSelectionText.value = ""
  }

  function onAiPromptKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault()
      submitAiPrompt()
    } else if (e.key === "Escape") {
      e.preventDefault()
      cancelAiPrompt()
    }
  }

  return {
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
  }
}
