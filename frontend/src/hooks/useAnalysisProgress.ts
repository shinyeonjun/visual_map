import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import type { AnalysisProgress } from '../domain'
import { isDesktopRuntime } from '../services/analysis'

export function useAnalysisProgress() {
  const [analysisProgress, setAnalysisProgress] = useState<AnalysisProgress | null>(null)

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

  return { analysisProgress, setAnalysisProgress }
}
