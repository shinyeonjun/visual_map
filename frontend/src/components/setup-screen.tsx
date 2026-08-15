import type { ReactNode } from 'react'
import type { AiProvider, AnalysisProgress, ClaudeModel, CodexModel, Project } from '../domain'
import { AnalysisProgressPanel } from './analysis-progress-panel'
import { Icon } from './icon'

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
  return (
    <div className={`setup-step ${complete ? 'complete' : ''}`}>
      <div className="setup-step-marker"><span>{complete ? '✓' : number}</span></div>
      <div className="setup-step-body">
        <div className="setup-step-title"><strong>{title}</strong>{complete && <small>완료</small>}</div>
        {children}
      </div>
    </div>
  )
}
