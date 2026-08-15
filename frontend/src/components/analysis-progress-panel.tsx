import { useEffect, useState } from 'react'
import type { AnalysisProgress } from '../domain'

function formatElapsed(milliseconds: number): string {
  if (milliseconds < 1000) return '방금 시작'
  const seconds = Math.floor(milliseconds / 1000)
  if (seconds < 60) return `${seconds}초 경과`
  return `${Math.floor(seconds / 60)}분 ${seconds % 60}초 경과`
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

  return (
    <section className={`analysis-progress-panel ${floating ? 'floating' : ''} ${isError ? 'error' : ''}`} aria-live="polite">
      <div className="analysis-progress-heading">
        <div><span className="eyebrow">{isError ? 'ANALYSIS FAILED' : 'ANALYSIS IN PROGRESS'}</span><strong>{progress.label}</strong></div>
        <span className="analysis-progress-step">{progressText}</span>
      </div>
      <p>{progress.detail}</p>
      <div
        className={`analysis-progress-track ${progress.indeterminate ? 'is-indeterminate' : ''}`}
        role="progressbar"
        aria-label={progress.label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuetext={`${progressText} · ${progress.detail}`}
        {...(!progress.indeterminate && { 'aria-valuenow': percent })}
      >
        <span style={{ width: `${percent}%` }} />
        {progress.indeterminate && <i aria-hidden="true" />}
      </div>
      <div className="analysis-progress-meta">
        <span>
          {progress.phase === 'semantic' && hasChunks
            ? (progress.current != null && progress.total != null && progress.current < progress.total
              ? `청크 ${progress.current + 1}/${progress.total} — AI 응답 대기`
              : `${progress.current}/${progress.total} 청크 완료`)
            : progress.phase === 'semantic' ? 'AI 분석 준비 중' : '완료된 단계 기준'}
        </span>
        <span>{elapsedText}</span>
      </div>
    </section>
  )
}
