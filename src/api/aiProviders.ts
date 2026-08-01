import { request } from './client'
import type {
  AiEngine,
  AiEnginesState,
  AiEngineUpdate,
  AiProvider,
  AiProviderCreate,
  AiProviderUpdate,
} from '@/types'

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

/* ---------- AI 引擎（sidecar）配置 ---------- */

export function listAiEngines(): Promise<AiEnginesState> {
  return request<AiEnginesState>('/api/ai-engines')
}

export function updateAiEngine(engine: string, input: AiEngineUpdate): Promise<AiEngine> {
  return request<AiEngine>(`/api/ai-engines/${engine}`, { method: 'PUT', json: input })
}

export function activateAiEngine(engine: string): Promise<AiEnginesState> {
  return request<AiEnginesState>('/api/ai-engines/active', { method: 'PUT', json: { engine } })
}
