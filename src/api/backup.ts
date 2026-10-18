import { request } from './client'
import type { FrontendPrefs } from '@/utils/backupPrefs'

/** 远端存储类型：S3 / WebDAV / SFTP */
export type BackupBackend = "s3" | "webdav" | "sftp"

export interface WebDavConfig {
  url: string
  username: string
  password: string
  path_prefix: string
}

/** SFTP 备份配置：引用已保存的 SSH 主机（凭证/主机密钥/跳板机复用主机配置） */
export interface SftpConfig {
  /** null/0 = 未选择；null 仅用于表单状态，提交前归一为 0 */
  host_id: number | null
  path_prefix: string
}

export interface BackupConfig {
  endpoint: string
  bucket: string
  region: string
  access_key: string
  secret_key: string
  path_prefix: string
  /** 旧版配置可能没有该字段 */
  webdav?: WebDavConfig | null
  sftp?: SftpConfig | null
}

export interface BackupItem {
  key: string
  timestamp: string
  size: number
}

export interface SaveConfigInput {
  endpoint: string
  bucket: string
  region: string
  access_key: string
  secret_key: string
  path_prefix: string
  webdav: WebDavConfig
  sftp: SftpConfig
}

/** 测试连接输入：保存配置的形状 + 待测试的后端类型 */
export interface TestConfigInput extends SaveConfigInput {
  backend: BackupBackend
}

/** 恢复/导入返回：命令历史 + 前端偏好快照（旧版本备份无 prefs 字段） */
export interface RestoredBackup {
  command_history: string[]
  frontend_prefs?: FrontendPrefs | null
}

export function getBackupConfig(): Promise<BackupConfig> {
  return request<BackupConfig>('/api/backup/config')
}

export function saveBackupConfig(input: SaveConfigInput): Promise<void> {
  return request<void>('/api/backup/config', { method: 'PUT', json: input })
}

export function testBackupConnection(input: TestConfigInput): Promise<void> {
  return request<void>('/api/backup/test', { method: 'POST', json: input })
}

export function createBackup(
  commandHistory: string[],
  frontendPrefs: FrontendPrefs,
  password: string,
  backend: BackupBackend,
): Promise<{ key: string }> {
  return request<{ key: string }>('/api/backup/create', {
    method: 'POST',
    json: { command_history: commandHistory, frontend_prefs: frontendPrefs, password, backend },
  })
}

export function exportBackup(
  commandHistory: string[],
  frontendPrefs: FrontendPrefs,
  password: string,
): Promise<{ content: string }> {
  return request<{ content: string }>('/api/backup/export', {
    method: 'POST',
    json: { command_history: commandHistory, frontend_prefs: frontendPrefs, password },
  })
}

export function listBackups(backend: BackupBackend): Promise<BackupItem[]> {
  return request<BackupItem[]>(`/api/backup/list?backend=${backend}`)
}

export function restoreBackup(
  key: string,
  password: string,
  backend: BackupBackend,
): Promise<RestoredBackup> {
  return request<RestoredBackup>('/api/backup/restore', {
    method: 'POST',
    json: { key, password, backend },
  })
}

export function importBackup(content: string, password: string): Promise<RestoredBackup> {
  return request<RestoredBackup>('/api/backup/import', {
    method: 'POST',
    json: { content, password },
  })
}

export function deleteBackup(key: string, backend: BackupBackend): Promise<void> {
  return request<void>('/api/backup/delete', {
    method: 'POST',
    json: { key, backend },
  })
}
