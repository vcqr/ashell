import { computed, ref, watch, type InjectionKey, type ComputedRef } from "vue";
import type { TerminalTab, HostNode } from "@/types";
import { useAiStore } from "@/stores/ai";
import { useBroadcastStore } from "@/stores/broadcast";
import { useStartupStore } from "@/stores/startup";
import { useHostStore } from "@/stores/hosts";
import { isTauri } from "@/utils/platform";
import { useTerminalStore } from "@/stores/terminal";
import { openTabInNewWindow } from "@/utils/newWindow";
import { useHostsPin } from "@/composables/useHostsPin";

/** TerminalView 通过 defineExpose 暴露的实例方法 */
export type TerminalViewExposed = {
  disconnect: () => void;
  reconnect: () => Promise<void>;
  relayout: () => void;
  serializeSession: () => string;
  sendCommand: (cmd: string) => void;
};

/** AiAssistant 通过 defineExpose 暴露的实例方法 */
export type AiAssistantExposed = {
  callStreamingApi: (content: string) => void;
  sendText: (text: string) => Promise<void>;
};

const TABS_KEY = "ashell:tabs";

/** 主机会话状态(取该主机所有 tab 中最活跃的状态),主机树/卡片视图的状态点用 */
export type HostSessionStatus = NonNullable<TerminalTab["status"]>;

/** App 层 provide、HostTree 层 inject 的聚合状态(避免 useTabs 多实例) */
export const HOST_SESSION_STATUS_KEY: InjectionKey<
  ComputedRef<Map<number, HostSessionStatus>>
> = Symbol("host-session-status");

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

/** 分屏方向：h = 左右并排，v = 上下堆叠 */
export type SplitDir = "h" | "v";

/** 窗格下标：双格 0-1，四格 0-3；-1 表示不在任何窗格 */
export type SplitPaneIdx = 0 | 1 | 2 | 3;

/** 每个布局的窗格槽数：双格（左右/上下）与四格（2×2） */
export const SPLIT_SLOT_COUNT = 4 as const;

/** 分屏布局的持久化形态（keys 里失效的 key 在恢复时过滤；旧数据只有 2 个槽位） */
interface PersistedSplit {
  dir: SplitDir;
  ratio: number;
  ratio2: number;
  grid: boolean;
  keys: (string | null)[];
  focused: number;
}

interface PersistedTabs {
  tabs: PersistedTab[];
  activeKey: string;
  split?: PersistedSplit | null;
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
    const tabs = parsed.tabs.filter(
      (t): t is PersistedTab =>
        !!t && typeof t.key === "string" && typeof t.title === "string",
    );
    // 分屏布局：槽位 key 必须仍指向存在的 tab，否则降级为不分屏
    let split: PersistedSplit | null = null;
    if (
      parsed.split &&
      (parsed.split.dir === "h" || parsed.split.dir === "v") &&
      Array.isArray(parsed.split.keys) &&
      parsed.split.keys.length >= 2
    ) {
      const alive = new Set(tabs.map((t) => t.key));
      const keys = parsed.split.keys.map((k) =>
        typeof k === "string" && alive.has(k) ? k : null,
      );
      if (keys.some((k) => k)) {
        const clamp = (v: number) => Math.min(0.85, Math.max(0.15, v));
        const ratio = Number(parsed.split.ratio);
        const ratio2 = Number(parsed.split.ratio2);
        const grid = parsed.split.grid === true;
        const focused = Math.min(
          grid ? 3 : 1,
          Math.max(0, Math.floor(Number(parsed.split.focused) || 0)),
        );
        // 不足 4 槽的旧数据补空槽；非 grid 布局忽略多余槽位
        const slots: (string | null)[] = [];
        for (let i = 0; i < SPLIT_SLOT_COUNT; i++) {
          slots.push(grid || i < 2 ? (keys[i] ?? null) : null);
        }
        split = {
          dir: parsed.split.dir,
          ratio: Number.isFinite(ratio) ? clamp(ratio) : 0.5,
          ratio2: Number.isFinite(ratio2) ? clamp(ratio2) : 0.5,
          grid,
          keys: slots,
          focused,
        };
      }
    }
    return {
      tabs,
      activeKey: typeof parsed.activeKey === "string" ? parsed.activeKey : "",
      split,
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
  const terminalStore = useTerminalStore();

  const hostsOpen = ref(false);
  const hostsPinned = useHostsPin();

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

  // ===== 分屏（split view）=====
  // 窗格是"槽位"，各自持有 tab key；焦点窗格决定 tab 条高亮 / 键盘焦点 / 广播源。
  // 不变量：splitEnabled 时 splitPaneKeys[focusedPaneIdx] === activeTabKey
  // （焦点窗格为空占位时 activeTabKey 为 ""）。单窗格模式完全不读这些状态，
  // 行为与历史版本一致。
  const splitEnabled = ref(false);
  const splitDir = ref<SplitDir>("h");
  /** 是否四分屏（2×2）：false 时按 splitDir 取左右/上下双格 */
  const splitGrid = ref(false);
  /** 双格分隔条位置；四分屏下是竖分隔条位置（左右占比） */
  const splitRatio = ref(0.5);
  /** 仅四分屏使用：横分隔条位置（上下占比） */
  const splitRatio2 = ref(0.5);
  const splitPaneKeys = ref<(string | null)[]>(
    Array<string | null>(SPLIT_SLOT_COUNT).fill(null),
  );
  const focusedPaneIdx = ref<SplitPaneIdx>(0);

  /** 当前布局的窗格数：双格 2，四格 4 */
  const splitSlotCount = computed(() => (splitGrid.value ? 4 : 2));

  // 恢复持久化的分屏布局（keys 已在 loadPersistedTabs 里按存活 tab 过滤）
  if (persisted.split) {
    splitDir.value = persisted.split.dir;
    splitRatio.value = persisted.split.ratio;
    splitRatio2.value = persisted.split.ratio2;
    splitGrid.value = persisted.split.grid;
    splitPaneKeys.value = persisted.split.keys.slice(0, SPLIT_SLOT_COUNT);
    focusedPaneIdx.value = persisted.split.focused as SplitPaneIdx;
    splitEnabled.value = true;
    // 保证焦点窗格槽位与 activeTabKey 一致（持久化数据异常时兜底）
    if (splitPaneKeys.value[focusedPaneIdx.value] !== activeTabKey.value) {
      activeTabKey.value = splitPaneKeys.value[focusedPaneIdx.value] ?? "";
    }
  }

  /** 切换焦点窗格（点击某窗格区域 / 快捷键循环时调用）：tab 高亮与广播输入源跟随。 */
  function focusPane(idx: SplitPaneIdx) {
    if (!splitEnabled.value || focusedPaneIdx.value === idx) return;
    if (idx >= splitSlotCount.value) return;
    focusedPaneIdx.value = idx;
    activeTabKey.value = splitPaneKeys.value[idx] ?? "";
  }

  /** 焦点窗格循环切换（快捷键 view.focusNextPane）：占位窗格也可聚焦以便填入会话。 */
  function focusNextPane() {
    if (!splitEnabled.value) return;
    focusPane(((focusedPaneIdx.value + 1) % splitSlotCount.value) as SplitPaneIdx);
  }

  /**
   * 激活某个 tab —— activeTabKey 的统一写入口。
   * 分屏时若该 tab 已显示在其他窗格，只切换焦点窗格、不搬运会话；
   * 否则放入当前焦点窗格（新会话也在焦点窗格打开）。
   */
  function activateTab(key: string) {
    if (splitEnabled.value) {
      const existingIdx = key
        ? splitPaneKeys.value.findIndex((k, i) => k === key && i !== focusedPaneIdx.value)
        : -1;
      if (existingIdx >= 0) {
        focusedPaneIdx.value = existingIdx as SplitPaneIdx;
        activeTabKey.value = key;
        return;
      }
      splitPaneKeys.value[focusedPaneIdx.value] = key || null;
      autoConnectForSplit(key);
    }
    activeTabKey.value = key;
  }

  /**
   * 关闭分屏，回到单窗格；其余会话都保留（只是隐藏为普通后台 tab）。
   * 若焦点格是空占位或焦点会话已不存在，则保住唯一有会话的窗格。
   */
  function disableSplit() {
    if (!splitEnabled.value) return;
    let keep = activeTabKey.value;
    if (!keep || !tabs.value.some((t) => t.key === keep)) {
      keep =
        splitPaneKeys.value.find(
          (k) => k && tabs.value.some((t) => t.key === k),
        ) ?? "";
    }
    splitEnabled.value = false;
    splitGrid.value = false;
    splitPaneKeys.value = [keep || null, null, null, null];
    focusedPaneIdx.value = 0;
    activeTabKey.value = keep;
  }

  /**
   * 分屏把会话摆进可见窗格时的自动连接：断开 / 出错的会话直接走重连。
   * 分屏是显式的"现在要看这个会话"动作，与打开主机、复制连接同类，
   * 不受「启动时自动连接记住的标签页」偏好门控（那只约束启动恢复）。
   * reconnect 保留 xterm 缓冲区，手动断开的会话被摆入时重连也不丢滚动历史。
   * 仅在摆入时机触发：焦点切换 / 切换方向不会偷偷建连。
   */
  function autoConnectForSplit(key: string | null) {
    if (!key) return;
    const t = tabs.value.find((x) => x.key === key);
    if (!t) return;
    const st = t.status ?? "closed";
    if (st === "connecting" || st === "connected") return;
    reconnectTab(key);
  }

  /** 开启双格分屏（副格尽量放 tabKey，否则取下一个 tab，再没有则空窗格）。 */
  function enableSplit(dir: SplitDir, tabKey?: string) {
    const current = activeTabKey.value;
    let second = tabKey && tabKey !== current ? tabKey : null;
    if (!second) {
      second = tabs.value.find((t) => t.key !== current)?.key ?? null;
    }
    splitGrid.value = false;
    splitDir.value = dir;
    splitPaneKeys.value = [current || null, second, null, null];
    focusedPaneIdx.value = 0;
    splitEnabled.value = true;
    // 开启分屏时两个窗格里的断开会话都自动重连
    autoConnectForSplit(current || null);
    autoConnectForSplit(second);
  }

  /** 开启四分屏（2×2）：当前会话进左上，右键指定的 tab 优先进右上，其余槽位按 tab 顺序填充。 */
  function splitGridTab(tabKey?: string) {
    const used = new Set<string>();
    if (!splitEnabled.value) {
      const current = activeTabKey.value;
      if (current) used.add(current);
      const slots: (string | null)[] = [current || null];
      if (tabKey && tabKey !== current && tabs.value.some((t) => t.key === tabKey)) {
        used.add(tabKey);
        slots.push(tabKey);
      }
      for (let i = slots.length; i < SPLIT_SLOT_COUNT; i++) {
        const next = tabs.value.find((t) => !used.has(t.key))?.key ?? null;
        if (next) used.add(next);
        slots.push(next);
      }
      splitGrid.value = true;
      splitPaneKeys.value = slots;
      focusedPaneIdx.value = 0;
      splitEnabled.value = true;
      for (const k of slots) autoConnectForSplit(k);
      return;
    }
    if (!splitGrid.value) {
      // 双格 → 四格：保留现有两格，空槽用隐藏 tab 补位
      for (const k of splitPaneKeys.value) if (k) used.add(k);
      for (let i = 0; i < SPLIT_SLOT_COUNT; i++) {
        if (splitPaneKeys.value[i]) continue;
        const next = tabs.value.find((t) => !used.has(t.key))?.key ?? null;
        if (next) {
          used.add(next);
          splitPaneKeys.value[i] = next;
          autoConnectForSplit(next);
        }
      }
      splitGrid.value = true;
    }
    // 已是四分屏：无操作
  }

  /**
   * 分屏入口（右键菜单 / 快捷键）：
   * - 未分屏：开启双格分屏，tabKey 尽量放进副格；
   * - 双格分屏：调整方向；tabKey 未显示时顺手放进非焦点窗格；
   * - 四分屏：收敛为指定方向的双格（保留焦点格与下一个有会话窗格）。
   */
  function splitTab(dir: SplitDir, tabKey?: string) {
    if (!splitEnabled.value) {
      enableSplit(dir, tabKey);
      return;
    }
    if (splitGrid.value) {
      const focusedKey = splitPaneKeys.value[focusedPaneIdx.value];
      const nextKey =
        splitPaneKeys.value.find(
          (k, i) => i !== focusedPaneIdx.value && k && tabs.value.some((t) => t.key === k),
        ) ?? null;
      const keep = focusedKey || nextKey;
      splitGrid.value = false;
      splitDir.value = dir;
      splitPaneKeys.value = [keep || null, keep === focusedKey ? nextKey : null, null, null];
      focusedPaneIdx.value = 0;
      if (keep) activeTabKey.value = keep;
      return;
    }
    splitDir.value = dir;
    if (tabKey) {
      const other = (1 - focusedPaneIdx.value) as SplitPaneIdx;
      const inFocused = splitPaneKeys.value[focusedPaneIdx.value] === tabKey;
      const inOther = splitPaneKeys.value[other] === tabKey;
      if (!inFocused && !inOther) {
        splitPaneKeys.value[other] = tabKey;
        autoConnectForSplit(tabKey);
      }
    }
  }

  /** 分屏槽位里指向已关闭 tab 的 key 置空（closeTab 里调用）。 */
  function sanitizeSplitSlots() {
    if (!splitEnabled.value) return;
    const alive = new Set(tabs.value.map((t) => t.key));
    for (let i = 0; i < SPLIT_SLOT_COUNT; i++) {
      const k = splitPaneKeys.value[i];
      if (k && !alive.has(k)) splitPaneKeys.value[i] = null;
    }
  }

  /** 分屏下仍有会话的窗格数（closeTab 后判断是否收缩为单格）。 */
  function occupiedSlotCount(): number {
    return splitPaneKeys.value.filter(
      (k) => k && tabs.value.some((t) => t.key === k),
    ).length;
  }

  /**
   * 主机会话状态聚合:hostId → 该主机所有 tab 中最活跃的状态。
   * 优先级 connected > connecting > error > closed——有多开会话时,
   * 只要有一个活连接就亮绿点;全部断开才是灰点。本地终端 tab 无 hostId 不参与。
   */
  const hostSessionStatus = computed<Map<number, HostSessionStatus>>(() => {
    const rank: Record<HostSessionStatus, number> = {
      connected: 3,
      connecting: 2,
      error: 1,
      closed: 0,
    };
    const map = new Map<number, HostSessionStatus>();
    for (const t of tabs.value) {
      if (t.hostId == null) continue;
      const s: HostSessionStatus = t.status ?? "connected";
      const prev = map.get(t.hostId);
      if (prev === undefined || rank[s] > rank[prev]) map.set(t.hostId, s);
    }
    return map;
  });

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
          split: splitEnabled.value
            ? {
                dir: splitDir.value,
                ratio: splitRatio.value,
                ratio2: splitRatio2.value,
                grid: splitGrid.value,
                keys: [...splitPaneKeys.value],
                focused: focusedPaneIdx.value,
              }
            : null,
        };
        localStorage.setItem(TABS_KEY, JSON.stringify(snapshot));
      } catch {
        // 配额满 / 隐私模式禁用 — 静默忽略，下次还会再尝试
      }
    }, 200);
  }

  watch(
    [
      tabs,
      activeTabKey,
      splitEnabled,
      splitDir,
      splitGrid,
      splitRatio,
      splitRatio2,
      splitPaneKeys,
      focusedPaneIdx,
    ],
    () => persistTabs(),
    { deep: true },
  );

  // tabs 变化时把本窗口 tab 列表广播给其他窗口（跨窗口广播功能依赖此目录）
  watch(
    tabs,
    (newTabs) => {
      broadcastStore.announceTabs(
        newTabs.map((t) => ({
          key: t.key,
          title: t.title,
          kind: t.kind ?? "ssh",
          status: t.status,
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

  /**
   * 任意已连接的终端 tab（SSH / 本地 / Telnet / 串口）。
   * 供模板命令等不依赖 SSH 会话的功能使用。
   */
  const activeTerminalTab = computed<TerminalTab | undefined>(() => {
    const t = activeTab.value;
    if (!t || !t.sid) return undefined;
    return t.status === "connected" ? t : undefined;
  });

  /**
   * 当前激活的本地 PTY tab：本地文件浏览器抽屉的挂载条件。
   * 不要求 sid / connected——文件浏览器走独立 HTTP API，只认 tab 类型。
   */
  const activeLocalTab = computed<TerminalTab | undefined>(() => {
    const t = activeTab.value;
    return t && t.kind === "local" ? t : undefined;
  });

  /** 是否还存在本地 PTY tab（不要求激活）：本地文件面板的销毁条件 */
  const hasLocalTab = computed(() => tabs.value.some((t) => t.kind === "local"));

  function openHost(node: HostNode, forceNew = false) {
    if (node.type !== "host") return;
    if (!forceNew) {
      const existing = tabs.value.find((t) => t.hostKey === node.key);
      if (existing) {
        activateTab(existing.key);
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
    activateTab(key);
    if (!hostsPinned.value) hostsOpen.value = false;
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
    activateTab(key);
    if (!hostsPinned.value) hostsOpen.value = false;
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
    sanitizeSplitSlots();
    if (activeTabKey.value === key) {
      const next = tabs.value[idx] ?? tabs.value[idx - 1];
      activateTab(next ? next.key : "");
    }
    // 只剩一个有会话的窗格（或全部清空）时收起分屏，避免长期留着单格分屏
    if (splitEnabled.value && occupiedSlotCount() <= 1) {
      disableSplit();
    }
    if (tabs.value.length === 0) {
      splitEnabled.value = false;
      splitGrid.value = false;
      splitPaneKeys.value = [null, null, null, null];
      focusedPaneIdx.value = 0;
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

  /** 向当前激活的终端发送一条命令并执行 */
  function sendCommandToActive(cmd: string) {
    if (!activeTabKey.value) return;
    const inst = terminalRefs.get(activeTabKey.value);
    if (!inst) return;
    inst.sendCommand(cmd);
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
      return;
    }
    // 远程终端断开时根据用户设置执行退出策略
    if (status === "closed" || status === "error") {
      const action = terminalStore.disconnectAction;
      if (action === "closeTab" || action === "closeWindow") {
        closeTab(tabKey);
        if (action === "closeWindow" && tabs.value.length === 0) {
          if (isTauri) {
            void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
              getCurrentWindow().close(),
            );
          } else {
            window.close();
          }
        }
      }
    }
  }

  function onTitleChange(tabKey: string, title: string) {
    const t = tabs.value.find((x) => x.key === tabKey);
    if (t) t.title = title;
  }

  /**
   * shell 上报 cwd（OSC 9;9 / OSC 7）：本地 tab 由后端按 shell 注入上报，
   * 远程 tab 依赖用户自配的 shell 集成。记录到所属 tab，
   * 驱动本地文件抽屉与 SFTP 远程栏的目录跟随。
   */
  function onCwdChange(tabKey: string, cwd: string) {
    const t = tabs.value.find((x) => x.key === tabKey);
    if (t) t.cwd = cwd;
  }

  function closeHostsIfOpen() {
    // 固定模式下主界面已让位并排显示，点主界面不应收起
    if (hostsPinned.value) return;
    if (hostsOpen.value) hostsOpen.value = false;
  }

  function toggleHosts() {
    hostsOpen.value = !hostsOpen.value;
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
    hostSessionStatus,
    activeTabKey,
    activeTab,
    activeSftpTab,
    activeAiTab,
    activeTerminalTab,
    activeLocalTab,
    hasLocalTab,
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
    onCwdChange,
    closeHostsIfOpen,
    toggleHosts,
    sendCommandToActive,
    splitEnabled,
    splitDir,
    splitGrid,
    splitRatio,
    splitRatio2,
    splitPaneKeys,
    focusedPaneIdx,
    activateTab,
    focusPane,
    focusNextPane,
    disableSplit,
    splitTab,
    splitGridTab,
  };
}
