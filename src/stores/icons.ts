import { defineStore } from "pinia"
import { computed, ref } from "vue"
import { buildIconUrl, listIcons } from "@/api/icons"
import type { IconItem } from "@/types"

/**
 * 主机图标列表（~/.ashell/icons/）。
 *
 * 设计：列表懒加载并缓存；URL 解析放在 store 内，组件层只拿同步映射。
 *
 * 缓存失效策略：URL 上拼 `?v=<mtime>`。`refresh()` 拉到新列表后逐项比对
 * 上次记录的 mtime；若 mtime 变化（用户替换/重命名了图标文件）就重新
 * 解析 URL，浏览器会按新 query 重新加载；mtime 一致则复用上次的 URL，
 * <img> 仍可命中浏览器缓存。
 */
export const useIconStore = defineStore("icons", () => {
  const items = ref<IconItem[]>([])
  /** name → 已解析的 URL（带 token + v=mtime） */
  const urls = ref<Record<string, string>>({})
  /** name → 上次记录的 mtime（用于决定是否需要重新拼 URL） */
  const mtimes = ref<Record<string, number>>({})
  const loaded = ref(false)
  const loading = ref(false)
  const error = ref<string | null>(null)

  const names = computed(() => items.value.map((it) => it.name))

  /** 全量刷新：拉新列表，按 mtime 决定哪些 URL 需要重建 */
  async function refresh() {
    loading.value = true
    error.value = null
    try {
      const list = await listIcons()
      const nextUrls: Record<string, string> = {}
      const nextMtimes: Record<string, number> = {}
      for (const it of list) {
        const prevMtime = mtimes.value[it.name]
        const prevUrl = urls.value[it.name]
        if (prevUrl != null && prevMtime === it.mtime) {
          // 文件未变化，复用旧 URL（可命中浏览器缓存）
          nextUrls[it.name] = prevUrl
        } else {
          nextUrls[it.name] = await buildIconUrl(it.name, it.mtime)
        }
        nextMtimes[it.name] = it.mtime
      }
      items.value = list
      urls.value = nextUrls
      mtimes.value = nextMtimes
      loaded.value = true
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function ensureLoaded() {
    if (loaded.value || loading.value) return
    await refresh()
  }

  /** 同步获取某个 name 对应的 URL，未加载或不存在时返回 null */
  function urlOf(name: string | null | undefined): string | null {
    if (!name) return null
    return urls.value[name] ?? null
  }

  return {
    items,
    names,
    urls,
    mtimes,
    loaded,
    loading,
    error,
    refresh,
    ensureLoaded,
    urlOf,
  }
})
