import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

/**
 * Pan and zoom for the map.
 *
 * A scroll box would be simpler, but scrollbars say "this content has an
 * edge" and the map does not: areas sit where the reader put them, which can
 * be anywhere. Dragging the ground is also how every other canvas works, so
 * it needs no explaining.
 *
 * The dot grid is painted on the viewport rather than the stage and offset by
 * the same translation. Painting it on the stage would scale the dots along
 * with the content and turn them into stripes at low zoom.
 */

/** Large repositories must still be able to fit the complete responsibility map. */
const MIN_SCALE = 0.05;
const MAX_SCALE = 2.2;
/** Small maps should use the available canvas instead of floating as an unreadable island. */
const MAX_FIT_SCALE = 1.2;
const GRID = 24;
/** Room left around the content when fitting, so nothing touches the edge. */
const FIT_PADDING = 64;

/*
  Wheel deltas are not a single unit. A precision trackpad reports pixels, a
  mouse wheel usually reports lines, and some devices report pages. Converting
  everything to pixels first is what makes one gesture feel the same on every
  device instead of moving three pixels per notch.
*/
const LINE_DELTA_PX = 16;
const PAGE_DELTA_PX = 400;
/**
 * Zoom is exponential in the scroll delta, so a pinch covers the same ratio
 * whether the map is at 5% or 200%. Linear steps feel fast when zoomed out and
 * unusably slow when zoomed in.
 */
const ZOOM_SENSITIVITY = 0.0075;
/**
 * A trackpad pinch arrives as many small deltas; a mouse wheel arrives as one
 * large notch. Capping the per-event ratio keeps the first smooth without
 * letting the second jump several zoom levels at once.
 */
const MAX_ZOOM_STEP = 1.25;

interface Viewport {
  scale: number;
  x: number;
  y: number;
}

export function useCanvasViewport(contentWidth: number, contentHeight: number, minFitScale = MIN_SCALE) {
  const viewRef = useRef<HTMLDivElement | null>(null);
  const [view, setView] = useState<Viewport>({ scale: 1, x: 0, y: 0 });
  const [panning, setPanning] = useState(false);
  const dragRef = useRef<{ pointerX: number; pointerY: number; startX: number; startY: number } | null>(null);

  const fit = useCallback(() => {
    const element = viewRef.current;
    if (!element) return;
    const bounds = element.getBoundingClientRect();
    if (bounds.width === 0 || bounds.height === 0) return;
    /*
      Fitting is only worth it while the result can still be read. A view whose
      job is names and line numbers would rather overflow and be panned than
      shrink its own text out of legibility, so it raises this floor.
    */
    const scale = clamp(
      Math.max(
        minFitScale,
        Math.min(
          (bounds.width - FIT_PADDING) / contentWidth,
          (bounds.height - FIT_PADDING) / contentHeight,
          MAX_FIT_SCALE,
        ),
      ),
    );
    setView({
      scale,
      x: (bounds.width - contentWidth * scale) / 2,
      y: (bounds.height - contentHeight * scale) / 2,
    });
  }, [contentHeight, contentWidth, minFitScale]);

  useEffect(() => {
    fit();
    const element = viewRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => fit());
    observer.observe(element);
    return () => observer.disconnect();
  }, [fit]);

  /** Zoom about a point in viewport coordinates, so it stays put on screen. */
  const zoomAt = useCallback((factor: number, originX: number, originY: number) => {
    setView((current) => {
      const scale = clamp(current.scale * factor);
      if (scale === current.scale) return current;
      const ratio = scale / current.scale;
      return {
        scale,
        x: originX - (originX - current.x) * ratio,
        y: originY - (originY - current.y) * ratio,
      };
    });
  }, []);

  const zoomBy = useCallback(
    (factor: number) => {
      const bounds = viewRef.current?.getBoundingClientRect();
      zoomAt(factor, (bounds?.width ?? 0) / 2, (bounds?.height ?? 0) / 2);
    },
    [zoomAt],
  );

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    // Controls sitting on the canvas are not the ground.
    if (event.target instanceof Element && event.target.closest("button, a, input, textarea")) return;
    dragRef.current = { pointerX: event.clientX, pointerY: event.clientY, startX: view.x, startY: view.y };
    setPanning(true);
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    const drag = dragRef.current;
    if (!drag) return;
    setView((current) => ({
      ...current,
      x: drag.startX + (event.clientX - drag.pointerX),
      y: drag.startY + (event.clientY - drag.pointerY),
    }));
  }

  function onPointerUp(event: ReactPointerEvent<HTMLDivElement>) {
    if (!dragRef.current) return;
    dragRef.current = null;
    setPanning(false);
    try {
      event.currentTarget.releasePointerCapture(event.pointerId);
    } catch {
      // The WebView can drop pointer capture when the cursor leaves the window.
    }
  }

  /*
    WebView2 first offers a precision-touchpad pinch to the page as a synthetic
    ctrl+wheel event. Tauri has to enable that native gesture for the event to
    exist at all; once it does, this non-passive listener consumes it before
    WebView2 falls back to scaling the whole page.

    Listen at the window boundary instead of only on the canvas. A pinch over
    the canvas becomes map zoom, while a pinch over a sidebar is consumed and
    does nothing. That keeps the desktop shell at 100% page scale.
  */
  useEffect(() => {
    const element = viewRef.current;
    if (!element) return;
    const canvasElement: HTMLDivElement = element;

    function onWheel(event: WheelEvent) {
      const overCanvas = event.composedPath().includes(canvasElement);
      const pageZoomGesture = event.ctrlKey || event.metaKey;

      if (!overCanvas) {
        if (pageZoomGesture) event.preventDefault();
        return;
      }

      event.preventDefault();
      const bounds = canvasElement.getBoundingClientRect();
      const { x: deltaX, y: deltaY } = wheelDeltaInPixels(event);
      // A trackpad pinch reaches the page as ctrl+wheel; so does ctrl+scroll.
      if (pageZoomGesture) {
        zoomAt(zoomStep(deltaY), event.clientX - bounds.left, event.clientY - bounds.top);
        return;
      }
      setView((current) => ({ ...current, x: current.x - deltaX, y: current.y - deltaY }));
    }

    function onKeyDown(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      if (isPageZoomKey(event.key)) event.preventDefault();
    }

    window.addEventListener("wheel", onWheel, { passive: false, capture: true });
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
      window.removeEventListener("wheel", onWheel, true);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [zoomAt]);

  return {
    viewRef,
    view,
    panning,
    fit,
    zoomIn: () => zoomBy(1.15),
    zoomOut: () => zoomBy(1 / 1.15),
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp },
    /** Keeps the dots a constant size while still moving with the content. */
    gridStyle: {
      backgroundSize: `${GRID * view.scale}px ${GRID * view.scale}px`,
      backgroundPosition: `${view.x}px ${view.y}px`,
    },
    stageStyle: {
      width: contentWidth,
      height: contentHeight,
      transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
    },
  };
}

function clamp(value: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));
}

/** Reports every wheel device in the same unit so one gesture feels the same. */
function wheelDeltaInPixels(event: WheelEvent): { x: number; y: number } {
  const unit =
    event.deltaMode === 1 /* DOM_DELTA_LINE */
      ? LINE_DELTA_PX
      : event.deltaMode === 2 /* DOM_DELTA_PAGE */
        ? PAGE_DELTA_PX
        : 1;
  return { x: event.deltaX * unit, y: event.deltaY * unit };
}

/** Scroll delta to a zoom ratio: proportional, direction-correct, bounded. */
function zoomStep(deltaY: number): number {
  const ratio = Math.exp(-deltaY * ZOOM_SENSITIVITY);
  return Math.min(MAX_ZOOM_STEP, Math.max(1 / MAX_ZOOM_STEP, ratio));
}

/** WebView page-zoom shortcuts must not resize the desktop application shell. */
function isPageZoomKey(key: string): boolean {
  return key === "+" || key === "=" || key === "-" || key === "_" || key === "0";
}
