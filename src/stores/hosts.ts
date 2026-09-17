import { defineStore } from 'pinia'
import { ref } from 'vue'
import {
  createGroup,
  deleteGroup,
  listGroups,
  updateGroup,
} from '@/api/groups'
import {
  createHost,
  deleteHost,
  listHostsWithGroup,
  updateHost,
} from '@/api/hosts'
import type {
  Group,
  GroupCreate,
  GroupUpdate,
  HostCreate,
  HostUpdate,
  HostWithGroup,
  HostNode,
} from '@/types'

/** 主机排序模式：按名称 / 按地址（IP 数值序） */
export type HostSortMode = "name" | "addr"

const PINNED_KEY = "ashell:host-pinned-ids"
const SORT_KEY = "ashell:host-sort-mode"

function loadPinnedHostIds(): Set<number> {
  if (typeof localStorage === "undefined") return new Set()
  try {
    const raw = localStorage.getItem(PINNED_KEY)
    const arr = raw ? (JSON.parse(raw) as unknown) : []
    if (!Array.isArray(arr)) return new Set()
    return new Set(
      arr.filter((n): n is number => typeof n === "number" && Number.isFinite(n)),
    )
  } catch {
    return new Set()
  }
}

function loadHostSortMode(): HostSortMode {
  if (typeof localStorage === "undefined") return "name"
  return localStorage.getItem(SORT_KEY) === "addr" ? "addr" : "name"
}

/** 地址比较：双方都是 IPv4 时按 octet 数值比较（字典序会把 10.x 排在 9.x 前），否则退化为字符串比较 */
export function compareAddr(a: string, b: string): number {
  const pa = a.split(".").map(Number)
  const pb = b.split(".").map(Number)
  const ipv4 =
    pa.length === 4 &&
    pb.length === 4 &&
    pa.every((n) => Number.isFinite(n)) &&
    pb.every((n) => Number.isFinite(n))
  if (ipv4) {
    for (let i = 0; i < 4; i++) {
      if (pa[i] !== pb[i]) return pa[i]! - pb[i]!
    }
    return 0
  }
  return a.localeCompare(b)
}

/** 组内排序：置顶主机优先（组内相对顺序保持名称序），其余按排序模式 */
function sortHosts(
  list: HostWithGroup[],
  pinned: Set<number>,
  mode: HostSortMode,
): HostWithGroup[] {
  return list.slice().sort((a, b) => {
    const pa = pinned.has(a.id) ? 0 : 1
    const pb = pinned.has(b.id) ? 0 : 1
    if (pa !== pb) return pa - pb
    return mode === "addr" ? compareAddr(a.addr, b.addr) : a.name.localeCompare(b.name)
  })
}

function buildTree(
  groups: Group[],
  hosts: HostWithGroup[],
  pinned: Set<number>,
  mode: HostSortMode,
): HostNode[] {
  // 按 parent_id 索引 group
  const groupChildren = new Map<number, Group[]>()
  for (const g of groups) {
    const arr = groupChildren.get(g.parent_id) ?? []
    arr.push(g)
    groupChildren.set(g.parent_id, arr)
  }

  // 按 gid 索引 host
  const hostsByGid = new Map<number, HostWithGroup[]>()
  for (const h of hosts) {
    const arr = hostsByGid.get(h.gid) ?? []
    arr.push(h)
    hostsByGid.set(h.gid, arr)
  }

  function makeFolder(g: Group): HostNode {
    const subFolders = (groupChildren.get(g.id) ?? [])
      .slice()
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(makeFolder)
    const subHosts = sortHosts(hostsByGid.get(g.id) ?? [], pinned, mode).map(makeHost)
    return {
      key: `folder-${g.id}`,
      label: g.name,
      type: 'folder',
      id: g.id,
      parentId: g.parent_id,
      level: g.level,
      children: [...subFolders, ...subHosts],
    }
  }

  function makeHost(h: HostWithGroup): HostNode {
    return {
      key: `host-${h.id}`,
      label: h.name,
      type: 'host',
      id: h.id,
      gid: h.gid,
      host: h.addr,
      port: h.port,
      username: h.username,
      icon: h.icon ?? null,
      color: h.color ?? null,
      desc: h.desc ?? null,
      protocol: h.protocol ?? 'ssh',
    }
  }

  // 根节点：parent_id === 0
  const rootGroups = (groupChildren.get(0) ?? [])
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name))
    .map(makeFolder)

  // gid === 0 的 host 直接挂在根（如果允许）
  const rootHosts = sortHosts(hostsByGid.get(0) ?? [], pinned, mode).map(makeHost)

  return [...rootGroups, ...rootHosts]
}

export const useHostStore = defineStore('hosts', () => {
  const groups = ref<Group[]>([])
  const hosts = ref<HostWithGroup[]>([])
  const tree = ref<HostNode[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  // 置顶主机：纯前端偏好（不动后端 schema），组内置顶优先，树形/平铺共用
  const pinnedHostIds = ref<Set<number>>(loadPinnedHostIds())
  // 主机排序模式（组内生效；平铺视图额外把置顶全局提前）
  const hostSortMode = ref<HostSortMode>(loadHostSortMode())

  function isHostPinned(id: number): boolean {
    return pinnedHostIds.value.has(id)
  }

  function togglePinHost(id: number): void {
    const next = new Set(pinnedHostIds.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    pinnedHostIds.value = next
    try {
      localStorage.setItem(PINNED_KEY, JSON.stringify([...next]))
    } catch {
      // ignore
    }
    // 重排只需本地重建树，不回源后端
    tree.value = buildTree(groups.value, hosts.value, next, hostSortMode.value)
  }

  function setHostSortMode(mode: HostSortMode): void {
    if (hostSortMode.value === mode) return
    hostSortMode.value = mode
    try {
      localStorage.setItem(SORT_KEY, mode)
    } catch {
      // ignore
    }
    // 纯本地重排，不回源后端
    tree.value = buildTree(groups.value, hosts.value, pinnedHostIds.value, mode)
  }

  async function refresh() {
    loading.value = true
    error.value = null
    try {
      const [g, h] = await Promise.all([listGroups(), listHostsWithGroup()])
      groups.value = g
      hosts.value = h
      tree.value = buildTree(g, h, pinnedHostIds.value, hostSortMode.value)
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  function findHost(id: number): HostWithGroup | undefined {
    return hosts.value.find((h) => h.id === id)
  }

  function findGroup(id: number): Group | undefined {
    return groups.value.find((g) => g.id === id)
  }

  async function addGroup(input: GroupCreate) {
    const g = await createGroup(input)
    await refresh()
    return g
  }

  async function editGroup(id: number, input: GroupUpdate) {
    const g = await updateGroup(id, input)
    await refresh()
    return g
  }

  async function removeGroup(id: number) {
    await deleteGroup(id)
    await refresh()
  }

  async function addHost(input: HostCreate) {
    const h = await createHost(input)
    await refresh()
    return h
  }

  async function editHost(id: number, input: HostUpdate) {
    const h = await updateHost(id, input)
    await refresh()
    return h
  }

  async function removeHost(id: number) {
    await deleteHost(id)
    await refresh()
  }

  return {
    groups,
    hosts,
    tree,
    loading,
    error,
    refresh,
    findHost,
    findGroup,
    addGroup,
    editGroup,
    removeGroup,
    addHost,
    editHost,
    removeHost,
    isHostPinned,
    togglePinHost,
    hostSortMode,
    setHostSortMode,
  }
})
