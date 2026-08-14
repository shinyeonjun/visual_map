import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import type { AiProvider, AnalysisProgress, ClaudeModel, CodexModel, Project } from '../domain'

export type IconName = 'grid' | 'route' | 'database' | 'layers' | 'search' | 'plus' | 'spark' | 'folder' | 'chevron' | 'close'

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  const paths: Record<IconName, string> = {
    grid: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
    route: 'M5 5h14M5 12h14M5 19h14M8 3v4M16 10v4M10 17v4',
    database: 'M4 5c0-2 16-2 16 0v14c0 2-16 2-16 0V5Zm0 0c0 2 16 2 16 0M4 12c0 2 16 2 16 0',
    layers: 'm12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5M3 17l9 5 9-5',
    search: 'm20 20-4.5-4.5m2-5.5a7.5 7.5 0 1 1-15 0 7.5 7.5 0 0 1 15 0Z',
    plus: 'M12 5v14M5 12h14',
    spark: 'm12 3 1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3Zm7 13 .7 2.3L22 19l-2.3.7L19 22l-.7-2.3L16 19l2.3-.7L19 16Z',
    folder: 'M3 7h7l2 2h9v10H3V7Zm0 0V5h6l2 2',
    chevron: 'm8 10 4 4 4-4',
    close: 'm6 6 12 12M18 6 6 18',
  }
  return <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d={paths[name]} /></svg>
}

export function AnalysisProgressPanel({ progress, floating = false }: { progress: AnalysisProgress; floating?: boolean }) {
  const isError = progress.phase === 'error'
  const percent = Math.min(100, Math.max(0, progress.percent))
  const hasChunks = progress.current != null && progress.total != null && progress.total > 0
  const [displayElapsedMs, setDisplayElapsedMs] = useState(progress.elapsedMs)
  const progressText = hasChunks
    ? progress.phase === 'semantic' && progress.current != null && progress.total != null && progress.current < progress.total
      ? `AI 청크 ${progress.current + 1}/${progress.total} 처리 중`
      : `AI 청크 ${progress.current}/${progress.total}`
    : `${progress.step}/${progress.totalSteps} 단계`

  useEffect(() => {
    setDisplayElapsedMs(progress.elapsedMs)
    if (isError || progress.phase === 'complete') return
    const startedAt = Date.now() - progress.elapsedMs
    const timer = window.setInterval(() => setDisplayElapsedMs(Date.now() - startedAt), 1000)
    return () => window.clearInterval(timer)
  }, [isError, progress.elapsedMs, progress.phase, progress.step, progress.current, progress.percent, progress.indeterminate])

  const elapsedText = formatElapsed(displayElapsedMs)

  return <section className={`analysis-progress-panel ${floating ? 'floating' : ''} ${isError ? 'error' : ''}`} aria-live="polite">
    <div className="analysis-progress-heading">
      <div><span className="eyebrow">{isError ? 'ANALYSIS FAILED' : 'ANALYSIS IN PROGRESS'}</span><strong>{progress.label}</strong></div>
      <span className="analysis-progress-step">{progressText}</span>
    </div>
    <p>{progress.detail}</p>
    <div className={`analysis-progress-track ${progress.indeterminate ? 'is-indeterminate' : ''}`} role="progressbar" aria-label={progress.label} aria-valuemin={0} aria-valuemax={100} aria-valuetext={`${progressText} · ${progress.detail}`} {...(!progress.indeterminate && { 'aria-valuenow': percent })}>
      <span style={{ width: `${percent}%` }} />
      {progress.indeterminate && <i aria-hidden="true" />}
    </div>
    <div className="analysis-progress-meta"><span>{progress.phase === 'semantic' && hasChunks ? (progress.current != null && progress.total != null && progress.current < progress.total ? `청크 ${progress.current + 1}/${progress.total} — AI 응답 대기` : `${progress.current}/${progress.total} 청크 완료`) : progress.phase === 'semantic' ? 'AI 분석 준비 중' : '완료된 단계 기준'}</span><span>{elapsedText}</span></div>
  </section>
}

function formatElapsed(milliseconds: number): string {
  if (milliseconds < 1000) return '방금 시작'
  const seconds = Math.floor(milliseconds / 1000)
  if (seconds < 60) return `${seconds}초 경과`
  return `${Math.floor(seconds / 60)}분 ${seconds % 60}초 경과`
}

export function SetupScreen({
  project,
  provider,
  onProviderChange,
  model,
  onModelChange,
  isAnalyzing,
  analysisProgress,
  isCodexChecking,
  codexVersion,
  models,
  claudeModels,
  codexError,
  isClaudeChecking,
  claudeVersion,
  claudeError,
  notice,
  onConnectProject,
  onCheckCodex,
  onCheckClaude,
  onAnalyze,
}: {
  project?: Project
  provider: AiProvider
  onProviderChange: (value: AiProvider) => void
  model: string
  onModelChange: (value: string) => void
  isAnalyzing: boolean
  analysisProgress: AnalysisProgress | null
  isCodexChecking: boolean
  codexVersion: string
  models: CodexModel[]
  claudeModels: ClaudeModel[]
  codexError: string
  isClaudeChecking: boolean
  claudeVersion: string
  claudeError: string
  notice: string
  onConnectProject: () => void
  onCheckCodex: () => void
  onCheckClaude: () => void
  onAnalyze: () => void
}) {
  const cliReady = provider === 'claude' ? Boolean(claudeVersion) : Boolean(codexVersion)
  const cliChecking = provider === 'claude' ? isClaudeChecking : isCodexChecking
  const cliError = provider === 'claude' ? claudeError : codexError
  const canAnalyze = Boolean(project && cliReady && model && !isAnalyzing)
  const providerLabel = provider === 'claude' ? 'Claude' : 'Codex'

  const modelOptions = provider === 'claude'
    ? claudeModels.map((item) => ({ slug: item.slug, displayName: item.displayName }))
    : models.map((item) => ({ slug: item.slug, displayName: item.displayName }))

  return (
    <div className="setup-shell">
      <header className="setup-header">
        <div className="brand"><span className="brand-mark"><span /></span><span>VisualMap</span><em>β</em></div>
        <span className="setup-header-note">CODEBASE VISUAL MAP</span>
      </header>
      <main className="setup-main">
        <div className="setup-intro">
          <span className="eyebrow">START A NEW MAP</span>
          <h1>코드베이스를 연결하세요</h1>
          <p>프로젝트와 AI CLI를 연결하면<br />코드 구조에서 비즈니스 도메인을 찾아 지도로 보여줍니다.</p>
        </div>

        <section className="setup-card" aria-label="분석 설정">
          <SetupStep number="01" title="프로젝트 연결" complete={Boolean(project)}>
            {project ? <div className="setup-connected"><strong>{project.name}</strong><span>{project.path}</span></div> : <p className="setup-placeholder">분석할 로컬 프로젝트 폴더를 선택하세요.</p>}
            <button className="setup-secondary" onClick={onConnectProject}><Icon name="folder" size={15} />{project ? '변경' : '폴더 선택'}</button>
          </SetupStep>

          <SetupStep number="02" title="AI 엔진 선택" complete={cliReady}>
            <div className="provider-toggle">
              <button className={`provider-option ${provider === 'codex' ? 'active' : ''}`} onClick={() => onProviderChange('codex')}>Codex</button>
              <button className={`provider-option ${provider === 'claude' ? 'active' : ''}`} onClick={() => onProviderChange('claude')}>Claude</button>
            </div>
            {cliReady
              ? <div className="setup-connected"><strong>{providerLabel} CLI 준비됨</strong><span>{provider === 'claude' ? claudeVersion : codexVersion}</span></div>
              : <p className="setup-placeholder">설치된 {providerLabel} CLI를 확인합니다.</p>}
            <button className="setup-secondary" onClick={provider === 'claude' ? onCheckClaude : onCheckCodex} disabled={cliChecking}><Icon name="spark" size={14} />{cliChecking ? '확인 중…' : cliReady ? '다시 확인' : '연결 확인'}</button>
          </SetupStep>

          <SetupStep number="03" title="분석 모델" complete={Boolean(model)}>
            <div className="setup-model-copy"><strong>도메인 이름 생성에 사용할 모델</strong><span>이 설정은 분석을 시작할 때 적용됩니다.</span></div>
            <select className="setup-model" value={model} onChange={(event) => void onModelChange(event.target.value)} aria-label="분석 모델 선택" disabled={modelOptions.length === 0}>
              {modelOptions.length === 0 && <option value="">먼저 CLI를 연결하세요</option>}
              {modelOptions.map((item) => <option key={item.slug} value={item.slug}>{item.displayName}</option>)}
            </select>
          </SetupStep>

          {analysisProgress && (isAnalyzing || analysisProgress.phase === 'error') && <AnalysisProgressPanel progress={analysisProgress} />}
          {cliError && <p className="setup-error">{cliError}</p>}
          {notice && !cliError && <p className="setup-notice">{notice}</p>}
          <button className="setup-primary" onClick={onAnalyze} disabled={!canAnalyze}>
            <Icon name="spark" size={15} />
            {isAnalyzing ? '프로젝트 분석 중…' : '분석 시작'}
            <span>↗</span>
          </button>
          <p className="setup-footnote">정적 분석 후 {providerLabel}가 비즈니스 도메인 이름만 생성합니다.</p>
        </section>
      </main>
    </div>
  )
}

function SetupStep({ number, title, complete, children }: { number: string; title: string; complete: boolean; children: ReactNode }) {
  return <div className={`setup-step ${complete ? 'complete' : ''}`}><div className="setup-step-marker"><span>{complete ? '✓' : number}</span></div><div className="setup-step-body"><div className="setup-step-title"><strong>{title}</strong>{complete && <small>완료</small>}</div>{children}</div></div>
}
