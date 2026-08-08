import { request } from './client'
import type {
  CommandTemplate,
  CommandTemplateCreate,
  CommandTemplateUpdate,
} from '@/types'

export function listTemplates(): Promise<CommandTemplate[]> {
  return request<CommandTemplate[]>('/api/command-templates')
}

export function createTemplate(input: CommandTemplateCreate): Promise<CommandTemplate> {
  return request<CommandTemplate>('/api/command-templates', { method: 'POST', json: input })
}

export function updateTemplate(id: number, input: CommandTemplateUpdate): Promise<CommandTemplate> {
  return request<CommandTemplate>(`/api/command-templates/${id}`, { method: 'PUT', json: input })
}

export function deleteTemplate(id: number): Promise<void> {
  return request<void>(`/api/command-templates/${id}`, { method: 'DELETE' })
}
