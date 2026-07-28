import { request } from './client'
import type { ForwardCreate, ForwardRule } from '@/types'

/** 列出 sid 关联的全部端口转发规则 */
export function listForwards(sid: string): Promise<ForwardRule[]> {
  return request<ForwardRule[]>('/api/ssh/forward', { params: { sid } })
}

/** 创建并启动一条端口转发规则 */
export function createForward(req: ForwardCreate): Promise<ForwardRule> {
  return request<ForwardRule>('/api/ssh/forward', {
    method: 'POST',
    body: req,
  })
}

/** 删除一条端口转发规则 */
export function deleteForward(sid: string, ruleId: string): Promise<void> {
  return request<void>(`/api/ssh/forward/${encodeURIComponent(ruleId)}`, {
    method: 'DELETE',
    params: { sid },
  })
}
