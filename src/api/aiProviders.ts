import { request } from './client'
import type { AiProvider, AiProviderCreate, AiProviderUpdate } from '@/types'

export function listAiProviders(): Promise<AiProvider[]> {
  return request<AiProvider[]>('/api/ai-providers')
}

export function getAiProvider(id: string): Promise<AiProvider> {
  return request<AiProvider>(`/api/ai-providers/${id}`)
}

export function createAiProvider(input: AiProviderCreate): Promise<AiProvider> {
  return request<AiProvider>('/api/ai-providers', { method: 'POST', json: input })
}

export function updateAiProvider(id: string, input: AiProviderUpdate): Promise<AiProvider> {
  return request<AiProvider>(`/api/ai-providers/${id}`, { method: 'PUT', json: input })
}

export function deleteAiProvider(id: string): Promise<void> {
  return request<void>(`/api/ai-providers/${id}`, { method: 'DELETE' })
}

export function activateAiProvider(id: string): Promise<AiProvider> {
  return request<AiProvider>(`/api/ai-providers/${id}/activate`, { method: 'POST' })
}
