import type { DomainFeature } from '../domain'

export function featureKindLabel(kind: string): string {
  if (kind === 'endpoint') return 'API'
  if (kind === 'operation') return 'FUNC'
  return kind.toUpperCase()
}

export function featureSummaryText(feature: Pick<DomainFeature, 'summary' | 'flows' | 'entrypoints'>): string {
  if (feature.summary?.trim()) return feature.summary.trim()
  if (feature.flows.length > 0) return `실행 길 ${feature.flows.length}개`
  if (feature.entrypoints > 0) return '외부 진입점'
  return '내부 서비스 로직'
}
