import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { analyzeProject, checkClaudeCli, createProject, isDesktopRuntime, listSavedProjects, loadClaudeModels, loadCodexModels, loadProjectMap, saveAiSettings, selectProjectFolder } from './services/analysis'
import type { AiProvider, ClaudeModel, CodexModel, Project } from './domain'
import { useCanvasViewport } from './hooks/useCanvasViewport'
import { useAnalysisProgress } from './hooks/useAnalysisProgress'
import { useMapNavigation } from './hooks/useMapNavigation'
import { AnalysisProgressPanel, SetupScreen } from './components/ui'
import { MapSidebar } from './components/map/map-sidebar'
import { MapToolbar } from './components/map/map-toolbar'
import { MapCanvas } from './components/map/map-canvas'
import { MapInspector } from './components/map/map-inspector'
import { DOMAIN_CARD, FEATURE_CARD, gridCanvasSize, gridPosition } from './map-layout'
import { flowMapCanvasSize, layoutFlowGraph } from './flow-layout'
import { resolveSelectedModel } from './lib/ai-model'
import { getErrorMessage } from './lib/errors'
import { mergeAnalysisIntoProject, normalizeProjectPath, projectFromAnalysis, workspaceKeyFromPath } from './lib/project-mapper'
import './styles.css'

function App() {
  const [projects, setProjects] = useState<Project[]>([])
  const [projectId, setProjectId] = useState('')
  const [model, setModel] = useState('')
  const [models, setModels] = useState<CodexModel[]>([])
  const [claudeModels, setClaudeModels] = useState<ClaudeModel[]>([])
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
  const [activeView, setActiveView] = useState<'setup' | 'map'>('setup')
  const [selectedFlowId, setSelectedFlowId] = useState<string | null>(null)
  const { analysisProgress, setAnalysisProgress } = useAnalysisProgress()
  const navigation = useMapNavigation()
  const {
    mapLayer,
    setMapLayer,
    selectedId,
    setSelectedId,
    selectedFeatureId,
    setSelectedFeatureId,
    query,
    setQuery,
    openDomain,
    openFeature,
    goToDomains,
    goToFeatures,
    resetSelection,
  } = navigation

  const codexCheckingRef = useRef(false)
  const selectedModelRef = useRef(model)
  const savedClaudeModelRef = useRef('')
  selectedModelRef.current = model

  const project = projects.find((item) => item.id === projectId)
  const selectedDomain = project?.domains.find((domain) => domain.id === selectedId) ?? project?.domains[0]
  const selectedFeature = selectedDomain?.featureItems.find((feature) => feature.id === selectedFeatureId) ?? null
  const selectedFlow = selectedFeature?.flows.find((flow) => flow.id === selectedFlowId)
    ?? selectedFeature?.flows[0]
    ?? null

  useEffect(() => {
    setSelectedFlowId(selectedFeature?.flows[0]?.id ?? null)
  }, [selectedFeature?.id])

  const visibleDomains = useMemo(() => {
    if (!project) return []
    const normalizedQuery = query.trim().toLowerCase()
    if (!normalizedQuery) return project.domains
    return project.domains.filter((domain) => `${domain.name} ${domain.summary} ${domain.signals.join(' ')}`.toLowerCase().includes(normalizedQuery))
  }, [project, query])

  const visibleFeatures = useMemo(() => {
    if (!selectedDomain) return []
    const normalizedQuery = query.trim().toLowerCase()
    if (!normalizedQuery || mapLayer !== 'features') return selectedDomain.featureItems
    return selectedDomain.featureItems.filter((feature) => {
      const haystack = `${feature.name} ${feature.summary ?? ''} ${feature.kind}`
      return haystack.toLowerCase().includes(normalizedQuery)
    })
  }, [mapLayer, query, selectedDomain])

  const domainLayout = useMemo(
    () => visibleDomains.map((domain, index) => ({ domain, ...gridPosition(index, 3, DOMAIN_CARD.width, DOMAIN_CARD.height) })),
    [visibleDomains],
  )

  const canvasSize = useMemo(() => {
    if (mapLayer === 'features') {
      return gridCanvasSize(visibleFeatures.length, 4, FEATURE_CARD.width, FEATURE_CARD.height)
    }
    if (mapLayer === 'flow' && selectedFlow) {
      return flowMapCanvasSize([layoutFlowGraph(selectedFlow)])
    }
    return gridCanvasSize(visibleDomains.length, 3, DOMAIN_CARD.width, DOMAIN_CARD.height)
  }, [mapLayer, selectedFlow, visibleDomains.length, visibleFeatures.length])

  const flowViewportOptions = mapLayer === 'flow'
    ? { fitMode: 'flow' as const }
    : undefined
  const canvas = useCanvasViewport(canvasSize.width, canvasSize.height, flowViewportOptions)
  const cliReady = provider === 'claude' ? Boolean(claudeVersion) : Boolean(codexVersion)

  function switchProject(id: string) {
    const next = projects.find((item) => item.id === id)
    if (!next) return
    setProjectId(id)
    setSelectedId(next.domains[0]?.id ?? '')
    setSelectedFeatureId('')
    setMapLayer('domains')
    setProjectMenuOpen(false)
    setNotice('프로젝트 지도를 불러왔습니다.')
    setActiveView(next.domains.length > 0 ? 'map' : 'setup')
  }

  async function handleAddProject() {
    const path = await selectProjectFolder()
    if (!path) return

    const existing = projects.find((item) => normalizeProjectPath(item.path) === normalizeProjectPath(path))
    if (existing) {
      switchProject(existing.id)
      setNotice(existing.domains.length > 0 ? '이미 추가된 프로젝트 지도를 열었습니다.' : '이미 추가된 프로젝트입니다.')
      return
    }

    try {
      const response = await loadProjectMap(path)
      if (response.domains.length === 0) {
        throw new Error('저장된 분석 결과에서 도메인을 찾지 못했습니다.')
      }
      const summary = {
        projectPath: path,
        workspaceKey: workspaceKeyFromPath(response.workspacePath, `project-${Date.now()}`),
        updatedAtMs: Date.now(),
      }
      const next = projectFromAnalysis(summary, response)
      setProjects((current) => [...current, next])
      setProjectId(next.id)
      setSelectedId(next.domains[0]?.id ?? '')
      setSelectedFeatureId('')
      setMapLayer('domains')
      setQuery('')
      setActiveView('map')
      setNotice('저장된 분석 결과를 불러왔습니다.')
    } catch {
      const next = createProject(path)
      setProjects((current) => [...current, next])
      setProjectId(next.id)
      setSelectedId('')
      setSelectedFeatureId('')
      setMapLayer('domains')
      setActiveView('setup')
      setNotice('프로젝트가 추가되었습니다. 분석을 시작해 도메인을 찾아보세요.')
    }
  }

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
    void (async () => {
      try {
        const saved = await listSavedProjects()
        if (disposed || saved.length === 0) return

        const loaded = await Promise.all(saved.map(async (summary) => {
          try {
            const response = await loadProjectMap(summary.projectPath)
            return projectFromAnalysis(summary, response)
          } catch {
            return createProject(summary.projectPath, {
              id: summary.workspaceKey,
              updatedAtMs: summary.updatedAtMs,
            })
          }
        }))

        if (disposed) return
        setProjects(loaded)
        const preferred = loaded.find((item) => item.domains.length > 0) ?? loaded[0]
        setProjectId(preferred.id)
        setSelectedId(preferred.domains[0]?.id ?? '')
        setSelectedFeatureId('')
        setMapLayer('domains')
        setActiveView(preferred.domains.length > 0 ? 'map' : 'setup')
        setNotice(preferred.domains.length > 0 ? '저장된 분석 결과를 불러왔습니다.' : '저장된 프로젝트를 불러왔습니다.')
      } catch {
        if (!disposed) setNotice('저장된 프로젝트를 불러오지 못했습니다.')
      }
    })()
    return () => {
      disposed = true
    }
  }, [setMapLayer, setSelectedFeatureId, setSelectedId])

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
      setProjects((current) => current.map((item) => item.id === project.id ? mergeAnalysisIntoProject(item, response) : item))
      setProjectId(workspaceKeyFromPath(response.workspacePath, project.id))
      setNotice(response.workspacePath ? '분석이 끝났습니다. 워크스페이스에 저장했습니다.' : '분석이 끝났습니다. 캔버스의 도메인 지도를 확인하세요.')
      setMapLayer('domains')
      setSelectedId(response.domains[0]?.domainId ?? '')
      setSelectedFeatureId('')
      setQuery('')
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
    return (
      <SetupScreen
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
    )
  }

  return (
    <div className="app-shell">
      <MapSidebar
        project={project}
        projects={projects}
        mapLayer={mapLayer}
        selectedDomainId={selectedDomain?.id}
        selectedFeatureId={selectedFeatureId}
        isProjectMenuOpen={isProjectMenuOpen}
        provider={provider}
        model={model}
        models={models}
        claudeModels={claudeModels}
        notice={notice}
        isAnalyzing={isAnalyzing}
        cliReady={cliReady}
        onToggleProjectMenu={() => setProjectMenuOpen((open) => !open)}
        onSwitchProject={switchProject}
        onAddProject={handleAddProject}
        onGoToDomains={goToDomains}
        onOpenDomain={openDomain}
        onOpenFeature={openFeature}
        onProviderChange={handleProviderChange}
        onModelChange={handleModelChange}
        onAnalyze={handleAnalysis}
      />
      <main className="main-stage">
        {analysisProgress && (isAnalyzing || analysisProgress.phase === 'error') && (
          <AnalysisProgressPanel progress={analysisProgress} floating />
        )}
        <MapToolbar
          projectName={project.name}
          mapLayer={mapLayer}
          selectedDomain={selectedDomain}
          selectedFeature={selectedFeature}
          query={query}
          zoomPercent={Math.round(canvas.view.scale * 100)}
          onGoToDomains={goToDomains}
          onGoToFeatures={goToFeatures}
          onQueryChange={setQuery}
          onZoomIn={canvas.zoomIn}
          onZoomOut={canvas.zoomOut}
          onFit={canvas.fit}
        />
        <MapCanvas
          mapLayer={mapLayer}
          canvas={canvas}
          canvasSize={canvasSize}
          domainLayout={domainLayout}
          visibleDomains={visibleDomains}
          visibleFeatures={visibleFeatures}
          selectedDomain={selectedDomain}
          selectedDomainId={selectedId}
          selectedFeatureId={selectedFeatureId}
          selectedFeature={selectedFeature}
          selectedFlowId={selectedFlowId}
          onSelectFlow={setSelectedFlowId}
          onSelectDomain={setSelectedId}
          onOpenDomain={openDomain}
          onSelectFeature={setSelectedFeatureId}
          onOpenFeature={openFeature}
        />
      </main>
      <MapInspector
        project={project}
        mapLayer={mapLayer}
        selectedDomain={selectedDomain}
        selectedFeature={selectedFeature}
        selectedFeatureId={selectedFeatureId}
        onResetSelection={resetSelection}
        onOpenDomain={openDomain}
        onOpenFeature={openFeature}
        onSelectFeature={setSelectedFeatureId}
        onSelectDomain={setSelectedId}
        onGoToDomains={goToDomains}
      />
    </div>
  )
}

export default App
