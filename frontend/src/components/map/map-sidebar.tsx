import type { AiProvider, ClaudeModel, CodexModel, Project } from '../../domain'
import { Icon } from '../icon'
import type { MapLayer } from '../../types/map'

interface MapSidebarProps {
  project: Project
  projects: Project[]
  mapLayer: MapLayer
  selectedDomainId: string | undefined
  selectedFeatureId: string
  isProjectMenuOpen: boolean
  provider: AiProvider
  model: string
  models: CodexModel[]
  claudeModels: ClaudeModel[]
  notice: string
  isAnalyzing: boolean
  cliReady: boolean
  onToggleProjectMenu: () => void
  onSwitchProject: (id: string) => void
  onAddProject: () => void
  onGoToDomains: () => void
  onOpenDomain: (domainId: string) => void
  onOpenFeature: (featureId: string) => void
  onProviderChange: (provider: AiProvider) => void
  onModelChange: (model: string) => void
  onAnalyze: () => void
}

export function MapSidebar({
  project,
  projects,
  mapLayer,
  selectedDomainId,
  selectedFeatureId,
  isProjectMenuOpen,
  provider,
  model,
  models,
  claudeModels,
  notice,
  isAnalyzing,
  cliReady,
  onToggleProjectMenu,
  onSwitchProject,
  onAddProject,
  onGoToDomains,
  onOpenDomain,
  onOpenFeature,
  onProviderChange,
  onModelChange,
  onAnalyze,
}: MapSidebarProps) {
  const providerLabel = provider === 'claude' ? 'Claude' : 'Codex'

  return (
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><span /></span><span>VisualMap</span><em>β</em></div>
      <div className="workspace-label">WORKSPACE <button className="tiny-icon" aria-label="워크스페이스 메뉴"><Icon name="chevron" size={15} /></button></div>
      <button className="project-switcher" onClick={onToggleProjectMenu}>
        <span className="project-dot" />
        <span className="project-switcher-text"><strong>{project.name}</strong><small>{project.branch} · 로컬 저장소</small></span>
        <Icon name="chevron" size={16} />
      </button>
      <button className="add-project-button" onClick={onAddProject}><Icon name="plus" size={15} />프로젝트 추가</button>
      {isProjectMenuOpen && (
        <div className="project-menu">
          {projects.map((item) => (
            <button key={item.id} onClick={() => onSwitchProject(item.id)}>
              <span className="project-dot" />
              <span><b>{item.name}</b><small>{item.path}</small></span>
            </button>
          ))}
        </div>
      )}
      <nav className="nav-group" aria-label="주요 메뉴">
        <p>MAP</p>
        <button className={`nav-item${mapLayer === 'domains' ? ' active' : ''}`} type="button" onClick={onGoToDomains} aria-current={mapLayer === 'domains' ? 'page' : undefined}>
          <Icon name="grid" /><span>도메인 지도</span>
        </button>
        <button className={`nav-item${mapLayer === 'features' ? ' active' : ''}`} type="button" onClick={() => selectedDomainId && onOpenDomain(selectedDomainId)} disabled={!selectedDomainId}>
          <Icon name="layers" /><span>기능 지도</span>
        </button>
        <button className={`nav-item${mapLayer === 'flow' ? ' active' : ''}`} type="button" onClick={() => selectedFeatureId && onOpenFeature(selectedFeatureId)} disabled={!selectedFeatureId}>
          <Icon name="route" /><span>실행 길</span>
        </button>
      </nav>
      <div className="project-stats">
        <p>프로젝트 요약</p>
        <span>{project.stats.units} units · {project.stats.features} features · {project.stats.flows} flows</span>
      </div>
      <div className="sidebar-bottom">
        <div className="engine-card">
          <div className="engine-status"><i /> {provider === 'claude' ? 'CLAUDE CLI' : 'CODEX CLI'}</div>
          <strong>의미 분석 엔진</strong>
          <label className="engine-model">
            <span>PROVIDER</span>
            <select value={provider} onChange={(event) => onProviderChange(event.target.value as AiProvider)}>
              <option value="codex">Codex</option>
              <option value="claude">Claude</option>
            </select>
          </label>
          <label className="engine-model">
            <span>MODEL</span>
            <select value={model} onChange={(event) => void onModelChange(event.target.value)}>
              {provider === 'claude'
                ? claudeModels.map((item) => <option key={item.slug} value={item.slug}>{item.displayName}</option>)
                : models.map((item) => <option key={item.slug} value={item.slug}>{item.displayName}</option>)}
            </select>
          </label>
          <small>{notice || '분석 결과는 캔버스에 표시됩니다.'}</small>
          <button className="engine-run" onClick={onAnalyze} disabled={isAnalyzing || !cliReady}>
            <Icon name="spark" size={13} />
            {isAnalyzing ? '분석 중…' : `${providerLabel}로 분석`}
            <span>↗</span>
          </button>
        </div>
        <div className="sidebar-foot"><span>Visual Map</span><span>v0.1.0</span></div>
      </div>
    </aside>
  )
}
