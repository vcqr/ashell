import { computed, nextTick, onBeforeUnmount, ref, useTemplateRef, watch } from "vue"
import type { Terminal } from "@xterm/xterm"
import { SearchAddon, type ISearchOptions } from "@xterm/addon-search"
import { NInput } from "naive-ui"
import { useI18n } from "vue-i18n"
import { useTerminalStore } from "@/stores/terminal"

/**
 * 终端搜索浮层：Ctrl/Cmd+F 唤起，SearchAddon 首次使用时才懒加载。
 * searchInputRef 通过 useTemplateRef 按 key 关联组件模板里 `ref="searchInputRef"`
 * 的 NInput（组件侧无需持有该 ref）。
 */
export function useTerminalSearch(getTerm: () => Terminal | null) {
  const { t } = useI18n()
  const termStore = useTerminalStore()

  const searchOpen = ref(false)
  const searchKeyword = ref("")
  const searchCaseSensitive = ref(false)
  const searchWholeWord = ref(false)
  const searchRegex = ref(false)
  const searchResultIndex = ref(-1)
  const searchResultCount = ref(0)
  const searchInputRef = useTemplateRef<InstanceType<typeof NInput>>("searchInputRef")

  let searchAddon: SearchAddon | null = null
  let searchDebounceTimer: number | null = null

  const searchResultText = computed(() => {
    if (!searchKeyword.value) return ""
    if (searchResultCount.value === 0) return t("terminal.search.noMatch")
    // resultIndex 是 0-based，展示用 1-based
    return `${searchResultIndex.value + 1} / ${searchResultCount.value}`
  })

  function getSearchOptions(): ISearchOptions {
    return {
      regex: searchRegex.value,
      wholeWord: searchWholeWord.value,
      caseSensitive: searchCaseSensitive.value,
      decorations: {
        matchBackground: "#5f4d2080",
        matchBorder: "#a08540",
        matchOverviewRuler: "#a08540",
        activeMatchBackground: "#d9923a",
        activeMatchBorder: "#ffffff",
        activeMatchColorOverviewRuler: "#d9923a",
      },
    }
  }

  function ensureSearchAddon(): SearchAddon | null {
    const term = getTerm()
    if (!term) return null
    if (searchAddon) return searchAddon
    const addon = new SearchAddon()
    term.loadAddon(addon)
    addon.onDidChangeResults((e) => {
      if (e === undefined) {
        searchResultIndex.value = -1
        searchResultCount.value = 0
      } else {
        searchResultIndex.value = e.resultIndex
        searchResultCount.value = e.resultCount
      }
    })
    searchAddon = addon
    return addon
  }

  function disposeSearchAddon() {
    if (!searchAddon) return
    try {
      searchAddon.clearDecorations()
    } catch {
      // ignore
    }
    try {
      searchAddon.dispose()
    } catch {
      // ignore
    }
    searchAddon = null
    searchResultIndex.value = -1
    searchResultCount.value = 0
  }

  function openSearchBar() {
    if (!termStore.searchHotkeyEnabled) return
    ensureSearchAddon()
    searchOpen.value = true
    void nextTick(() => {
      // NInput 的 focus 方法在内部 ref 上
      searchInputRef.value?.focus()
      if (searchKeyword.value) {
        runSearch("next")
      }
    })
  }

  function clearSearchDebounce() {
    if (searchDebounceTimer !== null) {
      window.clearTimeout(searchDebounceTimer)
      searchDebounceTimer = null
    }
  }

  /** 输入即搜：防抖 120ms，避免每次击键都在大 scrollback 上跑 findNext。 */
  function onSearchInput() {
    clearSearchDebounce()
    searchDebounceTimer = window.setTimeout(() => {
      searchDebounceTimer = null
      runSearch("next")
    }, 120)
  }

  function closeSearchBar() {
    clearSearchDebounce()
    searchOpen.value = false
    if (searchAddon) {
      try {
        // 关闭时清掉全部匹配高亮（而非仅 active），否则上一次搜索的
        // 装饰会一直残留在终端里
        searchAddon.clearDecorations()
      } catch {
        // ignore
      }
    }
    // 关闭后把焦点交还终端
    getTerm()?.focus()
  }

  function runSearch(direction: "next" | "previous") {
    const addon = ensureSearchAddon()
    if (!addon) return
    const kw = searchKeyword.value
    if (!kw) {
      // 关键词被删空：findNext 不接受空串，且必须主动清装饰，
      // 否则上一个关键词的匹配高亮会残留
      try {
        addon.clearDecorations()
      } catch {
        // ignore
      }
      searchResultIndex.value = -1
      searchResultCount.value = 0
      return
    }
    if (direction === "next") {
      addon.findNext(kw, getSearchOptions())
    } else {
      addon.findPrevious(kw, getSearchOptions())
    }
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault()
      runSearch(e.shiftKey ? "previous" : "next")
      return
    }
    if (e.key === "Escape") {
      e.preventDefault()
      closeSearchBar()
    }
  }

  // ===== 搜索条上的三个匹配开关（Aa / W / .*），模板 v-for 渲染 =====

  type SearchFlag = "caseSensitive" | "wholeWord" | "regex"

  const searchToggles = computed(() => [
    {
      key: "caseSensitive" as SearchFlag,
      label: "Aa",
      active: searchCaseSensitive.value,
      tooltip: t("terminal.search.caseSensitive"),
    },
    {
      key: "wholeWord" as SearchFlag,
      label: "W",
      active: searchWholeWord.value,
      tooltip: t("terminal.search.wholeWord"),
    },
    {
      key: "regex" as SearchFlag,
      label: ".*",
      active: searchRegex.value,
      tooltip: t("terminal.search.regex"),
    },
  ])

  function toggleSearchFlag(key: SearchFlag) {
    if (key === "caseSensitive") searchCaseSensitive.value = !searchCaseSensitive.value
    else if (key === "wholeWord") searchWholeWord.value = !searchWholeWord.value
    else searchRegex.value = !searchRegex.value
    runSearch("next")
  }

  watch(
    () => termStore.searchHotkeyEnabled,
    (enabled) => {
      // 关闭 Ctrl+F 仅意味着不再唤起搜索条；已打开的搜索条要主动收起，addon 也释放。
      if (!enabled) {
        if (searchOpen.value) closeSearchBar()
        disposeSearchAddon()
      }
    },
  )

  onBeforeUnmount(() => {
    clearSearchDebounce()
    disposeSearchAddon()
  })

  return {
    searchOpen,
    searchKeyword,
    searchResultText,
    searchToggles,
    openSearchBar,
    closeSearchBar,
    runSearch,
    onSearchInput,
    onSearchKeydown,
    toggleSearchFlag,
  }
}
