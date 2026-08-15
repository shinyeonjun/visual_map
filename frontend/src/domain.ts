export type AiProvider = 'codex' | 'claude'

export type AnalysisState = 'ready' | 'running' | 'partial' | 'error'

export type TrustStatus = 'verified' | 'candidate' | 'shared'

export function trustLabel(status: TrustStatus): string {
  if (status === 'verified') return '확인됨'
  if (status === 'shared') return '공유'
  return '후보'
}

export type DomainFlowStep = {
  label: string
  kind: string
  status: TrustStatus
}

export type DomainFlow = {
  id: string
  owner: string
  status: TrustStatus
  steps: DomainFlowStep[]
}

export type DomainFeature = {
  id: string
  name: string
  summary?: string | null
  kind: string
  status: TrustStatus
  entrypoints: number
  flows: DomainFlow[]
}

export type DomainNode = {
  id: string
  name: string
  summary: string
  color: string
  x: number
  y: number
  units: number
  features: number
  entrypoints: number
  confidence: number
  status: 'verified' | 'candidate' | 'shared'
  dependencies: string[]
  signals: string[]
  featureItems: DomainFeature[]
}

export type ProjectStats = {
  files: number
  units: number
  features: number
  flows: number
  resources: number
}

export type Project = {
  id: string
  name: string
  path: string
  branch: string
  updatedAt: string
  state: AnalysisState
  stats: ProjectStats
  domains: DomainNode[]
}

export type MapDomain = {
  domainId: string
  name: string
  summary: string
  status: 'verified' | 'candidate' | 'shared'
  confidence: number
  units: number
  features: number
  entrypoints: number
  dependencies: string[]
  signals: string[]
  featureItems: DomainFeature[]
}

export type CodexModel = {
  slug: string
  displayName: string
  description: string
  defaultReasoningLevel?: string | null
  supportedReasoningLevels: string[]
}

export type CodexModelCatalog = {
  executable: string
  version: string
  source: string
  selectedModel?: string | null
  models: CodexModel[]
  savedProvider: string
  savedClaudeModel: string
}

export type AnalysisResponse = {
  projectPath: string
  workspacePath?: string
  semanticResultPath?: string
  domains: MapDomain[]
  stats: ProjectStats
}

export type ClaudeStatus = {
  version: string
  executable: string
}

export type ClaudeModel = {
  slug: string
  displayName: string
}

export type ClaudeModelCatalog = {
  models: ClaudeModel[]
  selectedModel?: string | null
}

export type AnalysisProgress = {
  phase: 'preparing' | 'static' | 'context' | 'semantic' | 'finalizing' | 'complete' | 'error'
  label: string
  detail: string
  percent: number
  step: number
  totalSteps: number
  current?: number | null
  total?: number | null
  indeterminate: boolean
  elapsedMs: number
}

export const domainPalette = ['#e76f51', '#237a70', '#3264d6', '#d79b2e', '#7653a8', '#238f9d']
