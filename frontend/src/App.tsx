import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { analyzeProject, checkClaudeCli, createProject, isDesktopRuntime, loadClaudeModels, loadCodexModels, saveAiSettings, selectProjectFolder } from './services/analysis'
import type { AiProvider, AnalysisProgress, ClaudeModel, CodexModel, CodexModelCatalog, DomainNode, MapDomain, Project } from './domain'
import { domainPalette } from './domain'
import { useCanvasViewport } from './hooks/useCanvasViewport'
import { AnalysisProgressPanel, Icon, SetupScreen } from './components/ui'
import { DomainCard } from './components/domain-card'
import './styles.css'

function featureKindLabel(kind: string): string {
  if (kind === 'endpoint') return 'API'
  if (kind === 'operation') return 'FUNC'
  return kind.toUpperCase()
}

function mapDomainToNode(mapDomain: MapDomain, index: number, existing?: DomainNode): DomainNode {
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

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string' && error.trim()) return error
  if (error && typeof error === 'object' && 'message' in error) {
    const message = error.message
    if (typeof message === 'string' && message.trim()) return message
  }
  return fallback
}

function resolveSelectedModel(current: string, catalog: CodexModelCatalog): string {
  if (catalog.models.some((item) => item.slug === current)) return current
  if (catalog.selectedModel && catalog.models.some((item) => item.slug === catalog.selectedModel)) return catalog.selectedModel
  return catalog.models[0]?.slug ?? ''
}

function App() {
  const [projects, setProjects] = useState<Project[]>([])
  const [projectId, setProjectId] = useState('')
  const [selectedId, setSelectedId] = useState('')
  const [model, setModel] = useState('')
  const [models, setModels] = useState<CodexModel[]>([])
  const [claudeModels, setClaudeModels] = useState<ClaudeModel[]>([])
  const [query, setQuery] = useState('')
  const [isProjectMenuOpen, setProjectMenuOpen] = useState(false)
  const [provider, setProvider] = useState<AiProvider>('codex')
  const [isAnalyzing, setAnalyzing] = useState(false)
  const [notice, setNotice] = useState('')
  const [isCodexChecking, setCodexChecking] = useState(false)
  const [codexVersion, setCodexVersion] = useState('')
  const [codexExecutable, setCodexExecutable] = useState('codex')
  const [codexError, setCodexError] = useState('')
  const [isClaudeChecking, setClaudeChecking] = useState(false)
  const [claudeVersion, setClaudeVersion] = useState('')
  const [claudeError, setClaudeError] = useState('')
  const [analysisProgress, setAnalysisProgress] = useState<AnalysisProgress | null>(null)
  const [activeView, setActiveView] = useState<'setup' | 'map'>('setup')
  const codexCheckingRef = useRef(false)
  const selectedModelRef = useRef(model)
  selectedModelRef.current = model
  const canvas = useCanvasViewport(1040, 700)
  const project = projects.find((item) => item.id === projectId)
  const selectedDomain = project?.domains.find((domain) => domain.id === selectedId) ?? project?.domains[0]
  const visibleDomains = useMemo(() => {
    if (!project) return []
    const normalizedQuery = query.trim().toLowerCase()
    if (!normalizedQuery) return project.domains
    return project.domains.filter((domain) => `${domain.name} ${domain.summary} ${domain.signals.join(' ')}`.toLowerCase().includes(normalizedQuery))
  }, [project, query])

  function switchProject(id: string) {
    const next = projects.find((item) => item.id === id)
    if (!next) return
    setProjectId(id)
    setSelectedId(next.domains[0]?.id ?? '')
    setProjectMenuOpen(false)
    setNotice('프로젝트 지도를 불러왔습니다.')
    setActiveView(next.domains.length > 0 ? 'map' : 'setup')
  }

  async function handleAddProject() {
    const path = await selectProjectFolder()
    if (!path) return
    const next = createProject(path)
    setProjects((current) => [...current, next])
    setProjectId(next.id)
    setSelectedId('')
    setActiveView('setup')
    setNotice('프로젝트가 추가되었습니다. 분석을 시작해 도메인을 찾아보세요.')
  }

  const savedClaudeModelRef = useRef('')

  const handleClaudeConnection = useCallback(async () => {
    setClaudeChecking(true)
    setClaudeError('')
    try {
      const [status, catalog] = await Promise.all([checkClaudeCli(), loadClaudeModels()])
      setClaudeVersion(status.version)
      setClaudeModels(catalog.models)
      const nextModel = catalog.selectedModel && catalog.models.some((item) => item.slug === catalog.selectedModel)
        ? catalog.selectedModel
        : catalog.models[0]?.slug ?? ''
      if (nextModel) {
        setModel(nextModel)
        selectedModelRef.current = nextModel
        savedClaudeModelRef.current = nextModel
      }
    } catch (error) {
      setClaudeVersion('')
      setClaudeError(getErrorMessage(error, 'Claude CLI를 확인하지 못했습니다.'))
    } finally {
      setClaudeChecking(false)
    }
  }, [])

  const handleCodexConnection = useCallback(async () => {
    if (codexCheckingRef.current) return
    codexCheckingRef.current = true
    setCodexChecking(true)
    setCodexError('')
    try {
      const catalog = await loadCodexModels()
      setModels(catalog.models)
      setCodexVersion(catalog.version)
      setCodexExecutable(catalog.executable)

      const savedProvider = (catalog.savedProvider === 'claude' ? 'claude' : 'codex') as AiProvider
      savedClaudeModelRef.current = catalog.savedClaudeModel || ''

      if (savedProvider === 'claude') {
        setProvider('claude')
        void handleClaudeConnection()
      } else {
        setProvider('codex')
        const nextModel = resolveSelectedModel(selectedModelRef.current, catalog)
        setModel(nextModel)
        selectedModelRef.current = nextModel
      }
    } catch (error) {
      setCodexVersion('')
      setCodexError(getErrorMessage(error, 'Codex CLI를 확인하지 못했습니다.'))
    } finally {
      codexCheckingRef.current = false
      setCodexChecking(false)
    }
  }, [handleClaudeConnection])

  useEffect(() => {
    void handleCodexConnection()
  }, [handleCodexConnection])

  useEffect(() => {
    if (!isDesktopRuntime()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<AnalysisProgress>('analysis-progress', (event) => {
      if (!disposed) setAnalysisProgress(event.payload)
    }).then((cleanup) => {
      if (disposed) cleanup()
      else unlisten = cleanup
    })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  const cliReady = provider === 'claude' ? Boolean(claudeVersion) : Boolean(codexVersion)

  async function handleAnalysis() {
    if (!project?.path || isAnalyzing || !cliReady || !model) return
    setAnalyzing(true)
    const providerLabel = provider === 'claude' ? 'Claude' : 'Codex'
    setAnalysisProgress({
      phase: 'preparing',
      label: '분석을 준비하는 중입니다',
      detail: '프로젝트 워크스페이스와 분석 설정을 확인하고 있습니다.',
      percent: 0,
      step: 1,
      totalSteps: 5,
      indeterminate: true,
      elapsedMs: 0,
    })
    setNotice(`${providerLabel} ${model}로 ${project.name}을 분석하는 중입니다…`)
    setProjects((current) => current.map((item) => item.id === project.id ? { ...item, state: 'running' } : item))
    try {
      if (provider === 'codex') {
        await saveAiSettings({ model, cliVersion: codexVersion, executable: codexExecutable, provider: 'codex' })
      } else {
        await saveAiSettings({
          model: selectedModelRef.current || models[0]?.slug || '',
          cliVersion: codexVersion,
          executable: codexExecutable,
          provider: 'claude',
          claudeModel: model,
        })
      }
      const response = await analyzeProject({ projectPath: project.path, enginePath: '', configPath: '', model, provider })
      if (response.domains.length === 0) {
        throw new Error('분석 결과에서 도메인을 찾지 못했습니다.')
      }
      setProjects((current) => current.map((item) => {
        if (item.id !== project.id) return item
        const domains = response.domains.map((mapDomain, index) => {
          const existing = item.domains.find((domain) => domain.id === mapDomain.domainId)
          return mapDomainToNode(mapDomain, index, existing)
        })
        return { ...item, state: 'ready', stats: response.stats, domains }
      }))
      setNotice(response.workspacePath ? `분석이 끝났습니다. 워크스페이스에 저장했습니다.` : '분석이 끝났습니다. 캔버스의 도메인 지도를 확인하세요.')
      setActiveView('map')
    } catch (error) {
      setProjects((current) => current.map((item) => item.id === project.id ? { ...item, state: 'error' } : item))
      const message = getErrorMessage(error, '분석을 완료하지 못했습니다.')
      setNotice(message)
      setAnalysisProgress({ phase: 'error', label: '분석을 완료하지 못했습니다', detail: message, percent: 0, step: 0, totalSteps: 5, indeterminate: false, elapsedMs: 0 })
    } finally {
      setAnalyzing(false)
    }
  }

  function handleProviderChange(next: AiProvider) {
    setProvider(next)
    if (next === 'claude') {
      const claudeModel = savedClaudeModelRef.current || claudeModels[0]?.slug || ''
      setModel(claudeModel)
      if (!claudeVersion || claudeModels.length === 0) void handleClaudeConnection()
      void saveAiSettings({
        model: selectedModelRef.current || '',
        cliVersion: codexVersion,
        executable: codexExecutable,
        provider: 'claude',
        claudeModel,
      }).catch(() => {})
    } else {
      const nextModel = models.length > 0 ? (models.find((m) => m.slug === selectedModelRef.current)?.slug ?? models[0]?.slug ?? '') : ''
      setModel(nextModel)
      selectedModelRef.current = nextModel
      void saveAiSettings({
        model: nextModel,
        cliVersion: codexVersion,
        executable: codexExecutable,
        provider: 'codex',
      }).catch(() => {})
    }
  }

  async function handleModelChange(nextModel: string) {
    setModel(nextModel)
    selectedModelRef.current = nextModel
    try {
      if (provider === 'codex') {
        if (!codexVersion || !nextModel) return
        await saveAiSettings({ model: nextModel, cliVersion: codexVersion, executable: codexExecutable, provider: 'codex' })
      } else {
        savedClaudeModelRef.current = nextModel
        await saveAiSettings({ model: '', cliVersion: codexVersion, executable: codexExecutable, provider: 'claude', claudeModel: nextModel })
      }
      setNotice(`${nextModel} 모델을 VisualMap 설정에 저장했습니다.`)
    } catch (error) {
      setNotice(getErrorMessage(error, '모델 설정을 저장하지 못했습니다.'))
    }
  }

  if (activeView === 'setup' || !project) {
    return <SetupScreen
      project={project}
      provider={provider}
      onProviderChange={handleProviderChange}
      onModelChange={handleModelChange}
      isAnalyzing={isAnalyzing}
      analysisProgress={analysisProgress}
      isCodexChecking={isCodexChecking}
      codexVersion={codexVersion}
      models={models}
      claudeModels={claudeModels}
      model={model}
      codexError={codexError}
      isClaudeChecking={isClaudeChecking}
      claudeVersion={claudeVersion}
      claudeError={claudeError}
      notice={notice}
      onConnectProject={handleAddProject}
      onCheckCodex={handleCodexConnection}
      onCheckClaude={handleClaudeConnection}
      onAnalyze={handleAnalysis}
    />
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><span className="brand-mark"><span /></span><span>VisualMap</span><em>β</em></div>
        <div className="workspace-label">WORKSPACE <button className="tiny-icon" aria-label="워크스페이스 메뉴"><Icon name="chevron" size={15} /></button></div>
        <button className="project-switcher" onClick={() => setProjectMenuOpen((open) => !open)}>
          <span className="project-dot" /><span className="project-switcher-text"><strong>{project.name}</strong><small>{project.branch} · 로컬 저장소</small></span><Icon name="chevron" size={16} />
        </button>
        <button className="add-project-button" onClick={handleAddProject}><Icon name="plus" size={15} />프로젝트 추가</button>
        {isProjectMenuOpen && <div className="project-menu">{projects.map((item) => <button key={item.id} onClick={() => switchProject(item.id)}><span className="project-dot" /><span><b>{item.name}</b><small>{item.path}</small></span></button>)}</div>}
        <nav className="nav-group" aria-label="주요 메뉴">
          <p>MAP</p>
          <button className="nav-item active" type="button" aria-current="page"><Icon name="grid" /><span>도메인 지도</span></button>
        </nav>
        <div className="project-stats">
          <p>프로젝트 요약</p>
          <span>{project.stats.units} units · {project.stats.features} features · {project.stats.flows} flows</span>
        </div>
        <div className="sidebar-bottom">
          <div className="engine-card"><div className="engine-status"><i /> {provider === 'claude' ? 'CLAUDE CLI' : 'CODEX CLI'}</div><strong>의미 분석 엔진</strong><label className="engine-model"><span>PROVIDER</span><select value={provider} onChange={(event) => handleProviderChange(event.target.value as AiProvider)}><option value="codex">Codex</option><option value="claude">Claude</option></select></label><label className="engine-model"><span>MODEL</span><select value={model} onChange={(event) => void handleModelChange(event.target.value)}>{provider === 'claude' ? claudeModels.map((item) => <option key={item.slug} value={item.slug}>{item.displayName}</option>) : models.map((item) => <option key={item.slug} value={item.slug}>{item.displayName}</option>)}</select></label><small>{notice || '분석 결과는 캔버스에 표시됩니다.'}</small><button className="engine-run" onClick={handleAnalysis} disabled={isAnalyzing || !cliReady}><Icon name="spark" size={13} />{isAnalyzing ? '분석 중…' : `${provider === 'claude' ? 'Claude' : 'Codex'}로 분석`}<span>↗</span></button></div>
          <div className="sidebar-foot"><span>Visual Map</span><span>v0.1.0</span></div>
        </div>
      </aside>

      <main className="main-stage">
        {analysisProgress && (isAnalyzing || analysisProgress.phase === 'error') && <AnalysisProgressPanel progress={analysisProgress} floating />}
        <div className="map-panel">
          <div className="canvas-context"><span className="eyebrow">BUSINESS MAP / 01</span><strong>{project.name}</strong><span>도메인 지도</span></div>
          <label className="canvas-search"><Icon name="search" size={16} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="도메인 검색" /></label>
          <div className="map-panel-head"><div className="map-legend"><span><i className="legend-line solid" />확인된 관계</span><span><i className="legend-line dashed" />공유 경계</span><span><i className="legend-node" />도메인</span></div><div className="map-controls"><button onClick={canvas.zoomIn}>+</button><span>{Math.round(canvas.view.scale * 100)}%</span><button onClick={canvas.zoomOut}>−</button><button onClick={canvas.fit}>⌖</button></div></div>
          <div ref={canvas.viewRef} className="map-canvas" {...canvas.handlers} onContextMenu={(event) => event.preventDefault()}>
            <div className="canvas-grid" style={canvas.gridStyle} />
            <div className="canvas-world" style={canvas.stageStyle}>
              <svg className="relation-lines" width="1040" height="700" viewBox="0 0 1040 700" aria-hidden="true">{visibleDomains.flatMap((domain) => domain.dependencies.map((targetId) => { const target = project.domains.find((item) => item.id === targetId); if (!target) return null; return <line key={`${domain.id}-${target.id}`} x1={domain.x + 180} y1={domain.y + 66} x2={target.x + 10} y2={target.y + 66} className={domain.status === 'shared' ? 'relation shared' : 'relation'} /> }))}</svg>
              {visibleDomains.map((domain) => <DomainCard key={domain.id} domain={domain} selected={domain.id === selectedId} onSelect={() => setSelectedId(domain.id)} />)}
            </div>
            {visibleDomains.length === 0 && <div className="empty-map"><Icon name="search" size={24} /><strong>찾는 도메인이 없습니다</strong><span>다른 검색어를 입력해보세요.</span></div>}
            <div className="map-hint"><span>두 손가락 이동</span><b>·</b><span>핀치 / 휠 확대</span><b>·</b><span>드래그 이동</span></div>
          </div>
        </div>
      </main>

      <aside className="inspector">
        <div className="inspector-head">
          <div><span className="eyebrow">SELECTED DOMAIN</span><h2>{selectedDomain?.name ?? '도메인 선택'}</h2></div>
          <button className="icon-button" onClick={() => setSelectedId('')} aria-label="선택 해제"><Icon name="close" size={18} /></button>
        </div>
        {selectedDomain ? (
          <>
            <p className="inspector-summary">{selectedDomain.summary}</p>
            <div className="confidence">
              <div><span>분석 신뢰도</span><strong>{selectedDomain.confidence}%</strong></div>
              <div className="confidence-track"><span style={{ width: `${selectedDomain.confidence}%`, background: selectedDomain.color }} /></div>
              <small>정적 근거 {selectedDomain.status === 'verified' ? '확인됨' : selectedDomain.status === 'shared' ? '공유 경계' : '후보'} · {selectedDomain.units} units</small>
            </div>
            <div className="inspector-section">
              <div className="section-title"><span>구성된 기능</span><b>{selectedDomain.features}</b></div>
              {selectedDomain.featureItems.length > 0 ? (
                <div className="fake-list">
                  {selectedDomain.featureItems.slice(0, 8).map((feature) => (
                    <div className="fake-row" key={feature.id}>
                      <span className="mini-icon" style={{ color: selectedDomain.color }}>↗</span>
                      <span>
                        <strong>{feature.name}</strong>
                        <small>{feature.summary?.trim() || (feature.entrypoints > 0 ? '외부 진입점' : '내부 서비스 로직')}</small>
                      </span>
                      <em>{featureKindLabel(feature.kind)}</em>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="muted-copy">구성된 기능이 없습니다.</p>
              )}
            </div>
            <div className="inspector-section">
              <div className="section-title"><span>연결된 도메인</span><b>{selectedDomain.dependencies.length}</b></div>
              {selectedDomain.dependencies.length > 0 ? (
                <div className="linked-domains">
                  {selectedDomain.dependencies.map((id) => {
                    const linked = project.domains.find((item) => item.id === id)
                    return linked ? (
                      <button key={id} onClick={() => setSelectedId(id)}>
                        <span className="linked-dot" style={{ background: linked.color }} />
                        {linked.name}
                        <span>↗</span>
                      </button>
                    ) : null
                  })}
                </div>
              ) : (
                <p className="muted-copy">직접 연결된 도메인이 없습니다.</p>
              )}
            </div>
            <div className="inspector-section evidence">
              <div className="section-title"><span>분석 근거</span><b>{selectedDomain.signals.length}</b></div>
              <div className="tag-list">{selectedDomain.signals.map((signal) => <span key={signal}>{signal}</span>)}</div>
            </div>
          </>
        ) : (
          <div className="inspector-empty">
            <div className="empty-ring">+</div>
            <strong>지도를 탐색해보세요</strong>
            <span>도메인 카드를 선택하면 코드 구조와 연결 관계를 확인할 수 있습니다.</span>
          </div>
        )}
      </aside>
    </div>
  )
}

export default App
