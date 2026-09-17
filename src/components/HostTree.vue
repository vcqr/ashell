<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  NTree,
  NIcon,
  NButton,
  NInput,
  NDropdown,
  NModal,
  NCard,
  NSpace,
  NEmpty,
  NTooltip,
  NTreeSelect,
  NForm,
  NFormItem,
  useMessage,
  useDialog,
  type TreeOption,
  type TreeSelectOption,
  type DropdownOption,
} from "naive-ui"
import {
  TerminalOutline,
  AddOutline,
  RefreshOutline,
  CreateOutline,
  TrashOutline,
  SearchOutline,
  CloseOutline,
  CopyOutline,
  ServerOutline,
  DownloadOutline,
  AddCircleOutline,
  ListOutline,
  ExpandOutline,
  ContractOutline,
  LinkOutline,
  SwapVerticalOutline,
  CheckmarkOutline,
} from "@vicons/ionicons5"
import { Folder, FolderOpen } from "@vicons/fa"
import { FolderAddOutlined, PushpinFilled, PushpinOutlined } from "@vicons/antd"
import { useI18n } from "vue-i18n"
import { useHostStore } from "@/stores/hosts"
import { useIconStore } from "@/stores/icons"
import { useHostsPin } from "@/composables/useHostsPin"
import { copyText } from "@/utils/clipboard"
import { compareAddr } from "@/stores/hosts"
import SshConfigImportModal from "@/components/SshConfigImportModal.vue"
import type { HostNode, Host } from "@/types"

const emit = defineEmits<{
  close: []
  "open-host": [node: HostNode, forceNew?: boolean]
  "create-host": [parentGid: number]
  "edit-host": [host: Host]
}>()

const { t } = useI18n()
const store = useHostStore()
const iconStore = useIconStore()
const message = useMessage()
const dialog = useDialog()

const filter = ref("")
const selectedKeys = ref<string[]>([])
const expandedKeys = ref<string[]>([])
const importModalShow = ref(false)
const hostsPinned = useHostsPin()
const searchInputRef = ref<InstanceType<typeof NInput> | null>(null)

onMounted(() => {
  void iconStore.ensureLoaded()
})

const treeData = computed<TreeOption[]>(
  () => store.tree as unknown as TreeOption[],
)

/* ---------- 平铺视图：忽略目录，按树序列出所有主机 ---------- */
const FLAT_KEY = "ashell:hosts-flat"

const flatMode = ref(
  typeof localStorage !== "undefined" && localStorage.getItem(FLAT_KEY) === "true",
)

watch(flatMode, (v) => {
  try {
    localStorage.setItem(FLAT_KEY, String(v))
  } catch {
    // ignore
  }
})

function collectHosts(list: HostNode[], out: HostNode[] = []): HostNode[] {
  for (const n of list) {
    if (n.type === "host") out.push(n)
    if (n.children) collectHosts(n.children, out)
  }
  return out
}

const flatTreeData = computed<TreeOption[]>(() => {
  const hosts = collectHosts(store.tree)
  // 平铺是单一列表：置顶主机全局置顶（树形视图则是组内置顶），其余按排序模式
  const pinned = hosts.filter((h) => store.isHostPinned(h.id))
  const rest = hosts.filter((h) => !store.isHostPinned(h.id))
  const cmp =
    store.hostSortMode === "addr"
      ? (a: HostNode, b: HostNode) => compareAddr(a.host ?? "", b.host ?? "")
      : (a: HostNode, b: HostNode) => a.label.localeCompare(b.label)
  return [...pinned.sort(cmp), ...rest.sort(cmp)] as unknown as TreeOption[]
})
const displayData = computed<TreeOption[]>(() =>
  flatMode.value ? flatTreeData.value : treeData.value,
)

/* ---------- 目录内主机总数（含子目录，tooltip 用） ---------- */
const hostCountInFolder = computed(() => {
  const map = new Map<string, number>()
  const walk = (list: HostNode[]): number => {
    let count = 0
    for (const n of list) {
      if (n.type === "host") count++
      else if (n.children) {
        const sub = walk(n.children)
        map.set(n.key, sub)
        count += sub
      }
    }
    return count
  }
  walk(store.tree)
  return map
})

/* ---------- 展开 / 收起所有目录 ---------- */
function collectFolderKeys(list: HostNode[], out: string[] = []): string[] {
  for (const n of list) {
    if (n.type === "folder") {
      out.push(n.key)
      if (n.children) collectFolderKeys(n.children, out)
    }
  }
  return out
}

const allExpanded = computed(() => {
  const folderKeys = collectFolderKeys(store.tree)
  if (folderKeys.length === 0) return false
  return folderKeys.every((k) => expandedKeys.value.includes(k))
})

// 搜索口径：名称 + 主机地址（大小写不敏感）
function nodeMatchesPattern(node: HostNode, q: string): boolean {
  if (!q) return true
  if (node.label.toLowerCase().includes(q)) return true
  return node.type === "host" && !!node.host && node.host.toLowerCase().includes(q)
}

/* ---------- 数据层过滤：先在数据里筛出命中项,再把新列表交给 NTree 渲染。
   不用 NTree 的 pattern/filter/show-irrelevant-nodes——避免其内部过滤
   与行组件更新在同一次 flush 里互相踩踏（2.45 崩溃根因） ---------- */
const isFiltering = computed(() => filter.value.trim().length > 0)

/** 剪枝：保留命中节点与其祖先路径；命中目录若无匹配子孙则保留为叶子。无命中返回 null */
function pruneToMatches(list: HostNode[], q: string): HostNode[] | null {
  const out: HostNode[] = []
  for (const n of list) {
    if (n.type === "folder" && n.children) {
      const kids = pruneToMatches(n.children, q)
      if (kids) {
        out.push({ ...n, children: kids })
        continue
      }
    }
    if (nodeMatchesPattern(n, q)) {
      out.push(n.type === "folder" ? { ...n, children: [] } : n)
    }
  }
  return out.length > 0 ? out : null
}

const filteredDisplayData = computed<TreeOption[]>(() => {
  const q = filter.value.trim().toLowerCase()
  if (!q) return displayData.value
  if (flatMode.value) {
    return (displayData.value as unknown as HostNode[]).filter((n) =>
      nodeMatchesPattern(n, q),
    ) as unknown as TreeOption[]
  }
  return (pruneToMatches(store.tree, q) ?? []) as unknown as TreeOption[]
})

// 过滤态下命中路径全部展开,展开键由过滤结果决定,不读写 expandedKeys
const visibleExpandedKeys = computed<string[]>(() => {
  if (!isFiltering.value) return expandedKeys.value
  const keys: string[] = []
  const walk = (list: HostNode[]) => {
    for (const n of list) {
      if (n.type !== "folder") continue
      keys.push(n.key)
      if (n.children) walk(n.children)
    }
  }
  walk(filteredDisplayData.value as unknown as HostNode[])
  return keys
})

function onUpdateExpandedKeys(keys: string[]) {
  if (isFiltering.value) return
  expandedKeys.value = keys
}

function toggleAllExpand() {
  expandedKeys.value = allExpanded.value ? [] : collectFolderKeys(store.tree)
}

/* ---------- 仅含目录（folder）的列表，供 NTreeSelect 选父级 ---------- */
function buildFolderOptions(list: HostNode[]): TreeSelectOption[] {
  const out: TreeSelectOption[] = []
  for (const n of list) {
    if (n.type !== "folder") continue
    const children = n.children ? buildFolderOptions(n.children) : []
    const opt: TreeSelectOption = { key: n.id, label: n.label }
    if (children.length > 0) opt.children = children
    out.push(opt)
  }
  return out
}

const folderSelectOptions = computed<TreeSelectOption[]>(() => [
  { key: 0, label: t("common.rootDir"), children: buildFolderOptions(store.tree) },
])

function renderPrefix({ option }: { option: TreeOption }) {
  const node = option as unknown as HostNode
  if (node.type === "folder") {
    // 过滤态下命中路径全展开,目录图标按展开绘制
    const expanded = isFiltering.value || expandedKeys.value.includes(node.key)
    return h(
      NIcon,
      { color: "#f1c27d", size: 16 },
      { default: () => h(expanded ? FolderOpen : Folder) },
    )
  }
  const iconUrl = node.icon ? iconStore.urlOf(node.icon) : null
  if (iconUrl) {
    return h("img", {
      src: iconUrl,
      width: 16,
      height: 16,
      style: {
        borderRadius: "3px",
        objectFit: "contain",
        verticalAlign: "middle",
      },
    })
  }
  const color = node.color ?? "#7c5cff"
  return h(
    NIcon,
    { color, size: 16 },
    { default: () => h(TerminalOutline) },
  )
}

/** 悬停节点行末右对齐的元信息：主机=地址（端口非 22 时附带），目录=主机数 */
function suffixTextOfNode(node: HostNode): string {
  if (node.type === "folder") {
    const count = hostCountInFolder.value.get(node.key) ?? 0
    return count > 0 ? t("hosts.tree.hostCountShort", { count }) : ""
  }
  const addr = node.host ?? ""
  if (!addr) return ""
  if (node.protocol === "serial") return addr
  const port = node.port ?? "22"
  return port !== "22" ? `${addr}:${port}` : addr
}

function renderSuffix({ option }: { option: TreeOption }) {
  const node = option as unknown as HostNode
  const pinned = node.type === "host" && store.isHostPinned(node.id)
  const text = suffixTextOfNode(node)
  if (!pinned && !text) return null
  // 常驻渲染 + 0 宽度收起，悬停（CSS :hover）时宽度动画展开，名字被平滑推开；
  // 置顶图钉在折叠网格之外，不受 hover 门控
  return h("span", { class: "node-suffix-wrap" }, [
    pinned
      ? h(
          NIcon,
          { class: "node-pin", size: 12, color: "#7c5cff" },
          { default: () => h(PushpinFilled) },
        )
      : null,
    text
      ? h("span", { class: "node-suffix" }, [
          h("span", { class: "node-suffix-text" }, text),
        ])
      : null,
  ])
}

/** 按当前搜索词切分 label,命中的子串单独成段(大小写不敏感,与 NTree pattern 同口径) */
function splitHighlightSegments(label: string): Array<{ text: string; hit: boolean }> {
  const q = filter.value.toLowerCase()
  if (!q) return [{ text: label, hit: false }]
  const lower = label.toLowerCase()
  const segs: Array<{ text: string; hit: boolean }> = []
  let i = 0
  while (i < label.length) {
    const idx = lower.indexOf(q, i)
    if (idx === -1) {
      segs.push({ text: label.slice(i), hit: false })
      break
    }
    if (idx > i) segs.push({ text: label.slice(i, idx), hit: false })
    segs.push({ text: label.slice(idx, idx + q.length), hit: true })
    i = idx + q.length
  }
  return segs.length > 0 ? segs : [{ text: label, hit: false }]
}

function renderLabel({ option }: { option: TreeOption }) {
  const node = option as unknown as HostNode
  const label = String(node.label ?? "")
  const segments = splitHighlightSegments(label)
  if (!segments.some((s) => s.hit)) return label
  return h(
    "span",
    { class: "node-label" },
    segments.map((s) => (s.hit ? h("span", { class: "node-label-hit" }, s.text) : s.text)),
  )
}

function findParentAndIndex(
  list: HostNode[],
  key: string,
  parent: HostNode | null = null,
): { parent: HostNode | null; list: HostNode[]; index: number } | null {
  for (let i = 0; i < list.length; i++) {
    const item = list[i]!
    if (item.key === key) return { parent, list, index: i }
    if (item.children) {
      const found = findParentAndIndex(item.children, key, item)
      if (found) return found
    }
  }
  return null
}

function findNode(key: string): HostNode | null {
  const found = findParentAndIndex(store.tree, key)
  return found ? found.list[found.index]! : null
}

function collectAncestorKeys(key: string): string[] {
  const keys: string[] = []
  function walk(list: HostNode[], trail: string[]): boolean {
    for (const item of list) {
      if (item.key === key) {
        keys.push(...trail)
        return true
      }
      if (item.children && walk(item.children, [...trail, item.key])) return true
    }
    return false
  }
  walk(store.tree, [])
  return keys
}

function resolveSelectedFolderGid(): number {
  const k = selectedKeys.value[0]
  if (!k) return 0
  const node = findNode(k)
  if (!node) return 0
  if (node.type === "folder") return node.id
  const found = findParentAndIndex(store.tree, k)
  if (found?.parent && found.parent.type === "folder") return found.parent.id
  return 0
}

const ctxMenuShow = ref(false)
const ctxMenuX = ref(0)
const ctxMenuY = ref(0)
const ctxMenuKey = ref<string | null>(null)

/** 在指定坐标弹出上下文菜单（key=null 表示空白处根级菜单） */
function showCtxMenu(key: string | null, x: number, y: number) {
  ctxMenuKey.value = key
  ctxMenuX.value = x
  ctxMenuY.value = y
  // 关-开一帧，让 NDropdown 在坐标变化时重新定位
  ctxMenuShow.value = false
  requestAnimationFrame(() => (ctxMenuShow.value = true))
}

/** 空白处右键：弹出根级操作菜单（新建连接 / 新建目录 / 导入 / 刷新） */
function onBlankContextMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  showCtxMenu(null, e.clientX, e.clientY)
}

/* ---------- 拖拽（pointer events，wry 稳定） ---------- */
const DRAG_THRESHOLD = 5

interface DragState {
  hostNode: HostNode
  startX: number
  startY: number
  active: boolean
}

let drag: DragState | null = null
const dragGhost = ref<{ x: number; y: number; label: string } | null>(null)
const dropTargetKey = ref<string | null>(null) // "folder-<id>" | "__root__" | null

const treeBodyEl = ref<HTMLElement | null>(null)

function beginHostDrag(e: PointerEvent, node: HostNode) {
  drag = {
    hostNode: node,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener("pointermove", onDragMove)
  window.addEventListener("pointerup", onDragEnd)
  window.addEventListener("pointercancel", onDragEnd)
}

function onDragMove(e: PointerEvent) {
  if (!drag) return
  if (!drag.active) {
    const dx = e.clientX - drag.startX
    const dy = e.clientY - drag.startY
    if (Math.hypot(dx, dy) < DRAG_THRESHOLD) return
    drag.active = true
    // 选中压制走 body class + 全局 !important 规则（见 main.css）：
    // user-select 不继承，直接设 body.style 会被面板的 text 声明屏蔽
    document.body.classList.add("ashell-dragging")
    document.body.style.cursor = "grabbing"
  }
  dragGhost.value = {
    x: e.clientX,
    y: e.clientY,
    label: drag.hostNode.label,
  }
  dropTargetKey.value = findFolderKeyAtPoint(e.clientX, e.clientY)
}

function onDragEnd(_e: PointerEvent) {
  if (!drag) return
  const wasActive = drag.active
  const node = drag.hostNode
  const target = dropTargetKey.value
  cleanupDrag()
  if (!wasActive) return
  if (!target) return

  const newGid =
    target === "__root__" ? 0 : Number(target.replace(/^folder-/, ""))
  if (!Number.isFinite(newGid)) return
  if (node.type !== "host") return

  const host = store.findHost(node.id)
  if (!host) return
  if (host.gid === newGid) return

  // 抑制 pointerup 后的 click（避免触发 NTree 的选中/展开）
  const suppress = (ev: MouseEvent) => {
    ev.preventDefault()
    ev.stopPropagation()
  }
  window.addEventListener("click", suppress, { capture: true, once: true })
  setTimeout(() => {
    window.removeEventListener("click", suppress, { capture: true } as never)
  }, 50)

  void moveHostToGid(host, newGid)
}

function cleanupDrag() {
  drag = null
  dragGhost.value = null
  dropTargetKey.value = null
  document.body.classList.remove("ashell-dragging")
  document.body.style.cursor = ""
  window.removeEventListener("pointermove", onDragMove)
  window.removeEventListener("pointerup", onDragEnd)
  window.removeEventListener("pointercancel", onDragEnd)
}

onBeforeUnmount(cleanupDrag)

/** 反查光标下的 folder 节点 key；命中 host 时落到其父；空白处落到根 */
function findFolderKeyAtPoint(x: number, y: number): string | null {
  const root = treeBodyEl.value
  if (!root) return null
  const rect = root.getBoundingClientRect()
  if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) {
    return null
  }
  const els = document.elementsFromPoint(x, y)
  for (const el of els) {
    if (!(el instanceof HTMLElement)) continue
    if (!root.contains(el)) continue
    const carrier = el.closest<HTMLElement>("[data-node-key]")
    if (!carrier || !root.contains(carrier)) continue
    const key = carrier.dataset.nodeKey
    if (!key) continue
    const node = findNode(key)
    if (!node) continue
    if (node.type === "folder") return key
    const found = findParentAndIndex(store.tree, key)
    if (found?.parent && found.parent.type === "folder") return found.parent.key
    return "__root__"
  }
  return "__root__"
}

async function moveHostToGid(host: Host, newGid: number) {
  try {
    // update 对 icon/desc/私钥路径/keepalive 等可空字段是"直接覆盖"语义，
    // 只传 gid 会把它们清成 NULL——必须带全当前值（读改写）
    await store.editHost(host.id, {
      gid: newGid,
      icon: host.icon ?? null,
      color: host.color ?? null,
      desc: host.desc ?? null,
      private_key_path: host.private_key_path ?? null,
      protocol: host.protocol,
      baud_rate: host.baud_rate ?? null,
      data_bits: host.data_bits ?? null,
      stop_bits: host.stop_bits ?? null,
      parity: host.parity ?? null,
      flow_control: host.flow_control ?? null,
      keepalive_interval: host.keepalive_interval ?? null,
      inactivity_timeout: host.inactivity_timeout ?? null,
      idle_send_interval: host.idle_send_interval ?? null,
    })
    const folderName =
      newGid === 0 ? t("hosts.message.rootDir") : (store.findGroup(newGid)?.name ?? `#${newGid}`)
    message.success(t("hosts.message.moved", { name: host.name, target: folderName }))
    await nextTick()
    revealKey(`host-${host.id}`)
  } catch (e) {
    message.error(t("hosts.message.moveFailed", { error: String(e) }))
  }
}

function nodeProps({ option }: { option: TreeOption }) {
  const key = option.key as string
  const isDrop = dropTargetKey.value === key
  return {
    "data-node-key": key,
    "data-drop-active": isDrop ? "true" : "false",
    onContextmenu(e: MouseEvent) {
      e.preventDefault()
      e.stopPropagation()
      showCtxMenu(key, e.clientX, e.clientY)
    },
    onDblclick() {
      const node = findNode(key)
      if (!node) return
      // 双击 = 打开（已有会话则聚焦）；强制新会话降级为中键 / 右键"新建会话"
      if (node.type === "host") emit("open-host", node)
      // 目录不处理：expand-on-click 下双击的两次单击已自行抵消
    },
    onAuxclick(e: MouseEvent) {
      if (e.button !== 1) return
      const node = findNode(key)
      if (node?.type === "host") {
        e.preventDefault()
        emit("open-host", node, true)
      }
    },
    onMousedown(e: MouseEvent) {
      // 吞掉中键默认行为（自动滚动 / Linux 中键粘贴）
      if (e.button === 1) e.preventDefault()
    },
    onPointerdown(e: PointerEvent) {
      if (e.button !== 0) return
      // 平铺视图没有可见目录，落点语义不成立，禁用拖拽归组
      if (flatMode.value) return
      const node = findNode(key)
      if (!node || node.type !== "host") return
      beginHostDrag(e, node)
    },
  }
}

function toggleExpand(key: string) {
  const set = new Set(expandedKeys.value)
  if (set.has(key)) set.delete(key)
  else set.add(key)
  expandedKeys.value = Array.from(set)
}

/* ---------- 键盘操作 ---------- */
function focusSearch() {
  searchInputRef.value?.focus()
}

/** 面板内任意位置：Ctrl/Cmd+F 或 / 聚焦搜索 */
function onRootKeydown(e: KeyboardEvent) {
  if (e.isComposing) return
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "f") {
    e.preventDefault()
    focusSearch()
    return
  }
  if (e.key === "/" && !e.ctrlKey && !e.metaKey && !e.altKey) {
    const el = e.target as HTMLElement | null
    if (el?.tagName === "INPUT" || el?.tagName === "TEXTAREA" || el?.isContentEditable) return
    e.preventDefault()
    focusSearch()
  }
}

/** 搜索框内 Esc：先清空，已空则回落焦点到树 */
function onSearchEsc(e: KeyboardEvent) {
  if (e.isComposing) return
  if (filter.value) {
    filter.value = ""
    return
  }
  searchInputRef.value?.blur()
  treeBodyEl.value?.querySelector<HTMLElement>(".n-tree")?.focus()
}

/** 树上焦点：Enter 连接/展开，F2 重命名，Delete 删除，Menu 键弹菜单，Esc 清筛选 */
function onTreeKeydown(e: KeyboardEvent) {
  if (e.repeat || e.isComposing) return
  if (e.key === "Escape") {
    // Esc 级联：先关右键菜单 → 再清搜索词 → 最后取消选中
    if (ctxMenuShow.value) {
      ctxMenuShow.value = false
      return
    }
    if (filter.value) {
      e.preventDefault()
      filter.value = ""
      return
    }
    if (selectedKeys.value.length > 0) {
      e.preventDefault()
      selectedKeys.value = []
      // naive 的 pending 游标（行内淡色背景 + suffix 展开）没有公开 API，
      // 借它自己的 focusout 清理路径置空：派发 relatedTarget 为空的合成事件，
      // 不真实移焦，方向键导航不中断
      treeBodyEl.value
        ?.querySelector<HTMLElement>(".n-tree")
        ?.dispatchEvent(new FocusEvent("focusout", { relatedTarget: null }))
    }
    return
  }
  if (e.ctrlKey || e.metaKey || e.altKey) return
  if (e.key === "ContextMenu") {
    const k = selectedKeys.value[0]
    if (!k) return
    const el = treeBodyEl.value?.querySelector<HTMLElement>(
      `[data-node-key="${CSS.escape(k)}"]`,
    )
    if (!el) return
    const rect = el.getBoundingClientRect()
    showCtxMenu(k, rect.left + rect.width / 2, rect.bottom + 4)
    e.preventDefault()
    return
  }
  const k = selectedKeys.value[0]
  if (!k) return
  const node = findNode(k)
  if (!node) return
  if (e.key === "Enter") {
    e.preventDefault()
    if (node.type === "host") emit("open-host", node)
    else toggleExpand(node.key)
  } else if (e.key === "F2") {
    e.preventDefault()
    openRename(node)
  } else if (e.key === "Delete") {
    e.preventDefault()
    confirmDelete(node)
  }
}

function renderMenuIcon(comp: unknown) {
  return () => h(NIcon, null, { default: () => h(comp as never) })
}

const ctxMenuOptions = computed<DropdownOption[]>(() => {
  const node = ctxMenuKey.value ? findNode(ctxMenuKey.value) : null
  const opts: DropdownOption[] = []
  if (!node) {
    // 空白处右键：根级操作
    opts.push(
      {
        label: t("hosts.ctxMenu.newHost"),
        key: "new-host",
        icon: renderMenuIcon(ServerOutline),
      },
      {
        label: t("hosts.tree.newFolder"),
        key: "new-folder",
        icon: renderMenuIcon(FolderAddOutlined),
      },
      { type: "divider", key: "d-blank-1" },
      {
        label: t("hosts.tree.importSshConfig"),
        key: "import-ssh",
        icon: renderMenuIcon(DownloadOutline),
      },
      {
        label: t("hosts.tree.refresh"),
        key: "refresh",
        icon: renderMenuIcon(RefreshOutline),
      },
      { type: "divider", key: "d-blank-2" },
      {
        label: t("hosts.tree.sort"),
        key: "sort",
        icon: renderMenuIcon(SwapVerticalOutline),
        children: [
          {
            label: t("hosts.tree.sortByName"),
            key: "sort-name",
            icon:
              store.hostSortMode === "name"
                ? renderMenuIcon(CheckmarkOutline)
                : undefined,
          },
          {
            label: t("hosts.tree.sortByAddr"),
            key: "sort-addr",
            icon:
              store.hostSortMode === "addr"
                ? renderMenuIcon(CheckmarkOutline)
                : undefined,
          },
        ],
      },
    )
    return opts
  }
  if (node.type === "folder") {
    opts.push(
      {
        label: t("hosts.ctxMenu.newSubFolder"),
        key: "new-folder",
        icon: renderMenuIcon(FolderAddOutlined),
      },
      {
        label: t("hosts.ctxMenu.newHost"),
        key: "new-host",
        icon: renderMenuIcon(ServerOutline),
      },
      { type: "divider", key: "d1" },
      {
        label: t("hosts.ctxMenu.rename"),
        key: "rename",
        icon: renderMenuIcon(CreateOutline),
      },
      {
        label: t("hosts.ctxMenu.delete"),
        key: "delete",
        icon: renderMenuIcon(TrashOutline),
      },
    )
  } else {
    opts.push(
      {
        label: t("hosts.ctxMenu.openTerminal"),
        key: "open",
        icon: renderMenuIcon(TerminalOutline),
      },
      {
        label: t("hosts.ctxMenu.newSession"),
        key: "open-new",
        icon: renderMenuIcon(AddOutline),
      },
      { type: "divider", key: "d0" },
      { label: t("hosts.ctxMenu.edit"), key: "edit", icon: renderMenuIcon(CreateOutline) },
      {
        label: t("hosts.ctxMenu.copyConn"),
        key: "copy-conn",
        icon: renderMenuIcon(LinkOutline),
      },
      { label: t("hosts.ctxMenu.copy"), key: "copy", icon: renderMenuIcon(CopyOutline) },
      {
        label: store.isHostPinned(node.id)
          ? t("hosts.ctxMenu.unpinTop")
          : t("hosts.ctxMenu.pinTop"),
        key: "pin-top",
        icon: renderMenuIcon(
          store.isHostPinned(node.id) ? PushpinFilled : PushpinOutlined,
        ),
      },
      {
        label: t("hosts.ctxMenu.delete"),
        key: "delete",
        icon: renderMenuIcon(TrashOutline),
      },
    )
  }
  return opts
})

function onCtxSelect(key: string) {
  ctxMenuShow.value = false
  const targetKey = ctxMenuKey.value
  const node = targetKey ? findNode(targetKey) : null
  switch (key) {
    case "new-folder":
      openCreateFolder(node && node.type === "folder" ? node.id : 0)
      break
    case "new-host":
      emit("create-host", node && node.type === "folder" ? node.id : 0)
      break
    case "refresh":
      void onRefresh()
      break
    case "sort-name":
      store.setHostSortMode("name")
      // 树形视图排序只在目录内生效，明确告知避免"点了没反应"的误解
      message.success(
        t(flatMode.value ? "hosts.tree.sortedByName" : "hosts.tree.sortedByNameGrouped"),
      )
      break
    case "sort-addr":
      store.setHostSortMode("addr")
      message.success(
        t(flatMode.value ? "hosts.tree.sortedByAddr" : "hosts.tree.sortedByAddrGrouped"),
      )
      break
    case "import-ssh":
      importModalShow.value = true
      break
    case "open":
      if (node?.type === "host") emit("open-host", node)
      break
    case "open-new":
      if (node?.type === "host") emit("open-host", node, true)
      break
    case "edit":
      if (node?.type === "host") openEditHost(node)
      break
    case "copy":
      if (node?.type === "host") copyHost(node)
      break
    case "copy-conn":
      if (node?.type === "host") void copyConnStr(node)
      break
    case "pin-top":
      if (node?.type === "host") store.togglePinHost(node.id)
      break
    case "rename":
      if (node) openRename(node)
      break
    case "delete":
      if (node) confirmDelete(node)
      break
  }
}

const folderModalOpen = ref(false)
const folderName = ref("")
const folderParentGid = ref<number>(0)
const folderSubmitting = ref(false)

function openCreateFolder(parentGid: number) {
  folderParentGid.value = parentGid
  folderName.value = ""
  folderModalOpen.value = true
}

async function submitFolder() {
  const name = folderName.value.trim()
  if (!name) {
    message.warning(t("hosts.message.folderNameRequired"))
    return
  }
  folderSubmitting.value = true
  try {
    const g = await store.addGroup({
      parent_id: folderParentGid.value,
      name,
    })
    folderModalOpen.value = false
    message.success(t("hosts.message.folderCreated", { name }))
    await nextTick()
    revealKey(`folder-${g.id}`)
  } catch (e) {
    message.error(t("hosts.message.createFailed", { error: String(e) }))
  } finally {
    folderSubmitting.value = false
  }
}

const renameModalOpen = ref(false)
const renameValue = ref("")
const renameTarget = ref<HostNode | null>(null)
const renameSubmitting = ref(false)

function openRename(node: HostNode) {
  if (node.type === "host") {
    const host = store.findHost(node.id)
    if (host) emit("edit-host", host as Host)
    return
  }
  renameTarget.value = node
  renameValue.value = node.label
  renameModalOpen.value = true
}

async function submitRename() {
  const target = renameTarget.value
  if (!target) return
  const name = renameValue.value.trim()
  if (!name) {
    message.warning(t("hosts.message.nameRequired"))
    return
  }
  renameSubmitting.value = true
  try {
    await store.editGroup(target.id, { name })
    renameModalOpen.value = false
    message.success(t("hosts.message.renamed"))
  } catch (e) {
    message.error(t("hosts.message.renameFailed", { error: String(e) }))
  } finally {
    renameSubmitting.value = false
  }
}

function openEditHost(node: HostNode) {
  const host = store.findHost(node.id)
  if (!host) {
    message.error(t("hosts.message.hostNotFound"))
    return
  }
  emit("edit-host", host as Host)
}

async function copyHost(node: HostNode) {
  const host = store.findHost(node.id)
  if (!host) return
  try {
    await store.addHost({
      gid: host.gid,
      name: t("hosts.message.copiedName", { name: host.name }),
      addr: host.addr,
      port: host.port,
      username: host.username,
      icon: host.icon ?? null,
      color: host.color ?? null,
      desc: host.desc ?? null,
    })
    message.success(t("hosts.message.copied"))
  } catch (e) {
    message.error(t("hosts.message.copyFailed", { error: String(e) }))
  }
}

/** 复制连接串（user@addr:port），与"复制主机"（克隆记录）区分 */
async function copyConnStr(node: HostNode) {
  const host = store.findHost(node.id)
  if (!host) return
  const auth = host.username ? `${host.username}@` : ""
  const text = `${auth}${host.addr}:${host.port}`
  try {
    await copyText(text)
    message.success(t("hosts.message.connCopied", { text }))
  } catch (e) {
    message.error(t("hosts.message.copyFailed", { error: String(e) }))
  }
}

function confirmDelete(node: HostNode) {
  const tip =
    node.type === "folder"
      ? t("hosts.message.deleteFolderConfirm", { name: node.label })
      : t("hosts.message.deleteHostConfirm", { name: node.label })
  dialog.warning({
    title: t("hosts.message.deleteTitle"),
    content: tip,
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        if (node.type === "folder") await store.removeGroup(node.id)
        else await store.removeHost(node.id)
        message.success(t("hosts.message.deleted"))
      } catch (e) {
        message.error(t("hosts.message.deleteFailed", { error: String(e) }))
      }
    },
  })
}

function revealKey(key: string) {
  const ancestors = collectAncestorKeys(key)
  const set = new Set(expandedKeys.value)
  for (const a of ancestors) set.add(a)
  expandedKeys.value = Array.from(set)
  selectedKeys.value = [key]
}

function newFolderAtSelection() {
  openCreateFolder(resolveSelectedFolderGid())
}

function newHostAtSelection() {
  emit("create-host", resolveSelectedFolderGid())
}

async function onRefresh() {
  try {
    await store.refresh()
    message.success(t("hosts.message.refreshed"))
  } catch (e) {
    message.error(t("hosts.message.refreshFailed", { error: String(e) }))
  }
}
</script>

<template>
  <div class="host-tree" @keydown="onRootKeydown">
    <div class="tree-header">
      <span class="tree-title">{{ t("hosts.tree.title") }}</span>
      <div class="header-actions">
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              type="primary"
              @click="newHostAtSelection"
            >
              <template #icon>
                <NIcon :size="20"><AddCircleOutline /></NIcon>
              </template>
            </NButton>
          </template>
          {{ t("hosts.ctxMenu.newHost") }}
        </NTooltip>
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              @click="newFolderAtSelection"
            >
              <template #icon>
                <NIcon :size="20"><FolderAddOutlined /></NIcon>
              </template>
            </NButton>
          </template>
          {{ t("hosts.tree.newFolder") }}
        </NTooltip>
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              :type="hostsPinned ? 'primary' : 'default'"
              @click="hostsPinned = !hostsPinned"
            >
              <template #icon>
                <NIcon :size="18">
                  <component :is="hostsPinned ? PushpinFilled : PushpinOutlined" />
                </NIcon>
              </template>
            </NButton>
          </template>
          {{ hostsPinned ? t("hosts.tree.unpin") : t("hosts.tree.pin") }}
        </NTooltip>
        <NTooltip>
          <template #trigger>
            <NButton
              size="small"
              quaternary
              circle
              @click="emit('close')"
            >
              <template #icon>
                <NIcon :size="20"><CloseOutline /></NIcon>
              </template>
            </NButton>
          </template>
          {{ t("common.close") }}
        </NTooltip>
      </div>
    </div>

    <div class="tree-search">
      <NInput
        ref="searchInputRef"
        v-model:value="filter"
        size="small"
        :placeholder="t('hosts.tree.searchPlaceholder')"
        clearable
        @keydown.esc="onSearchEsc"
      >
        <template #prefix>
          <NIcon><SearchOutline /></NIcon>
        </template>
        <template #suffix>
          <NTooltip>
            <template #trigger>
              <NButton
                size="tiny"
                quaternary
                circle
                :type="flatMode ? 'primary' : 'default'"
                @click="flatMode = !flatMode"
              >
                <template #icon>
                  <NIcon :size="16"><ListOutline /></NIcon>
                </template>
              </NButton>
            </template>
            {{ flatMode ? t("hosts.tree.treeView") : t("hosts.tree.flatView") }}
          </NTooltip>
          <NTooltip>
            <template #trigger>
              <NButton
                size="tiny"
                quaternary
                circle
                :disabled="flatMode"
                @click="toggleAllExpand"
              >
                <template #icon>
                  <NIcon :size="14">
                    <component :is="allExpanded ? ContractOutline : ExpandOutline" />
                  </NIcon>
                </template>
              </NButton>
            </template>
            {{ allExpanded ? t("hosts.tree.collapseAll") : t("hosts.tree.expandAll") }}
          </NTooltip>
        </template>
      </NInput>
    </div>

    <div
      ref="treeBodyEl"
      class="tree-body"
      :class="{ 'drop-on-root': dropTargetKey === '__root__' }"
      :data-drop-key="dropTargetKey ?? ''"
      @contextmenu="onBlankContextMenu"
      @keydown="onTreeKeydown"
    >
      <div class="tree-spin">
        <NEmpty
          v-if="!store.loading && treeData.length === 0"
          :description="t('hosts.tree.empty')"
        >
          <div class="empty-actions">
            <NButton type="primary" size="small" @click="newHostAtSelection">
              <template #icon>
                <NIcon><ServerOutline /></NIcon>
              </template>
              {{ t("hosts.ctxMenu.newHost") }}
            </NButton>
            <NButton size="small" @click="importModalShow = true">
              <template #icon>
                <NIcon><DownloadOutline /></NIcon>
              </template>
              {{ t("hosts.tree.importSshConfig") }}
            </NButton>
          </div>
        </NEmpty>
        <NEmpty
          v-else-if="!store.loading && isFiltering && filteredDisplayData.length === 0"
          :description="t('hosts.tree.noMatch')"
        >
          <NButton size="small" quaternary @click="filter = ''">
            {{ t("hosts.tree.clearFilter") }}
          </NButton>
        </NEmpty>
        <NTree
          v-else
          :data="filteredDisplayData"
          block-line
          show-line
          ellipsis
          expand-on-click
          :selected-keys="selectedKeys"
          :expanded-keys="visibleExpandedKeys"
          :render-label="renderLabel"
          :render-prefix="renderPrefix"
          :render-suffix="renderSuffix"
          :node-props="nodeProps"
          :selectable="true"
          key-field="key"
          label-field="label"
          children-field="children"
          style="height: 100%"
          @update:selected-keys="(k: string[]) => (selectedKeys = k)"
          @update:expanded-keys="onUpdateExpandedKeys"
        />
      </div>
    </div>

    <NDropdown
      placement="bottom-start"
      trigger="manual"
      :x="ctxMenuX"
      :y="ctxMenuY"
      :options="ctxMenuOptions"
      :show="ctxMenuShow"
      @select="onCtxSelect"
      @clickoutside="ctxMenuShow = false"
    />

    <!-- 拖拽 ghost：跟随鼠标显示主机名 -->
    <Teleport to="body">
      <div
        v-if="dragGhost"
        class="drag-ghost"
        :style="{ left: dragGhost.x + 12 + 'px', top: dragGhost.y + 12 + 'px' }"
      >
        <NIcon :size="14" color="#7c5cff"><TerminalOutline /></NIcon>
        <span>{{ dragGhost.label }}</span>
      </div>
    </Teleport>

    <NModal v-model:show="folderModalOpen" :mask-closable="false">
      <NCard
        style="width: 420px"
        :title="t('hosts.tree.newFolderDialog')"
        size="small"
        :bordered="false"
        role="dialog"
        aria-modal="true"
      >
        <NForm
          label-placement="top"
          require-mark-placement="right-hanging"
          size="small"
        >
          <NFormItem :label="t('hosts.tree.parentDir')" :show-feedback="false">
            <NTreeSelect
              v-model:value="folderParentGid"
              :options="folderSelectOptions"
              key-field="key"
              label-field="label"
              children-field="children"
              default-expand-all
              :consistent-menu-width="false"
              :placeholder="t('hosts.tree.parentDirPlaceholder')"
            />
          </NFormItem>
          <div style="margin-top: 12px">
            <NFormItem :label="t('hosts.tree.name')" :show-feedback="false">
              <NInput
                v-model:value="folderName"
                :placeholder="t('hosts.tree.namePlaceholder')"
                autofocus
                @keydown.enter="submitFolder"
              />
            </NFormItem>
          </div>
        </NForm>
        <template #footer>
          <NSpace justify="end">
            <NButton :disabled="folderSubmitting" @click="folderModalOpen = false">
              {{ t("hosts.tree.cancel") }}
            </NButton>
            <NButton
              type="primary"
              :loading="folderSubmitting"
              @click="submitFolder"
            >
              {{ t("hosts.tree.create") }}
            </NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>

    <NModal v-model:show="renameModalOpen" :mask-closable="false">
      <NCard
        style="width: 380px"
        :title="t('hosts.tree.renameDialog')"
        size="small"
        :bordered="false"
      >
        <NInput
          v-model:value="renameValue"
          autofocus
          @keydown.enter="submitRename"
        />
        <template #footer>
          <NSpace justify="end">
            <NButton :disabled="renameSubmitting" @click="renameModalOpen = false">
              {{ t("hosts.tree.cancel") }}
            </NButton>
            <NButton
              type="primary"
              :loading="renameSubmitting"
              @click="submitRename"
            >
              {{ t("hosts.tree.save") }}
            </NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>

    <SshConfigImportModal v-model:show="importModalShow" />
  </div>
</template>

<style scoped>
.host-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px;
  gap: 10px;
}

.tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}

.tree-title {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-subtle);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.tree-search {
  flex-shrink: 0;
}

/* 收进输入框的视图切换小按钮：贴紧排布 */
.tree-search :deep(.n-input__suffix) {
  gap: 2px;
}

.tree-body {
  flex: 1;
  overflow: auto;
  margin: 0 -8px;
  padding: 0 4px;
  border-radius: 6px;
  transition: background 0.12s ease, box-shadow 0.12s ease;
}

.tree-body.drop-on-root {
  background: rgba(124, 92, 255, 0.06);
  box-shadow: inset 0 0 0 1px rgba(124, 92, 255, 0.45);
}

.tree-spin {
  height: 100%;
}

/* 空态下的引导操作 */
.empty-actions {
  display: flex;
  justify-content: center;
  gap: 8px;
  margin-top: 8px;
}

:deep(.n-tree-node-content) {
  font-size: 13px;
  border-radius: 6px;
}

:deep(.n-tree-node--selected .n-tree-node-content) {
  background: rgba(124, 92, 255, 0.15) !important;
}

/* 悬停节点行末元信息：grid 0fr→1fr 做宽度动画，悬停时名字被平滑推开而非跳变；
   不支持网格轨道动画的引擎（旧 WebKitGTK）退化为直接展开，功能不受影响 */
:deep(.n-tree-node-content__suffix) {
  flex-shrink: 0;
  min-width: 0;
  max-width: 60%;
}
:deep(.node-suffix) {
  display: grid;
  grid-template-columns: 0fr;
  margin-left: 0;
  opacity: 0;
  font-size: 11px;
  transition:
    grid-template-columns 0.18s ease,
    opacity 0.18s ease,
    margin-left 0.18s ease;
}
:deep(.n-tree-node-content:hover .node-suffix) {
  grid-template-columns: 1fr;
  margin-left: 8px;
  opacity: 0.55;
}
/* 选中/键盘待选行常驻展开（悬停、选中、方向键待选任一命中即显示；方向键移动时
   naive 会把虚拟滚动滚到待选行，展开正好可见；焦点离开树时 pending 自动清空） */
:deep(.n-tree-node--selected .node-suffix) {
  grid-template-columns: 1fr;
  margin-left: 8px;
  opacity: 0.55;
}
:deep(.n-tree-node--pending .node-suffix) {
  grid-template-columns: 1fr;
  margin-left: 8px;
  opacity: 0.55;
}
:deep(.node-suffix-text) {
  grid-column: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

/* 置顶图钉：suffix 内、折叠网格外的常驻图标；行末元信息外层容器 inline-flex 收紧排布 */
:deep(.node-suffix-wrap) {
  display: inline-flex;
  align-items: center;
  min-width: 0;
}
:deep(.node-pin) {
  flex-shrink: 0;
  margin-right: 4px;
}

/* 拖拽中：drop 目标 folder 高亮 */
.tree-body[data-drop-key] :deep([data-node-key]) {
  transition: background 0.12s ease, box-shadow 0.12s ease;
}
.tree-body[data-drop-key=""] :deep([data-node-key]) {
  background: transparent;
}
</style>

<style>
/* drop target 高亮（穿透 scoped，因为 data-node-key 是动态加在内部节点上） */
.host-tree .tree-body [data-node-key].drop-target,
.host-tree .tree-body [data-node-key][data-drop-active="true"] {
  outline: 2px solid rgba(124, 92, 255, 0.55);
  outline-offset: -2px;
  border-radius: 6px;
}

.drag-ghost {
  position: fixed;
  z-index: 9999;
  pointer-events: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: var(--ashell-panel-bg, #1f1f24);
  color: var(--ashell-text, #e5e5ea);
  border: 1px solid rgba(124, 92, 255, 0.5);
  border-radius: 6px;
  font-size: 12px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  user-select: none;
  white-space: nowrap;
}

/* 节点 tooltip（NTooltip 挂到 body，样式须全局） */
/* 搜索命中子串（renderLabel 动态创建、挂在 NTree 子树，scoped 属性够不到，样式须全局） */
.node-label-hit {
  color: #7c5cff;
  font-weight: 600;
}

/* 拖拽中抑制行内元信息展开（body class 由拖拽逻辑维护，scoped 够不到 body 级状态） */
body.ashell-dragging .host-tree .node-suffix {
  grid-template-columns: 0fr !important;
  margin-left: 0 !important;
  opacity: 0 !important;
}
</style>
