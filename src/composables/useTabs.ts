import { computed, ref, watch } from "vue";
import type { TerminalTab, HostNode } from "@/types";
import { useAiStore } from "@/stores/ai";
import { useBroadcastStore } from "@/stores/broadcast";
import { useStartupStore } from "@/stores/startup";
import { useHostStore } from "@/stores/hosts";
import { openTabInNewWindow } from "@/utils/newWindow";

/** TerminalView 通过 defineExpose 暴露的实例方法 */
export type TerminalViewExposed = {
  disconnect: () => void;
  reconnect: () => Promise<void>;
  relayout: () => void;
  serializeSession: () => string;
};

/** AiAssistant 通过 defineExpose 暴露的实例方法 */
export type AiAssistantExposed = {
  callStreamingApi: (content: string) => void;
  sendText: (text: string) => Promise<void>;
};

const TABS_KEY = "ashell:tabs";

/** 仅持久化 tab 骨架；sid/status/lines 是运行时状态，重启后必然失效。 */
type PersistedTab = Pick<
  TerminalTab,
  | "key"
  | "title"
  | "kind"
  | "hostId"
  | "hostKey"
  | "icon"
  | "color"
  | "shell"
  | "hostInfo"
>;

interface PersistedTabs {
  tabs: PersistedTab[];
  activeKey: string;
}

function loadPersistedTabs(restoreTabs: boolean): PersistedTabs {
  if (typeof localStorage === "undefined") return { tabs: [], activeKey: "" };
  // 用户偏好关闭"记住打开的 tab"时，启动不恢复，并把已有落盘记录顺手抹掉，
  // 避免下次重新打开开关又看到上上次的旧状态。
  if (!restoreTabs) {
    try {
      localStorage.removeItem(TABS_KEY);
    } catch {
      // ignore
    }
    return { tabs: [], activeKey: "" };
  }
  try {
    const raw = localStorage.getItem(TABS_KEY);
    if (!raw) return { tabs: [], activeKey: "" };
    const parsed = JSON.parse(raw) as Partial<PersistedTabs>;
    if (!parsed || !Array.isArray(parsed.tabs)) {
      return { tabs: [], activeKey: "" };
    }
    return {
      tabs: parsed.tabs.filter(
        (t): t is PersistedTab =>
          !!t && typeof t.key === "string" && typeof t.title === "string",
      ),
      activeKey: typeof parsed.activeKey === "string" ? parsed.activeKey : "",
    };
  } catch {
    return { tabs: [], activeKey: "" };
  }
}

function findHostNode(nodes: HostNode[], id: number): HostNode | undefined {
  for (const n of nodes) {
    if (n.type === "host" && n.id === id) return n;
    if (n.children) {
      const found = findHostNode(n.children, id);
      if (found) return found;
    }
  }
  return undefined;
}

/**
 * 终端 Tab 生命周期管理：状态、持久化、打开/关闭/重排/重连等操作。
 *
 * hostsOpen（主机抽屉开关）也放在这里：打开 tab 的入口（openHost/openLocal/
 * onTabBarNew）会直接读写它，与 tab 操作耦合最紧，放一起避免跨 composable 环依赖。
 */
export function useTabs() {
  const aiStore = useAiStore();
  const broadcastStore = useBroadcastStore();
  const startupStore = useStartupStore();
  const hostStore = useHostStore();

  const hostsOpen = ref(false);

  const persisted = loadPersistedTabs(startupStore.restoreTabs);

  let tabSeq = persisted.tabs.reduce((max, t) => {
    const m = /^tab-(\d+)$/.exec(t.key);
    const n = m ? Number(m[1]) : 0;
    return Number.isFinite(n) && n > max ? n : max;
  }, 0);

  const tabs = ref<TerminalTab[]>(
    persisted.tabs.map((t) => ({
      ...t,
      status: "closed",
    })),
  );
  const activeTabKey = ref<string>(
    persisted.tabs.some((t) => t.key === persisted.activeKey)
      ? persisted.activeKey
      : (tabs.value[0]?.key ?? ""),
  );

  /**
   * 由持久化恢复的 tab 集合。默认这些 tab 的 TerminalView 应跳过自动 ws 连接，
   * 等用户手动右键"重新连接"；若启动偏好开启 autoConnectRememberedTabs，则恢复时也自动连接。
   * openHost / duplicateTab 等新建路径不会进入这个集合。
   */
  const restoredTabKeys = new Set<string>(persisted.tabs.map((t) => t.key));

  let saveTimer: number | null = null;
  function persistTabs() {
    if (typeof localStorage === "undefined") return;
    if (saveTimer !== null) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      saveTimer = null;
      // 用户偏好关闭"记住打开的 tab"时，运行期任何 tab 变化都不写盘，并保证已有
      // 记录被抹掉——这样下一次启动也是干净的。
      if (!startupStore.restoreTabs) {
        try {
          localStorage.removeItem(TABS_KEY);
        } catch {
          // ignore
        }
        return;
      }
      try {
        const snapshot: PersistedTabs = {
          tabs: tabs.value.map((t) => ({
            key: t.key,
            title: t.title,
            kind: t.kind,
            hostId: t.hostId,
            hostKey: t.hostKey,
            icon: t.icon ?? null,
            color: t.color ?? null,
            shell: t.shell ?? null,
            hostInfo: t.hostInfo,
          })),
          activeKey: activeTabKey.value,
        };
        localStorage.setItem(TABS_KEY, JSON.stringify(snapshot));
      } catch {
        // 配额满 / 隐私模式禁用 — 静默忽略，下次还会再尝试
      }
    }, 200);
  }

  watch([tabs, activeTabKey], () => persistTabs(), { deep: true });

  // tabs 变化时把本窗口 tab 列表广播给其他窗口（跨窗口广播功能依赖此目录）
  watch(
    tabs,
    (newTabs) => {
      broadcastStore.announceTabs(
        newTabs.map((t) => ({
          key: t.key,
          title: t.title,
          kind: t.kind ?? "ssh",
        })),
      );
    },
    { deep: true },
  );

  // 偏好从 true → false 时立刻抹掉已落盘的 tab 记录；从 false → true 时让下一次
  // tab 变化驱动 persistTabs 把当前状态写回。
  watch(
    () => startupStore.restoreTabs,
    (next) => {
      if (!next) {
        try {
          localStorage.removeItem(TABS_KEY);
        } catch {
          // ignore
        }
      } else {
        persistTabs();
      }
    },
  );

  const activeTab = computed<TerminalTab | undefined>(() =>
    tabs.value.find((t) => t.key === activeTabKey.value),
  );

  const activeSftpTab = computed<TerminalTab | undefined>(() => {
    const t = activeTab.value;
    if (!t || t.kind === "local" || t.kind === "telnet" || t.kind === "serial") return undefined;
    if (t.hostId === undefined || !t.sid) return undefined;
    // ws 中断 / 网络掉线时 status 会被推为 closed/error，此时下游面板（SFTP/主机信息/转发/AI）
    // 调后端必失败，统一在这里把 activeSftpTab 视为不可用，按钮自动置灰、已打开的抽屉被
    // watch(activeSftpTab) 联动关闭。
    // 本地 PTY tab（kind==='local'）不依赖任何 SSH 会话，永远不进这套面板。
    return t.status === "connected" ? t : undefined;
  });

  /**
   * AI 助手可用的激活 tab：比 activeSftpTab 更宽，包含本地 PTY tab。
   * 本地 tab 的 sid 由 /api/local/terminal ws ready 帧下发，后端 local_pty 已把
   * 同一 sid 注册进终端命令/输出通道，AI sidecar 走 /api/ssh/send/{sid} 一样能注入命令。
   * AI 助手总开关关闭时返回 undefined：面板收起、toggleAi / onSendToAi 自动失效。
   */
  const activeAiTab = computed<TerminalTab | undefined>(() => {
    if (!startupStore.aiAssistantEnabled) return undefined;
    const t = activeTab.value;
    if (!t || !t.sid) return undefined;
    return t.status === "connected" ? t : undefined;
  });

  function openHost(node: HostNode, forceNew = false) {
    if (node.type !== "host") return;
    if (!forceNew) {
      const existing = tabs.value.find((t) => t.hostKey === node.key);
      if (existing) {
        activeTabKey.value = existing.key;
        return;
      }
    }
    tabSeq += 1;
    const key = `tab-${tabSeq}`;
    const sameHostCount = tabs.value.filter((t) => t.hostKey === node.key).length;
    const title =
      sameHostCount > 0 ? `${node.label} (${sameHostCount + 1})` : node.label;
    const protocol = node.protocol ?? "ssh";
    const kind = protocol === "telnet" ? "telnet" : protocol === "serial" ? "serial" : "ssh";
    tabs.value.push({
      key,
      title,
      kind,
      hostId: node.id,
      hostKey: node.key,
      icon: node.icon ?? null,
      color: node.color ?? null,
      status: "connecting",
      hostInfo: {
        addr: node.host ?? "",
        port: node.port ?? "22",
        username: node.username ?? "root",
      },
    });
    activeTabKey.value = key;
    hostsOpen.value = false;
  }

  function openLocal(shell?: string) {
    tabSeq += 1;
    const key = `tab-${tabSeq}`;
    const chosen =
      (shell ?? startupStore.defaultShell ?? "auto").trim() || "auto";
    const sameLocalCount = tabs.value.filter((t) => t.kind === "local").length;
    const baseTitle = chosen === "auto" ? "Local" : `Local (${chosen})`;
    const title =
      sameLocalCount > 0 ? `${baseTitle} ${sameLocalCount + 1}` : baseTitle;
    tabs.value.push({
      key,
      title,
      kind: "local",
      shell: chosen,
      status: "connecting",
      icon: null,
      color: null,
    });
    activeTabKey.value = key;
    hostsOpen.value = false;
  }

  function onTabBarNew(kind: "host" | "local") {
    if (kind === "local") openLocal();
    else hostsOpen.value = true;
  }

  function closeTab(key: string) {
    const idx = tabs.value.findIndex((t) => t.key === key);
    if (idx < 0) return;
    const t = tabs.value[idx];
    // 关闭 tab 时主动 kill 对应 sidecar 并清空 AI 会话状态。
    // 不能依赖 onStatusChange：TerminalView 卸载时把 ws.onclose 置空再 close，
    // 不会再 emit status-change，SSH/local 都走不到原来的 killFor 分支。
    if (t?.sid) {
      void aiStore.killFor(t.sid);
    }
    tabs.value.splice(idx, 1);
    if (activeTabKey.value === key) {
      const next = tabs.value[idx] ?? tabs.value[idx - 1];
      activeTabKey.value = next ? next.key : "";
    }
    // 清理广播 store 里残留的引用，避免已关闭 tab 的 key 仍在 targetKeys / sourceKey 中
    broadcastStore.purgeKey(key);
  }

  function reorderTabs(next: TerminalTab[]) {
    tabs.value = next;
  }

  const terminalRefs = new Map<string, TerminalViewExposed>();
  const aiAssistantRef = ref<AiAssistantExposed | null>(null);

  function setTerminalRef(key: string, inst: unknown) {
    if (
      inst &&
      typeof inst === "object" &&
      "disconnect" in inst &&
      "reconnect" in inst &&
      "relayout" in inst &&
      "serializeSession" in inst
    ) {
      terminalRefs.set(key, inst as TerminalViewExposed);
    } else {
      terminalRefs.delete(key);
    }
  }

  function reconnectTab(key: string) {
    const inst = terminalRefs.get(key);
    if (!inst) return;
    // 先清掉旧 sid，等 reconnect 后 sid-ready 帧再写回
    const t = tabs.value.find((x) => x.key === key);
    if (t?.sid) {
      void aiStore.killFor(t.sid);
      t.sid = undefined;
    }
    void inst.reconnect();
  }

  function disconnectTab(key: string) {
    const inst = terminalRefs.get(key);
    if (!inst) return;
    inst.disconnect();
  }

  /** 提供给 TabBar 的回调：取出指定 tab 当前的会话快照（带 ANSI 颜色）。 */
  function getSessionContent(key: string): string | null {
    const inst = terminalRefs.get(key);
    if (!inst) return null;
    try {
      return inst.serializeSession();
    } catch (e) {
      console.error("[ashell] serialize session failed:", e);
      return null;
    }
  }

  function duplicateTab(key: string) {
    const t = tabs.value.find((x) => x.key === key);
    if (!t) return;
    if (t.kind === "local") {
      openLocal(t.shell ?? undefined);
      return;
    }
    if (t.hostId === undefined || t.hostId === null || !t.hostKey) return;
    // 拼一个最小可用的 HostNode 给 openHost(forceNew=true)
    const node: HostNode = {
      type: "host",
      id: t.hostId,
      key: t.hostKey,
      label: t.title.replace(/\s\(\d+\)$/, ""),
      icon: t.icon ?? null,
      color: t.color ?? null,
      host: t.hostInfo?.addr ?? "",
      port: t.hostInfo?.port ?? "22",
      username: t.hostInfo?.username ?? "root",
      protocol: t.kind === "telnet" ? "telnet" : t.kind === "serial" ? "serial" : "ssh",
    };
    openHost(node, true);
  }

  function openInNewWindow(key: string) {
    const t = tabs.value.find((x) => x.key === key);
    if (!t) return;
    void openTabInNewWindow(t);
  }

  function renameTab(key: string, title: string) {
    const t = tabs.value.find((x) => x.key === key);
    if (t) t.title = title;
  }

  function closeOtherTabs(key: string) {
    const keys = tabs.value.filter((t) => t.key !== key).map((t) => t.key);
    for (const k of keys) closeTab(k);
  }

  function closeLeftTabs(key: string) {
    const idx = tabs.value.findIndex((t) => t.key === key);
    if (idx <= 0) return;
    const keys = tabs.value.slice(0, idx).map((t) => t.key);
    for (const k of keys) closeTab(k);
  }

  function closeRightTabs(key: string) {
    const idx = tabs.value.findIndex((t) => t.key === key);
    if (idx < 0) return;
    const keys = tabs.value.slice(idx + 1).map((t) => t.key);
    for (const k of keys) closeTab(k);
  }

  function onSidReady(tabKey: string, sid: string) {
    const t = tabs.value.find((x) => x.key === tabKey);
    if (t) t.sid = sid;
  }

  function onStatusChange(
    tabKey: string,
    status: NonNullable<TerminalTab["status"]>,
  ) {
    const t = tabs.value.find((x) => x.key === tabKey);
    if (!t) return;
    t.status = status;
    // SSH session 断开时 kill 对应 ssid 的 AI sidecar 并清理会话状态
    if ((status === "closed" || status === "error") && t.sid) {
      void aiStore.killFor(t.sid);
    }
    // local 终端断连后无法重连，直接关闭 tab
    if (t.kind === "local" && (status === "closed" || status === "error")) {
      closeTab(tabKey);
    }
  }

  function onTitleChange(tabKey: string, title: string) {
    const t = tabs.value.find((x) => x.key === tabKey);
    if (t) t.title = title;
  }

  function closeHostsIfOpen() {
    if (hostsOpen.value) hostsOpen.value = false;
  }

  // 启动钩子：新窗口通过 URL query string 接收启动参数自动打开 tab；
  // 主窗口走"记住 tab"或"自动开本地终端"。
  // 放在 store 创建之后、其它逻辑之前；不放 onMounted 是为了在首屏渲染前就把 tab 加上。
  const launchParams = new URLSearchParams(window.location.search);
  const isNewWindow = launchParams.get("newwin") === "1";
  if (isNewWindow) {
    const kind = launchParams.get("kind");
    if (kind === "local") {
      const shell = launchParams.get("shell") ?? undefined;
      queueMicrotask(() => {
        if (tabs.value.length === 0) openLocal(shell ?? undefined);
      });
    } else if (kind === "host") {
      const hostIdStr = launchParams.get("hostId");
      const hostId = hostIdStr ? Number(hostIdStr) : NaN;
      if (Number.isFinite(hostId)) {
        queueMicrotask(async () => {
          try {
            await hostStore.refresh();
          } catch {
            // ignore
          }
          const node = findHostNode(hostStore.tree, hostId);
          if (node) openHost(node, true);
        });
      }
    }
  } else if (tabs.value.length === 0 && startupStore.openLocalOnStart) {
    // 用 queueMicrotask 让 openLocal 之前 reactive ref 都建立完成，避免初始化期赋值。
    queueMicrotask(() => {
      if (tabs.value.length === 0) openLocal();
    });
  }

  return {
    hostsOpen,
    tabs,
    activeTabKey,
    activeTab,
    activeSftpTab,
    activeAiTab,
    restoredTabKeys,
    terminalRefs,
    aiAssistantRef,
    setTerminalRef,
    openHost,
    openLocal,
    onTabBarNew,
    closeTab,
    reorderTabs,
    reconnectTab,
    disconnectTab,
    getSessionContent,
    duplicateTab,
    openInNewWindow,
    renameTab,
    closeOtherTabs,
    closeLeftTabs,
    closeRightTabs,
    onSidReady,
    onStatusChange,
    onTitleChange,
    closeHostsIfOpen,
  };
}
