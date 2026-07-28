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

/** 把 group + host 列表合成前端的树结构 */
function buildTree(groups: Group[], hosts: HostWithGroup[]): HostNode[] {
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
    const subHosts = (hostsByGid.get(g.id) ?? [])
      .slice()
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(makeHost)
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
  const rootHosts = (hostsByGid.get(0) ?? [])
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name))
    .map(makeHost)

  return [...rootGroups, ...rootHosts]
}

export const useHostStore = defineStore('hosts', () => {
  const groups = ref<Group[]>([])
  const hosts = ref<HostWithGroup[]>([])
  const tree = ref<HostNode[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function refresh() {
    loading.value = true
    error.value = null
    try {
      const [g, h] = await Promise.all([listGroups(), listHostsWithGroup()])
      groups.value = g
      hosts.value = h
      tree.value = buildTree(g, h)
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
  }
})
