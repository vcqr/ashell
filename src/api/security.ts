import { request } from './client'

export interface OpPasswordStatus {
  set: boolean
}

export interface RevealResponse {
  password: string | null
  private_key: string | null
}

export function getOpPasswordStatus(): Promise<OpPasswordStatus> {
  return request<OpPasswordStatus>('/api/op-password')
}

export function setOpPassword(password: string): Promise<void> {
  return request<void>('/api/op-password', { method: 'POST', json: { password } })
}

export function changeOpPassword(oldPassword: string, newPassword: string): Promise<void> {
  return request<void>('/api/op-password', { method: 'PUT', json: { old_password: oldPassword, new_password: newPassword } })
}

export function clearOpPassword(password: string): Promise<void> {
  return request<void>('/api/op-password', { method: 'DELETE', json: { password } })
}

export function revealCredentials(hostId: number, password: string): Promise<RevealResponse> {
  return request<RevealResponse>(`/api/hosts/${hostId}/reveal`, { method: 'POST', json: { password } })
}
