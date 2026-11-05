import { onBeforeUnmount, ref } from "vue"

/**
 * sudo 密码自动填充状态机：后端识别到 sudo 密码提示（sudo_prompt）后
 * armSudo 进入 15 秒待填充状态。
 *
 * 确认分两层：
 * - 强置信（[sudo]/doas 行首锚定、紧邻回显的裸 Password:）：回车即可
 *   确认——这些提示形态几乎不会误报；
 * - 弱置信（sudo -p 自定义短提示等，sudoPrompt.tier === "weak"）：回车
 *   不确认，只认点击提示条或 Ctrl+Enter——回车是最高频按键，弱置信误
 *   命中时回车确认会把密码明文注入命令行（回显可见、进 shell history）。
 *
 * 待填充期间输入其他字符立即退出。
 */
export function useSudoFill() {
  const sudoArmed = ref(false)
  const sudoStrongTier = ref(true)
  let sudoTimer: number | null = null

  function clearSudoTimer() {
    if (sudoTimer !== null) {
      window.clearTimeout(sudoTimer)
      sudoTimer = null
    }
  }

  function disarmSudo() {
    sudoArmed.value = false
    clearSudoTimer()
  }

  function armSudo(strongTier = true) {
    sudoArmed.value = true
    sudoStrongTier.value = strongTier
    clearSudoTimer()
    sudoTimer = window.setTimeout(() => {
      sudoArmed.value = false
      sudoTimer = null
    }, 15_000)
  }

  onBeforeUnmount(disarmSudo)

  return { sudoArmed, sudoStrongTier, armSudo, disarmSudo }
}
