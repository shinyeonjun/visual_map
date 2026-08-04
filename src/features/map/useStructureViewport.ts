import { useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent, WheelEvent } from "react";

type ViewState = { zoom: number; left: number; top: number };

const MIN_ZOOM = 0.4;
const MAX_ZOOM = 1.8;

export function useStructureViewport(viewKey: string) {
  const stageRef = useRef<HTMLDivElement | null>(null);
  const panRef = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const statesRef = useRef(new Map<string, ViewState>());
  const zoomRef = useRef(1);
  const [zoom, setZoom] = useState(1);

  useLayoutEffect(() => {
    const saved = statesRef.current.get(viewKey) ?? { zoom: 1, left: 0, top: 0 };
    zoomRef.current = saved.zoom;
    setZoom(saved.zoom);
    window.requestAnimationFrame(() => {
      const stage = stageRef.current;
      if (!stage) return;
      stage.scrollLeft = saved.left;
      stage.scrollTop = saved.top;
    });
  }, [viewKey]);

  function startPan(event: PointerEvent<HTMLDivElement>, enabled: boolean) {
    const target = event.target instanceof Element ? event.target : null;
    const stage = stageRef.current;
    if (!enabled || !stage || event.button !== 0 || target?.closest("button, summary, a, input, select")) return;
    panRef.current = { x: event.clientX, y: event.clientY, left: stage.scrollLeft, top: stage.scrollTop };
    stage.setPointerCapture(event.pointerId);
    stage.dataset.panning = "true";
  }

  function movePan(event: PointerEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    const pan = panRef.current;
    if (!stage || !pan) return;
    stage.scrollLeft = pan.left - (event.clientX - pan.x);
    stage.scrollTop = pan.top - (event.clientY - pan.y);
  }

  function stopPan(event: PointerEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    if (!stage || !panRef.current) return;
    panRef.current = null;
    delete stage.dataset.panning;
    remember();
    try {
      stage.releasePointerCapture(event.pointerId);
    } catch {
      // The WebView can release pointer capture when the cursor leaves the window.
    }
  }

  function handleWheel(event: WheelEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    if (!stage) return;
    event.preventDefault();
    if (event.ctrlKey || event.metaKey) {
      zoomBy(event.deltaY < 0 ? 0.1 : -0.1, { x: event.clientX, y: event.clientY });
      return;
    }
    stage.scrollLeft += event.deltaX || (event.shiftKey ? event.deltaY : 0);
    stage.scrollTop += event.shiftKey ? 0 : event.deltaY;
    remember();
  }

  function zoomBy(delta: number, origin?: { x: number; y: number }) {
    const current = zoomRef.current;
    const next = clamp(current + delta, MIN_ZOOM, MAX_ZOOM);
    if (next === current) return;
    const stage = stageRef.current;
    zoomRef.current = next;
    setZoom(next);
    if (stage && origin) {
      const rect = stage.getBoundingClientRect();
      const x = origin.x - rect.left;
      const y = origin.y - rect.top;
      const ratio = next / current;
      window.requestAnimationFrame(() => {
        stage.scrollLeft = (stage.scrollLeft + x) * ratio - x;
        stage.scrollTop = (stage.scrollTop + y) * ratio - y;
        remember();
      });
    } else {
      remember();
    }
  }

  function fitCanvas() {
    const stage = stageRef.current;
    const world = stage?.querySelector<HTMLElement>(".flow-world");
    if (!stage || !world) return;
    const current = zoomRef.current;
    const bounds = world.getBoundingClientRect();
    const next = clamp(
      current *
        Math.min(
          (stage.clientWidth - 72) / Math.max(1, bounds.width),
          (stage.clientHeight - 72) / Math.max(1, bounds.height),
          1,
        ),
      MIN_ZOOM,
      MAX_ZOOM,
    );
    zoomRef.current = next;
    setZoom(next);
    window.requestAnimationFrame(() => {
      stage.scrollLeft = Math.max(0, (stage.scrollWidth - stage.clientWidth) / 2);
      stage.scrollTop = Math.max(0, (stage.scrollHeight - stage.clientHeight) / 2);
      remember();
    });
  }

  function resetView() {
    zoomRef.current = 1;
    setZoom(1);
    const stage = stageRef.current;
    if (stage) {
      stage.scrollLeft = 0;
      stage.scrollTop = 0;
    }
    remember();
  }

  function remember() {
    const stage = stageRef.current;
    if (!stage) return;
    statesRef.current.set(viewKey, { zoom: zoomRef.current, left: stage.scrollLeft, top: stage.scrollTop });
  }

  return { stageRef, zoom, startPan, movePan, stopPan, handleWheel, zoomBy, fitCanvas, resetView, remember };
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
