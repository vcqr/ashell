import { request } from './client'
import type { Host, HostCreate, HostUpdate, HostWithGroup } from '@/types'

export function listHosts(opts: { gid?: number; withGroup?: boolean } = {}): Promise<Host[]> {
  return request<Host[]>('/api/hosts', {
    params: {
      gid: opts.gid,
      with_group: opts.withGroup ? true : undefined,
    },
  })
}

/** 联表返回带 group_name / parent_gid 的列表 */
export function listHostsWithGroup(gid?: number): Promise<HostWithGroup[]> {
  return request<HostWithGroup[]>('/api/hosts', {
    params: {
      gid,
      with_group: true,
    },
  })
}

export function getHost(id: number): Promise<Host> {
  return request<Host>(`/api/hosts/${id}`)
}

export function createHost(input: HostCreate): Promise<Host> {
  return request<Host>('/api/hosts', { method: 'POST', json: input })
}

export function updateHost(id: number, input: HostUpdate): Promise<Host> {
  return request<Host>(`/api/hosts/${id}`, { method: 'PUT', json: input })
}

export function deleteHost(id: number): Promise<void> {
  return request<void>(`/api/hosts/${id}`, { method: 'DELETE' })
}
