import { request } from './client'

/** 微信远程状态（设置 → 远程） */
export interface WeixinStatus {
  logged_in: boolean
  bot_id: string
  user_id: string
  saved_at: number
  /** 通知事件是否同步推送到微信 */
  notify_enabled: boolean
  /** 长轮询保活任务是否在跑 */
  polling: boolean
  /** token 失效冷却剩余毫秒（0 = 未冷却） */
  paused_remaining_ms: number
  last_poll_at: number
  last_inbound_at: number
  last_error?: string
  peers: WeixinPeer[]
}

export interface WeixinPeer {
  user_id: string
  updated_at: number
  last_text: string
}

export type WeixinLoginStatus =
  | 'idle'
  | 'waiting'
  | 'scanned'
  | 'need_verifycode'
  | 'confirmed'
  | 'binded'
  | 'failed'

export interface WeixinLoginView {
  status: WeixinLoginStatus
  /** 二维码内容 URL（前端渲染成二维码图片） */
  qr_url?: string
  message?: string
}

export function getWeixinStatus(): Promise<WeixinStatus> {
  return request<WeixinStatus>('/api/weixin/status')
}

export function startWeixinLogin(): Promise<WeixinLoginView> {
  return request<WeixinLoginView>('/api/weixin/login/start', { method: 'POST' })
}

export function pollWeixinLogin(): Promise<WeixinLoginView> {
  return request<WeixinLoginView>('/api/weixin/login/poll')
}

export function submitWeixinVerifyCode(code: string): Promise<void> {
  return request<void>('/api/weixin/login/verify', { method: 'POST', json: { code } })
}

export function cancelWeixinLogin(): Promise<void> {
  return request<void>('/api/weixin/login/cancel', { method: 'POST' })
}

export function weixinLogout(): Promise<void> {
  return request<void>('/api/weixin/logout', { method: 'POST' })
}

export function saveWeixinConfig(notify_enabled: boolean): Promise<void> {
  return request<void>('/api/weixin/config', { method: 'PUT', json: { notify_enabled } })
}

export function testWeixinPush(): Promise<void> {
  return request<void>('/api/weixin/test', { method: 'POST' })
}
