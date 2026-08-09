import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent, WheelEvent as ReactWheelEvent } from "react";

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

const MIN_SCALE = 0.35;
const MAX_SCALE = 2.2;
const GRID = 24;
/** Room left around the content when fitting, so nothing touches the edge. */
const FIT_PADDING = 64;

interface Viewport {
  scale: number;
  x: number;
  y: number;
}

export function useCanvasViewport(contentWidth: number, contentHeight: number) {
  const viewRef = useRef<HTMLDivElement | null>(null);
  const [view, setView] = useState<Viewport>({ scale: 1, x: 0, y: 0 });
  const [panning, setPanning] = useState(false);
  const dragRef = useRef<{ pointerX: number; pointerY: number; startX: number; startY: number } | null>(null);

  const fit = useCallback(() => {
    const element = viewRef.current;
    if (!element) return;
    const bounds = element.getBoundingClientRect();
    if (bounds.width === 0 || bounds.height === 0) return;
    const scale = clamp(
      Math.min((bounds.width - FIT_PADDING) / contentWidth, (bounds.height - FIT_PADDING) / contentHeight, 1),
    );
    setView({
      scale,
      x: (bounds.width - contentWidth * scale) / 2,
      y: (bounds.height - contentHeight * scale) / 2,
    });
  }, [contentHeight, contentWidth]);

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

  function onWheel(event: ReactWheelEvent<HTMLDivElement>) {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (event.ctrlKey || event.metaKey) {
      zoomAt(event.deltaY < 0 ? 1.12 : 1 / 1.12, event.clientX - bounds.left, event.clientY - bounds.top);
      return;
    }
    setView((current) => ({ ...current, x: current.x - event.deltaX, y: current.y - event.deltaY }));
  }

  return {
    viewRef,
    view,
    panning,
    fit,
    zoomIn: () => zoomBy(1.15),
    zoomOut: () => zoomBy(1 / 1.15),
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp, onWheel },
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
