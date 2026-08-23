import { onBeforeUnmount, onMounted, ref, type Ref } from "vue"
import type { TransferTask } from "@/types"

/**
 * 传输速度采样：每秒对 running 任务做 loaded 差分，指数平滑抑制跳动。
 *
 * 只在挂载期间采样（任务列表弹窗打开时），弹窗关闭即停——速度只在列表可见时
 * 展示，重开后 1-2 秒内重新收敛，不浪费后台定时器。
 */
export function useTransferSpeed(tasks: Ref<TransferTask[]>) {
  /** taskId -> 字节/秒 */
  const speeds = ref<Record<string, number>>({})
  const samples = new Map<string, { t: number; loaded: number }>()
  let timer: number | null = null

  function sample() {
    const now = Date.now()
    const next: Record<string, number> = {}
    for (const task of tasks.value) {
      if (task.status !== "running") {
        samples.delete(task.id)
        continue
      }
      const prev = samples.get(task.id)
      samples.set(task.id, { t: now, loaded: task.loaded })
      if (!prev) continue
      const dt = (now - prev.t) / 1000
      if (dt <= 0) continue
      const db = task.loaded - prev.loaded
      if (db < 0) continue
      const inst = db / dt
      const old = speeds.value[task.id]
      next[task.id] = old != null && old > 0 ? old * 0.6 + inst * 0.4 : inst
    }
    speeds.value = next
  }

  onMounted(() => {
    timer = window.setInterval(sample, 1000)
  })
  onBeforeUnmount(() => {
    if (timer !== null) window.clearInterval(timer)
    timer = null
  })

  return { speeds }
}
