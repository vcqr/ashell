import { request } from './client'
import type { FrontendPrefs } from '@/utils/backupPrefs'

export interface BackupConfig {
  endpoint: string
  bucket: string
  region: string
  access_key: string
  secret_key: string
  path_prefix: string
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

export function testBackupConnection(input: SaveConfigInput): Promise<void> {
  return request<void>('/api/backup/test', { method: 'POST', json: input })
}

export function createBackup(
  commandHistory: string[],
  frontendPrefs: FrontendPrefs,
  password: string,
): Promise<{ key: string }> {
  return request<{ key: string }>('/api/backup/create', {
    method: 'POST',
    json: { command_history: commandHistory, frontend_prefs: frontendPrefs, password },
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

export function listBackups(): Promise<BackupItem[]> {
  return request<BackupItem[]>('/api/backup/list')
}

export function restoreBackup(key: string, password: string): Promise<RestoredBackup> {
  return request<RestoredBackup>('/api/backup/restore', {
    method: 'POST',
    json: { key, password },
  })
}

export function importBackup(content: string, password: string): Promise<RestoredBackup> {
  return request<RestoredBackup>('/api/backup/import', {
    method: 'POST',
    json: { content, password },
  })
}

export function deleteBackup(key: string): Promise<void> {
  return request<void>('/api/backup/delete', {
    method: 'POST',
    json: { key },
  })
}
