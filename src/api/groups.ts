import { request } from './client'
import type { Group, GroupCreate, GroupUpdate } from '@/types'

export function listGroups(parentId?: number): Promise<Group[]> {
  return request<Group[]>('/api/groups', {
    params: parentId !== undefined ? { parent_id: parentId } : undefined,
  })
}

export function getGroup(id: number): Promise<Group> {
  return request<Group>(`/api/groups/${id}`)
}

export function createGroup(input: GroupCreate): Promise<Group> {
  return request<Group>('/api/groups', { method: 'POST', json: input })
}

export function updateGroup(id: number, input: GroupUpdate): Promise<Group> {
  return request<Group>(`/api/groups/${id}`, { method: 'PUT', json: input })
}

export function deleteGroup(id: number): Promise<void> {
  return request<void>(`/api/groups/${id}`, { method: 'DELETE' })
}
