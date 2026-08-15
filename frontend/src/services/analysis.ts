import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { AnalysisResponse, ClaudeModelCatalog, ClaudeStatus, CodexModelCatalog, Project, ProjectStats, SavedProject } from '../domain'

export const isDesktopRuntime = (): boolean => '__TAURI_INTERNALS__' in window

export async function loadCodexModels(): Promise<CodexModelCatalog> {
  if (!isDesktopRuntime()) {
    return {
      executable: 'codex',
      version: '브라우저 미리보기',
      source: 'preview',
      selectedModel: 'gpt-5-codex',
      models: [{
        slug: 'gpt-5-codex',
        displayName: 'GPT-5-Codex',
        description: '브라우저 미리보기 모델',
        defaultReasoningLevel: 'medium',
        supportedReasoningLevels: ['low', 'medium', 'high'],
      }],
      savedProvider: 'codex',
      savedClaudeModel: 'claude-opus-4-6',
    }
  }

  return invoke<CodexModelCatalog>('get_codex_models')
}

export async function saveAiSettings(settings: {
  model: string
  cliVersion: string
  executable: string
  provider?: string
  claudeModel?: string
}): Promise<void> {
  if (!isDesktopRuntime()) return
  await invoke('save_ai_settings', settings)
}

export async function selectProjectFolder(): Promise<string | null> {
  if (!isDesktopRuntime()) {
    return window.prompt('분석할 프로젝트 폴더 경로를 입력하세요.')
  }

  const selected = await open({ directory: true, multiple: false, title: '프로젝트 폴더 선택' })
  return typeof selected === 'string' ? selected : null
}

export async function loadClaudeModels(): Promise<ClaudeModelCatalog> {
  if (!isDesktopRuntime()) {
    return {
      models: [
        { slug: 'claude-opus-4-6', displayName: 'Claude Opus 4.6' },
        { slug: 'sonnet', displayName: 'Sonnet (latest alias)' },
      ],
      selectedModel: 'claude-opus-4-6',
    }
  }
  return invoke<ClaudeModelCatalog>('get_claude_models')
}

export async function checkClaudeCli(): Promise<ClaudeStatus> {
  if (!isDesktopRuntime()) {
    return { version: 'Claude CLI (미리보기)', executable: 'claude' }
  }
  return invoke<ClaudeStatus>('get_claude_status')
}

export async function analyzeProject(input: {
  projectPath: string
  enginePath: string
  configPath: string
  model: string
  provider: string
}): Promise<AnalysisResponse> {
  if (!isDesktopRuntime()) {
    await new Promise((resolve) => window.setTimeout(resolve, 850))
    return {
      projectPath: input.projectPath,
      domains: [],
      stats: { files: 0, units: 0, features: 0, flows: 0, resources: 0 },
    }
  }

  return invoke<AnalysisResponse>('analyze_project', { request: input })
}

export async function listSavedProjects(): Promise<SavedProject[]> {
  if (!isDesktopRuntime()) return []
  return invoke<SavedProject[]>('list_saved_projects')
}

export async function loadProjectMap(projectPath: string): Promise<AnalysisResponse> {
  if (!isDesktopRuntime()) {
    return {
      projectPath,
      domains: [],
      stats: { files: 0, units: 0, features: 0, flows: 0, resources: 0 },
    }
  }
  return invoke<AnalysisResponse>('load_project_map', { projectPath })
}

function formatUpdatedAt(updatedAtMs: number): string {
  if (!updatedAtMs) return '알 수 없음'
  const diffMs = Date.now() - updatedAtMs
  if (diffMs < 60_000) return '방금 전'
  const minutes = Math.floor(diffMs / 60_000)
  if (minutes < 60) return `${minutes}분 전`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}시간 전`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days}일 전`
  return new Date(updatedAtMs).toLocaleDateString('ko-KR')
}

export function createProject(
  path: string,
  options: {
    id?: string
    domains?: Project['domains']
    stats?: ProjectStats
    updatedAtMs?: number
    state?: Project['state']
  } = {},
): Project {
  const name = path.split(/[\\/]/).filter(Boolean).pop() ?? '새 프로젝트'
  const stats = options.stats ?? { files: 0, units: 0, features: 0, flows: 0, resources: 0 }
  const domains = options.domains ?? []
  return {
    id: options.id ?? `project-${Date.now()}`,
    name,
    path,
    branch: 'local',
    updatedAt: formatUpdatedAt(options.updatedAtMs ?? Date.now()),
    state: options.state ?? 'ready',
    stats,
    domains,
  }
}
