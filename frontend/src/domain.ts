export type AnalysisState = 'ready' | 'running' | 'partial' | 'error'

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

export type SemanticSuggestion = {
  domainId: string
  name: string
  summary?: string | null
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
}

export type AnalysisResponse = {
  projectPath: string
  workspacePath?: string
  semanticResultPath?: string
  domains: SemanticSuggestion[]
  stats?: Partial<ProjectStats>
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
