import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getApiInfo } from '@/api/client'
import type { ApiInfo } from '@/types'

/** 全局 API 信息单例 */
export const useApiStore = defineStore('api', () => {
  const info = ref<ApiInfo | null>(null)
  const ready = ref(false)
  const error = ref<string | null>(null)

  async function init() {
    try {
      info.value = await getApiInfo()
      ready.value = true
      error.value = null
    } catch (e) {
      error.value = String(e)
      ready.value = false
    }
  }

  return { info, ready, error, init }
})
