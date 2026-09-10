import { request } from "./client"

/** 已信任的 SSH 主机密钥指纹（known_hosts 表） */
export interface KnownHost {
  id: number
  addr: string
  port: number
  key_type: string
  fingerprint: string
  created_at?: string
  updated_at?: string
}

/** 全部已信任指纹（设置页管理列表） */
export function listKnownHosts(): Promise<KnownHost[]> {
  return request<KnownHost[]>("/api/known-hosts")
}

/** 信任（或变更后更新）一条指纹 */
export function trustKnownHost(payload: {
  addr: string
  port: number
  key_type: string
  fingerprint: string
}): Promise<void> {
  return request<void>("/api/known-hosts", { method: "POST", json: payload })
}

/** 删除信任记录（下次连接重新确认） */
export function deleteKnownHost(id: number): Promise<void> {
  return request<void>(`/api/known-hosts/${id}`, { method: "DELETE" })
}
