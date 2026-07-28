import { defineStore } from "pinia"
import { computed, ref } from "vue"
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event"

/**
 * 多 Tab 广播输入状态（支持跨窗口）。
 *
 * 跨窗口设计：
 * - 每个窗口有唯一 windowId（Tauri window label）。
 * - tab 的全局唯一标识 = `${windowId}:${tabKey}`。
 * - 状态变更（enabled / sourceKey / targetKeys / appendCR）通过 Tauri event
 *   `ashell:broadcast-state` 广播到所有窗口，各窗口合并到本地 store。
 * - 输入字节通过 `ashell:broadcast-input` event 跨窗口转发：源窗口 fanout 时
 *   本地 target 直接注入 ws，跨窗口 target 走 emit；目标窗口 listen 后注入
 *   对应 tab 的 ws。
 * - "跟随激活 tab"的 source 语义仅在源窗口本地生效；跨窗口的 source 必须
 *   是显式锁定的全局 key。
 */
export const useBroadcastStore = defineStore("broadcast", () => {
  /** 本窗口的 Tauri window label，init() 时赋值。 */
  const windowId = ref<string>("")

  /** 总开关。关闭时 fanout 直接 no-op。 */
  const enabled = ref(false)

  /**
   * 源 tab 的全局 key（`windowId:tabKey`）。
   * - null = 跟随当前激活 tab（仅本窗口语义）
   * - 非 null = 用户手动锁定的源
   */
  const sourceKey = ref<string | null>(null)

  /** 目标 tab 的全局 key 集合。 */
  const targetKeys = ref<Set<string>>(new Set())

  /** 发送命令文本时是否自动追加 \r。 */
  const appendCR = ref(true)

  /** TerminalView 注册的"向自己 ws 注入输入字节"的回调。key = tabKey（本窗口内）。 */
  const inputSenders = new Map<string, (data: string) => void>()

  let unlistenState: UnlistenFn | null = null
  let unlistenInput: UnlistenFn | null = null
  let unlistenTabs: UnlistenFn | null = null
  /** 防止自己 emit 的事件被自己 listen 回来导致循环。 */
  let suppressStateSync = false

  /**
   * 远程 tab 目录：其他窗口广播过来的 tab 列表。
   * key = windowId, value = RemoteTabInfo[]
   * BroadcastPopover 用它 + 本窗口 tabs 聚合展示全部可选 tab。
   */
  const remoteTabs = ref<Map<string, RemoteTabInfo[]>>(new Map())

  /**
   * 初始化跨窗口广播。在 App.vue setup 中调用一次。
   * @param wid 当前窗口的 Tauri label
   */
  async function init(wid: string) {
    windowId.value = wid

    unlistenState = await listen<BroadcastStatePayload>(
      "ashell:broadcast-state",
      (e) => {
        if (e.payload.origin === wid) return
        suppressStateSync = true
        enabled.value = e.payload.enabled
        sourceKey.value = e.payload.sourceKey
        targetKeys.value = new Set(e.payload.targetKeys)
        appendCR.value = e.payload.appendCR
        suppressStateSync = false
      },
    )

    unlistenInput = await listen<BroadcastInputPayload>(
      "ashell:broadcast-input",
      (e) => {
        if (e.payload.origin === wid) return
        for (const gkey of e.payload.targetKeys) {
          const [w, tabKey] = splitGlobalKey(gkey)
          if (w !== wid) continue
          const sender = inputSenders.get(tabKey)
          if (!sender) continue
          try {
            sender(e.payload.data)
          } catch {
            // ignore
          }
        }
      },
    )

    unlistenTabs = await listen<TabsAnnouncementPayload>(
      "ashell:broadcast-tabs",
      (e) => {
        if (e.payload.origin === wid) return
        const next = new Map(remoteTabs.value)
        if (e.payload.tabs.length === 0) {
          next.delete(e.payload.origin)
        } else {
          next.set(e.payload.origin, e.payload.tabs)
        }
        remoteTabs.value = next
      },
    )
  }

  function destroy() {
    unlistenState?.()
    unlistenInput?.()
    unlistenTabs?.()
    unlistenState = null
    unlistenInput = null
    unlistenTabs = null
  }

  /**
   * App.vue 在 tabs 变化时调用：把本窗口 tab 列表广播给其他窗口。
   * 同时更新 localTabSnapshot 供 getRemoteTabSnapshot 使用。
   */
  function announceTabs(tabs: RemoteTabInfo[]) {
    void emit("ashell:broadcast-tabs", {
      origin: windowId.value,
      tabs,
    } satisfies TabsAnnouncementPayload)
  }

  /**
   * 获取所有可选 tab（本窗口 + 远程窗口），供 BroadcastPopover 展示。
   * @param localTabs 本窗口的 TerminalTab[]
   * @param localWindowTitle 本窗口标题
   */
  function getAllTabs(
    localTabs: { key: string; title: string; kind?: string }[],
  ): { gkey: string; title: string; kind: string; isLocal: boolean; windowId: string }[] {
    const result: { gkey: string; title: string; kind: string; isLocal: boolean; windowId: string }[] = []
    // 本窗口
    for (const t of localTabs) {
      result.push({
        gkey: globalKey(t.key),
        title: t.title,
        kind: t.kind ?? "ssh",
        isLocal: true,
        windowId: windowId.value,
      })
    }
    // 远程窗口
    for (const [wid, tabs] of remoteTabs.value) {
      for (const t of tabs) {
        result.push({
          gkey: `${wid}:${t.key}`,
          title: t.title,
          kind: t.kind,
          isLocal: false,
          windowId: wid,
        })
      }
    }
    return result
  }

  /** 把本窗口的 tabKey 转成全局 key。 */
  function globalKey(tabKey: string): string {
    return `${windowId.value}:${tabKey}`
  }

  /** 拆分全局 key → [windowId, tabKey]。 */
  function splitGlobalKey(gkey: string): [string, string] {
    const idx = gkey.indexOf(":")
    if (idx < 0) return ["", gkey]
    return [gkey.slice(0, idx), gkey.slice(idx + 1)]
  }

  /** 状态变更后广播给其他窗口。 */
  function syncState() {
    if (suppressStateSync) return
    void emit("ashell:broadcast-state", {
      origin: windowId.value,
      enabled: enabled.value,
      sourceKey: sourceKey.value,
      targetKeys: [...targetKeys.value],
      appendCR: appendCR.value,
    } satisfies BroadcastStatePayload)
  }

  function registerSender(tabKey: string, send: (data: string) => void) {
    inputSenders.set(tabKey, send)
  }

  function unregisterSender(tabKey: string) {
    inputSenders.delete(tabKey)
  }

  /** 由 App.vue 在 tab 关闭时调用。 */
  function purgeKey(tabKey: string) {
    const gkey = globalKey(tabKey)
    targetKeys.value.delete(gkey)
    if (sourceKey.value === gkey) sourceKey.value = null
    inputSenders.delete(tabKey)
    syncState()
  }

  function toggleTarget(gkey: string) {
    const next = new Set(targetKeys.value)
    if (next.has(gkey)) next.delete(gkey)
    else next.add(gkey)
    targetKeys.value = next
    syncState()
  }

  function setTargets(gkeys: string[]) {
    targetKeys.value = new Set(gkeys)
    syncState()
  }

  function clearTargets() {
    targetKeys.value = new Set()
    syncState()
  }

  function setEnabled(v: boolean) {
    enabled.value = v
    syncState()
  }

  function setSourceKey(gkey: string | null) {
    sourceKey.value = gkey
    syncState()
  }

  function setAppendCR(v: boolean) {
    appendCR.value = v
    syncState()
  }

  /**
   * 由源 TerminalView 在 onData 里调用。
   * @param fromTabKey 调用方自身 tab key（本窗口内）
   * @param data       用户敲下的字节流
   */
  function fanout(fromTabKey: string, data: string) {
    if (!enabled.value) return
    if (targetKeys.value.size === 0) return

    const fromGkey = globalKey(fromTabKey)
    // 收集本地 target 和跨窗口 target
    const crossWindowTargets: string[] = []

    for (const gkey of targetKeys.value) {
      if (gkey === fromGkey) continue
      const [w, tabKey] = splitGlobalKey(gkey)
      if (w === windowId.value) {
        // 本地 target：直接注入
        const sender = inputSenders.get(tabKey)
        if (sender) {
          try {
            sender(data)
          } catch {
            // ignore
          }
        }
      } else {
        crossWindowTargets.push(gkey)
      }
    }

    // 跨窗口 target：通过 Tauri event 转发
    if (crossWindowTargets.length > 0) {
      void emit("ashell:broadcast-input", {
        origin: windowId.value,
        data,
        targetKeys: crossWindowTargets,
      } satisfies BroadcastInputPayload)
    }
  }

  /**
   * 当前生效的源 tab。
   * @param activeTabKey 本窗口当前激活 tab 的 key（用于"跟随激活"模式）
   * @param activeWindowId 本窗口 id
   */
  function effectiveSource(
    activeTabKey: string | null,
    activeWid: string | null,
  ): string | null {
    if (sourceKey.value) return sourceKey.value
    if (activeTabKey && activeWid) return `${activeWid}:${activeTabKey}`
    return null
  }

  const isActive = computed(() => enabled.value && targetKeys.value.size > 0)

  return {
    windowId,
    enabled,
    sourceKey,
    targetKeys,
    appendCR,
    remoteTabs,
    isActive,
    init,
    destroy,
    announceTabs,
    getAllTabs,
    globalKey,
    splitGlobalKey,
    registerSender,
    unregisterSender,
    purgeKey,
    toggleTarget,
    setTargets,
    clearTargets,
    setEnabled,
    setSourceKey,
    setAppendCR,
    fanout,
    effectiveSource,
  }
})

interface RemoteTabInfo {
  key: string
  title: string
  kind: string
}

interface BroadcastStatePayload {
  origin: string
  enabled: boolean
  sourceKey: string | null
  targetKeys: string[]
  appendCR: boolean
}

interface BroadcastInputPayload {
  origin: string
  data: string
  targetKeys: string[]
}

interface TabsAnnouncementPayload {
  origin: string
  tabs: RemoteTabInfo[]
}
