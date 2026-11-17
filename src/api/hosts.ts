import { request } from './client'
import type { Host, HostCreate, HostUpdate, HostWithGroup, SshConfigHost } from '@/types'

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

/** 批量删除：被批量外主机作为跳板机引用的成员跳过并报告（referrer = 引用方名） */
export interface BatchDeleteSkipped {
  id: number
  name: string
  referrer: string
}

export interface BatchDeleteResult {
  deleted: number
  skipped: BatchDeleteSkipped[]
}

export function batchDeleteHosts(ids: number[]): Promise<BatchDeleteResult> {
  return request<BatchDeleteResult>('/api/hosts/batch-delete', {
    method: 'POST',
    json: { ids },
  })
}

/** 批量恢复被跳过的成员（前端据此组 i18n 文案） */
export interface BatchRestoreSkipped {
  id: number
  name: string
  /** "orphan" = 所属目录已删除；"jump_target_missing" = 跳板机目标仍处于删除态 */
  reason: string
}

export interface BatchRestoreResult {
  restored: number
  skipped: BatchRestoreSkipped[]
}

/** 批量恢复软删主机（删除撤销窗口）；凭证随行原样保留 */
export function batchRestoreHosts(ids: number[]): Promise<BatchRestoreResult> {
  return request<BatchRestoreResult>('/api/hosts/batch-restore', {
    method: 'POST',
    json: { ids },
  })
}

export function batchMoveHosts(ids: number[], gid: number): Promise<{ moved: number }> {
  return request<{ moved: number }>('/api/hosts/batch-move', {
    method: 'POST',
    json: { ids, gid },
  })
}

/** 从 ~/.ssh/config 解析主机列表 */
export function listSshConfigHosts(): Promise<SshConfigHost[]> {
  return request<SshConfigHost[]>('/api/hosts/ssh-config')
}
