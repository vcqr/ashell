import { request } from './client'

/** 通知平台：飞书 / 企业微信 / 钉钉 */
export type NotifyPlatform = 'feishu' | 'wecom' | 'dingtalk'

export interface NotifyConfig {
  enabled: boolean
  platform: NotifyPlatform
  webhook: string
  /** 钉钉加签 / 飞书签名校验密钥；企业微信无密钥留空 */
  secret: string
}

/** 测试发送输入：不需要 enabled（测试即视为启用） */
export interface NotifyTestInput {
  platform: NotifyPlatform
  webhook: string
  secret: string
}

export function getNotifyConfig(): Promise<NotifyConfig> {
  return request<NotifyConfig>('/api/notify/config')
}

export function saveNotifyConfig(input: NotifyConfig): Promise<void> {
  return request<void>('/api/notify/config', { method: 'PUT', json: input })
}

export function testNotifyConnection(input: NotifyTestInput): Promise<void> {
  return request<void>('/api/notify/test', { method: 'POST', json: input })
}
