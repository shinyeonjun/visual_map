import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { analyzeProject, createProject, isDesktopRuntime, loadCodexModels, saveCodexSettings, selectProjectFolder } from './services/analysis'
import type { AnalysisProgress, CodexModel, CodexModelCatalog, Project } from './domain'
import { domainPalette } from './domain'
import { useCanvasViewport } from './hooks/useCanvasViewport'
import { AnalysisProgressPanel, Icon, SetupScreen } from './components/ui'
import { DomainCard } from './components/domain-card'
import { featureLabel } from './components/domain-label'
import './styles.css'

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
  const [query, setQuery] = useState('')
  const [isProjectMenuOpen, setProjectMenuOpen] = useState(false)
  const [isAnalyzing, setAnalyzing] = useState(false)
  const [notice, setNotice] = useState('')
  const [isCodexChecking, setCodexChecking] = useState(false)
  const [codexVersion, setCodexVersion] = useState('')
  const [codexExecutable, setCodexExecutable] = useState('codex')
  const [codexError, setCodexError] = useState('')
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
      const nextModel = resolveSelectedModel(selectedModelRef.current, catalog)
      setModel(nextModel)
      selectedModelRef.current = nextModel
      if (nextModel) {
        try {
          await saveCodexSettings({ model: nextModel, cliVersion: catalog.version, executable: catalog.executable })
        } catch (error) {
          setNotice(getErrorMessage(error, 'Codex CLI는 확인했지만 설정을 저장하지 못했습니다.'))
        }
      }
    } catch (error) {
      setCodexVersion('')
      setCodexError(getErrorMessage(error, 'Codex CLI를 확인하지 못했습니다.'))
    } finally {
      codexCheckingRef.current = false
      setCodexChecking(false)
    }
  }, [])

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

  async function handleAnalysis() {
    if (!project?.path || isAnalyzing || !codexVersion || !model) return
    setAnalyzing(true)
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
    setNotice(`${model}로 ${project.name}을 분석하는 중입니다…`)
    setProjects((current) => current.map((item) => item.id === project.id ? { ...item, state: 'running' } : item))
    try {
      await saveCodexSettings({ model, cliVersion: codexVersion, executable: codexExecutable })
      const response = await analyzeProject({ projectPath: project.path, enginePath: '', configPath: '', model })
      if (response.domains.length > 0) {
        setProjects((current) => current.map((item) => {
          if (item.id !== project.id) return item
          const domains = response.domains.map((suggestion, index) => {
            const existing = item.domains.find((domain) => domain.id === suggestion.domainId)
            if (existing) return { ...existing, name: suggestion.name, summary: suggestion.summary ?? existing.summary }
            return {
              id: suggestion.domainId,
              name: suggestion.name,
              summary: suggestion.summary ?? 'Codex가 코드 구조에서 식별한 비즈니스 책임입니다.',
              color: domainPalette[index % domainPalette.length],
              x: 70 + (index % 3) * 320,
              y: 70 + Math.floor(index / 3) * 220,
              units: 0,
              features: 0,
              entrypoints: 0,
              confidence: 0,
              status: 'candidate' as const,
              dependencies: [],
              signals: ['Codex semantic review'],
            }
          })
          return { ...item, state: 'ready', domains }
        }))
      } else {
        setProjects((current) => current.map((item) => item.id === project.id ? { ...item, state: 'ready' } : item))
      }
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

  async function handleModelChange(nextModel: string) {
    setModel(nextModel)
    selectedModelRef.current = nextModel
    if (!codexVersion || !nextModel) return
    try {
      await saveCodexSettings({ model: nextModel, cliVersion: codexVersion, executable: codexExecutable })
      setNotice(`${nextModel} 모델을 VisualMap 설정에 저장했습니다.`)
    } catch (error) {
      setNotice(getErrorMessage(error, '모델 설정을 저장하지 못했습니다.'))
    }
  }

  if (activeView === 'setup' || !project) {
    return <SetupScreen
      project={project}
      onModelChange={handleModelChange}
      isAnalyzing={isAnalyzing}
      analysisProgress={analysisProgress}
      isCodexChecking={isCodexChecking}
      codexVersion={codexVersion}
      models={models}
      model={model}
      codexError={codexError}
      notice={notice}
      onConnectProject={handleAddProject}
      onCheckCodex={handleCodexConnection}
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
          <button className="nav-item active"><Icon name="grid" /><span>도메인 지도</span><kbd>⌘ 1</kbd></button>
          <button className="nav-item"><Icon name="route" /><span>실행 흐름</span><kbd>⌘ 2</kbd></button>
          <button className="nav-item"><Icon name="layers" /><span>코드 탐색기</span><kbd>⌘ 3</kbd></button>
          <button className="nav-item"><Icon name="database" /><span>리소스</span><kbd>⌘ 4</kbd></button>
        </nav>
        <div className="sidebar-bottom">
          <div className="engine-card"><div className="engine-status"><i /> CODEX CLI</div><strong>의미 분석 엔진</strong><label className="engine-model"><span>MODEL</span><select value={model} onChange={(event) => void handleModelChange(event.target.value)}>{models.map((item) => <option key={item.slug} value={item.slug}>{item.displayName}</option>)}</select></label><small>{notice || '분석 결과는 캔버스에 표시됩니다.'}</small><button className="engine-run" onClick={handleAnalysis} disabled={isAnalyzing}><Icon name="spark" size={13} />{isAnalyzing ? '분석 중…' : 'Codex로 분석'}<span>↗</span></button></div>
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

      <aside className="inspector"><div className="inspector-head"><div><span className="eyebrow">SELECTED DOMAIN</span><h2>{selectedDomain?.name ?? '도메인 선택'}</h2></div><button className="icon-button" onClick={() => setSelectedId('')} aria-label="선택 해제"><Icon name="close" size={18} /></button></div>{selectedDomain ? <><p className="inspector-summary">{selectedDomain.summary}</p><div className="confidence"><div><span>분석 신뢰도</span><strong>{selectedDomain.confidence}%</strong></div><div className="confidence-track"><span style={{ width: `${selectedDomain.confidence}%`, background: selectedDomain.color }} /></div><small>정적 근거 {selectedDomain.status === 'verified' ? '확인됨' : '후보'} · {selectedDomain.units} units</small></div><div className="inspector-section"><div className="section-title"><span>구성된 기능</span><b>{selectedDomain.features}</b></div><div className="fake-list">{Array.from({ length: Math.min(4, selectedDomain.features) }, (_, index) => <div className="fake-row" key={index}><span className="mini-icon" style={{ color: selectedDomain.color }}>↗</span><span><strong>{featureLabel(selectedDomain, index)}</strong><small>{index % 2 === 0 ? '외부 진입점' : '내부 서비스 로직'}</small></span><em>{index % 2 === 0 ? 'API' : 'FUNC'}</em></div>)}</div></div><div className="inspector-section"><div className="section-title"><span>연결된 도메인</span><b>{selectedDomain.dependencies.length}</b></div>{selectedDomain.dependencies.length > 0 ? <div className="linked-domains">{selectedDomain.dependencies.map((id) => { const linked = project.domains.find((item) => item.id === id); return linked ? <button key={id} onClick={() => setSelectedId(id)}><span className="linked-dot" style={{ background: linked.color }} />{linked.name}<span>↗</span></button> : null })}</div> : <p className="muted-copy">직접 연결된 도메인이 없습니다.</p>}</div><div className="inspector-section evidence"><div className="section-title"><span>분석 근거</span><b>{selectedDomain.signals.length}</b></div><div className="tag-list">{selectedDomain.signals.map((signal) => <span key={signal}>{signal}</span>)}</div></div><button className="open-code-button"><Icon name="layers" size={16} />관련 코드 열기 <span>⌘ ↗</span></button></> : <div className="inspector-empty"><div className="empty-ring">+</div><strong>지도를 탐색해보세요</strong><span>도메인 카드를 선택하면 코드 구조와 연결 관계를 확인할 수 있습니다.</span></div>}</aside>
    </div>
  )
}

export default App
