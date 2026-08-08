import { request } from './client'
import type { QuickPhrase, QuickPhraseCreate } from '@/types'

export function listPhrases(): Promise<QuickPhrase[]> {
  return request<QuickPhrase[]>('/api/ai-phrases')
}

export function createPhrase(input: QuickPhraseCreate): Promise<QuickPhrase> {
  return request<QuickPhrase>('/api/ai-phrases', { method: 'POST', json: input })
}

export function deletePhrase(id: number): Promise<void> {
  return request<void>(`/api/ai-phrases/${id}`, { method: 'DELETE' })
}

export function clearAllPhrases(): Promise<void> {
  return request<void>('/api/ai-phrases', { method: 'DELETE' })
}
