import { request } from './client'
import type { TmuxSessionInfo } from '@/types'

/** 列出 sid 关联主机上的全部 tmux 会话（远端未装 tmux 时后端报错） */
export function listTmuxSessions(sid: string, hostId: number | null): Promise<TmuxSessionInfo[]> {
  return request<TmuxSessionInfo[]>('/api/ssh/tmux/sessions', {
    params: { sid, host_id: hostId },
  })
}

/** 终止指定 tmux 会话 */
export function killTmuxSession(
  sid: string,
  name: string,
  hostId: number | null,
): Promise<void> {
  return request<void>('/api/ssh/tmux/kill', {
    method: 'POST',
    body: { sid, name, host_id: hostId },
  })
}
