import { useCallback, useEffect, useRef, useState } from 'react'
import type { PointerEvent as ReactPointerEvent } from 'react'

type Viewport = { scale: number; x: number; y: number }

export type ViewportFitOptions = {
  minFitScale?: number
  fitMode?: 'both' | 'width' | 'flow'
  alignY?: 'center' | 'top'
}

const MIN_SCALE = 0.05
const MAX_SCALE = 2.2
const MAX_FIT_SCALE = 1.2
const MAX_ZOOM_STEP = 1.25
const ZOOM_SENSITIVITY = 0.0075
const LINE_DELTA_PX = 16
const PAGE_DELTA_PX = 400
const FIT_PADDING = 64
const GRID_SIZE = 29
const FLOW_READABLE_MIN = 0.78
const FLOW_READABLE_MAX = 1.05

export function useCanvasViewport(
  contentWidth: number,
  contentHeight: number,
  options: ViewportFitOptions = {},
) {
  const minFitScale = options.minFitScale ?? MIN_SCALE
  const fitMode = options.fitMode ?? 'both'
  const alignY = options.alignY ?? 'center'
  const viewRef = useRef<HTMLDivElement | null>(null)
  const [canvasReady, setCanvasReady] = useState(false)
  const [view, setView] = useState<Viewport>({ scale: 1, x: 0, y: 0 })
  const dragRef = useRef<{ pointerX: number; pointerY: number; startX: number; startY: number } | null>(null)
  const spaceRef = useRef(false)
  const [spaceHeld, setSpaceHeld] = useState(false)

  const fit = useCallback(() => {
    const element = viewRef.current
    if (!element) return
    const bounds = element.getBoundingClientRect()
    if (bounds.width === 0 || bounds.height === 0) return
    const widthScale = (bounds.width - FIT_PADDING) / contentWidth
    const heightScale = (bounds.height - FIT_PADDING) / contentHeight

    if (fitMode === 'flow') {
      const fitsBoth = Math.min(widthScale, heightScale)
      if (fitsBoth >= FLOW_READABLE_MIN) {
        const scale = clamp(Math.min(fitsBoth, FLOW_READABLE_MAX))
        setView({
          scale,
          x: (bounds.width - contentWidth * scale) / 2,
          y: (bounds.height - contentHeight * scale) / 2,
        })
        return
      }

      const scale = clamp(widthScale < FLOW_READABLE_MIN ? widthScale : Math.min(widthScale, FLOW_READABLE_MAX))
      setView({
        scale,
        x: (bounds.width - contentWidth * scale) / 2,
        y: FIT_PADDING / 2,
      })
      return
    }

    const scale = clamp(
      Math.max(
        minFitScale,
        Math.min(
          fitMode === 'width' ? widthScale : Math.min(widthScale, heightScale),
          MAX_FIT_SCALE,
        ),
      ),
    )
    const y = alignY === 'top'
      ? FIT_PADDING / 2
      : (bounds.height - contentHeight * scale) / 2
    setView({ scale, x: (bounds.width - contentWidth * scale) / 2, y })
  }, [alignY, contentHeight, contentWidth, fitMode, minFitScale])

  useEffect(() => {
    fit()
    const element = viewRef.current
    if (!element || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(() => fit())
    observer.observe(element)
    return () => observer.disconnect()
  }, [fit])

  const zoomAt = useCallback((factor: number, originX: number, originY: number) => {
    setView((current) => {
      const scale = clamp(current.scale * factor)
      if (scale === current.scale) return current
      const ratio = scale / current.scale
      return { scale, x: originX - (originX - current.x) * ratio, y: originY - (originY - current.y) * ratio }
    })
  }, [])

  const zoomBy = useCallback((factor: number) => {
    const bounds = viewRef.current?.getBoundingClientRect()
    zoomAt(factor, (bounds?.width ?? 0) / 2, (bounds?.height ?? 0) / 2)
  }, [zoomAt])

  const setViewRef = useCallback((node: HTMLDivElement | null) => {
    viewRef.current = node
    setCanvasReady(node !== null)
  }, [])

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    const isLeftButton = event.button === 0
    const isMiddleButton = event.button === 1
    if (!isLeftButton && !isMiddleButton) return

    if (isLeftButton && !spaceRef.current) {
      if (event.target instanceof Element && event.target.closest('button, a, input, textarea')) return
      if (event.target instanceof Element && event.target.closest('.domain-card, .feature-card')) return
    }

    dragRef.current = { pointerX: event.clientX, pointerY: event.clientY, startX: view.x, startY: view.y }
    event.currentTarget.setPointerCapture(event.pointerId)
    event.preventDefault()
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    const drag = dragRef.current
    if (!drag) return
    setView((current) => ({ ...current, x: drag.startX + event.clientX - drag.pointerX, y: drag.startY + event.clientY - drag.pointerY }))
  }

  function onPointerUp(event: ReactPointerEvent<HTMLDivElement>) {
    if (!dragRef.current) return
    dragRef.current = null
    try {
      event.currentTarget.releasePointerCapture(event.pointerId)
    } catch {
      // WebView가 창 밖에서 pointer capture를 먼저 해제할 수 있다.
    }
  }

  useEffect(() => {
    if (!canvasReady) return

    /*
      WebView2는 precision touchpad pinch를 synthetic ctrl+wheel로 변환한다.
      zoomHotkeysEnabled가 켜져 있어야 DOM까지 도달한다.
      window capture에서 소비하지 않으면 WebView2가 페이지 전체를 확대한다.
    */
    function onWheel(event: WheelEvent) {
      const canvas = viewRef.current
      if (!canvas) return

      const overCanvas = event.composedPath().includes(canvas)
      const pageZoomGesture = event.ctrlKey || event.metaKey

      if (!overCanvas) {
        if (pageZoomGesture) event.preventDefault()
        return
      }

      event.preventDefault()
      const bounds = canvas.getBoundingClientRect()
      const { x: deltaX, y: deltaY } = wheelDeltaInPixels(event)

      if (pageZoomGesture) {
        zoomAt(zoomStep(deltaY), event.clientX - bounds.left, event.clientY - bounds.top)
        return
      }

      setView((current) => ({ ...current, x: current.x - deltaX, y: current.y - deltaY }))
    }

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === ' ' && !event.repeat) {
        const tag = (event.target as Element)?.tagName
        if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
        event.preventDefault()
        spaceRef.current = true
        setSpaceHeld(true)
      }
      if ((event.ctrlKey || event.metaKey) && !event.altKey && isPageZoomKey(event.key)) {
        event.preventDefault()
      }
    }

    function onKeyUp(event: KeyboardEvent) {
      if (event.key === ' ') {
        spaceRef.current = false
        setSpaceHeld(false)
      }
    }

    window.addEventListener('wheel', onWheel, { passive: false, capture: true })
    window.addEventListener('keydown', onKeyDown, { capture: true })
    window.addEventListener('keyup', onKeyUp, { capture: true })
    return () => {
      window.removeEventListener('wheel', onWheel, true)
      window.removeEventListener('keydown', onKeyDown, true)
      window.removeEventListener('keyup', onKeyUp, true)
    }
  }, [canvasReady, zoomAt])

  return {
    viewRef: setViewRef,
    view,
    fit,
    spaceHeld,
    zoomIn: () => zoomBy(1.15),
    zoomOut: () => zoomBy(1 / 1.15),
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp },
    gridStyle: {
      backgroundSize: `${GRID_SIZE * view.scale}px ${GRID_SIZE * view.scale}px`,
      backgroundPosition: `${view.x}px ${view.y}px`,
    },
    stageStyle: { width: contentWidth, height: contentHeight, transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})` },
  }
}

function clamp(value: number) {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value))
}

function wheelDeltaInPixels(event: WheelEvent) {
  const unit = event.deltaMode === 1 ? LINE_DELTA_PX : event.deltaMode === 2 ? PAGE_DELTA_PX : 1
  return { x: event.deltaX * unit, y: event.deltaY * unit }
}

function zoomStep(deltaY: number) {
  const ratio = Math.exp(-deltaY * ZOOM_SENSITIVITY)
  return Math.min(MAX_ZOOM_STEP, Math.max(1 / MAX_ZOOM_STEP, ratio))
}

function isPageZoomKey(key: string) {
  return key === '+' || key === '=' || key === '-' || key === '_' || key === '0'
}
