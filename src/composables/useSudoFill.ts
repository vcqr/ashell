import { onBeforeUnmount, ref } from "vue"

/**
 * sudo 密码自动填充状态机：后端识别到 sudo 密码提示（sudo_prompt）后
 * armSudo 进入 15 秒武装期，期间下一个回车被解释为"确认填充"
 * （onData 侧发 sudo_fill 让后端注入密码），输入其他字符立即解除。
 */
export function useSudoFill() {
  const sudoArmed = ref(false)
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

  function armSudo() {
    sudoArmed.value = true
    clearSudoTimer()
    sudoTimer = window.setTimeout(() => {
      sudoArmed.value = false
      sudoTimer = null
    }, 15_000)
  }

  onBeforeUnmount(disarmSudo)

  return { sudoArmed, armSudo, disarmSudo }
}
