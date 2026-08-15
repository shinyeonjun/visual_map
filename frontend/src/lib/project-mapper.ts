import type { AnalysisResponse, DomainNode, MapDomain, Project, SavedProject } from '../domain'
import { domainPalette } from '../domain'
import { createProject } from '../services/analysis'

export function normalizeProjectPath(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/$/, '').toLowerCase()
}

export function workspaceKeyFromPath(path: string | undefined, fallback: string): string {
  return path?.split(/[\\/]/).filter(Boolean).pop() ?? fallback
}

export function mapDomainToNode(mapDomain: MapDomain, index: number, existing?: DomainNode): DomainNode {
  return {
    id: mapDomain.domainId,
    name: mapDomain.name,
    summary: mapDomain.summary,
    color: existing?.color ?? domainPalette[index % domainPalette.length],
    x: existing?.x ?? 70 + (index % 3) * 320,
    y: existing?.y ?? 70 + Math.floor(index / 3) * 220,
    units: mapDomain.units,
    features: mapDomain.features,
    entrypoints: mapDomain.entrypoints,
    confidence: mapDomain.confidence,
    status: mapDomain.status,
    dependencies: mapDomain.dependencies,
    signals: mapDomain.signals,
    featureItems: mapDomain.featureItems,
  }
}

export function projectFromAnalysis(
  summary: Pick<SavedProject, 'projectPath' | 'workspaceKey' | 'updatedAtMs'>,
  response: AnalysisResponse,
  existing?: Project,
): Project {
  const domains = response.domains.map((mapDomain, index) => {
    const prior = existing?.domains.find((domain) => domain.id === mapDomain.domainId)
    return mapDomainToNode(mapDomain, index, prior)
  })
  return createProject(summary.projectPath, {
    id: summary.workspaceKey,
    domains,
    stats: response.stats,
    updatedAtMs: summary.updatedAtMs,
    state: 'ready',
  })
}

export function mergeAnalysisIntoProject(project: Project, response: AnalysisResponse): Project {
  const domains = response.domains.map((mapDomain, index) => {
    const existing = project.domains.find((domain) => domain.id === mapDomain.domainId)
    return mapDomainToNode(mapDomain, index, existing)
  })
  return {
    ...project,
    id: workspaceKeyFromPath(response.workspacePath, project.id),
    state: 'ready',
    stats: response.stats,
    domains,
    updatedAt: '방금 전',
  }
}
