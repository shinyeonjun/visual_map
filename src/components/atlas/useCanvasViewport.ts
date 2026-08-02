import { useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent, WheelEvent } from "react";
import { clamp } from "./atlasRelations";

type CanvasViewState = {
  zoom: number;
  left: number;
  top: number;
};

export function useCanvasViewport(mode: string) {
  const stageRef = useRef<HTMLDivElement | null>(null);
  const panRef = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const zoomRef = useRef(1);
  const viewStatesRef = useRef(new Map<string, CanvasViewState>());
  const [atlasZoom, setAtlasZoom] = useState(1);

  useLayoutEffect(() => {
    const saved = viewStatesRef.current.get(mode) ?? { zoom: 1, left: 0, top: 0 };
    zoomRef.current = saved.zoom;
    setAtlasZoom(saved.zoom);
    window.requestAnimationFrame(() => {
      if (!stageRef.current) return;
      stageRef.current.scrollLeft = saved.left;
      stageRef.current.scrollTop = saved.top;
    });
  }, [mode]);

  function startPan(event: PointerEvent<HTMLDivElement>) {
    const target = event.target instanceof Element ? event.target : null;
    if (event.button !== 0 || target?.closest("button, [role='button']")) {
      return;
    }
    const stage = stageRef.current;
    if (!stage) {
      return;
    }
    panRef.current = { x: event.clientX, y: event.clientY, left: stage.scrollLeft, top: stage.scrollTop };
    stage.setPointerCapture(event.pointerId);
    stage.classList.add("panning");
  }

  function movePan(event: PointerEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    const pan = panRef.current;
    if (!stage || !pan) {
      return;
    }
    stage.scrollLeft = pan.left - (event.clientX - pan.x);
    stage.scrollTop = pan.top - (event.clientY - pan.y);
  }

  function stopPan(event: PointerEvent<HTMLDivElement>) {
    if (!panRef.current) {
      return;
    }
    panRef.current = null;
    stageRef.current?.classList.remove("panning");
    try {
      stageRef.current?.releasePointerCapture(event.pointerId);
    } catch {
      // Pointer capture may already be released by the WebView.
    }
  }

  function handleWheel(event: WheelEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    if (!stage) {
      return;
    }
    if (!event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      stage.scrollLeft += event.deltaX;
      stage.scrollTop += event.deltaY;
      return;
    }
    event.preventDefault();
    zoomAtlas(event.deltaY < 0 ? 0.08 : -0.08, { clientX: event.clientX, clientY: event.clientY });
  }

  function zoomAtlas(delta: number, origin?: { clientX: number; clientY: number }) {
    setAtlasZoom((current) => {
      const next = clamp(current + delta, 0.55, 1.65);
      zoomRef.current = next;
      const stage = stageRef.current;
      if (origin && stage && next !== current) {
        const rect = stage.getBoundingClientRect();
        const x = origin.clientX - rect.left;
        const y = origin.clientY - rect.top;
        const ratio = next / current;
        window.requestAnimationFrame(() => {
          stage.scrollLeft = (stage.scrollLeft + x) * ratio - x;
          stage.scrollTop = (stage.scrollTop + y) * ratio - y;
          rememberCanvasView();
        });
      }
      viewStatesRef.current.set(mode, {
        zoom: next,
        left: stage?.scrollLeft ?? 0,
        top: stage?.scrollTop ?? 0,
      });
      return next;
    });
  }

  function resetAtlasView() {
    zoomRef.current = 1;
    setAtlasZoom(1);
    if (stageRef.current) {
      stageRef.current.scrollLeft = 0;
      stageRef.current.scrollTop = 0;
    }
    viewStatesRef.current.set(mode, { zoom: 1, left: 0, top: 0 });
  }

  function resetAtlasZoom() {
    zoomRef.current = 1;
    setAtlasZoom(1);
    const stage = stageRef.current;
    viewStatesRef.current.set(mode, {
      zoom: 1,
      left: stage?.scrollLeft ?? 0,
      top: stage?.scrollTop ?? 0,
    });
  }

  function fitCanvas() {
    const stage = stageRef.current;
    const surface = stage?.querySelector<HTMLElement>(".at-map-surface");
    if (!stage || !surface) {
      return;
    }
    const current = zoomRef.current;
    const bounds = surface.getBoundingClientRect();
    const widthRatio = (stage.clientWidth - 24) / Math.max(1, bounds.width);
    const heightRatio = (stage.clientHeight - 24) / Math.max(1, bounds.height);
    const next = clamp(current * Math.min(1, widthRatio, heightRatio), 0.55, 1.65);
    zoomRef.current = next;
    setAtlasZoom(next);
    stage.scrollLeft = 0;
    stage.scrollTop = 0;
    viewStatesRef.current.set(mode, { zoom: next, left: 0, top: 0 });
  }

  function focusCanvasNode(nodeId: string | null | undefined) {
    if (!nodeId) {
      return;
    }
    window.requestAnimationFrame(() => {
      const stage = stageRef.current;
      const target = Array.from(stage?.querySelectorAll<HTMLElement>("[data-atlas-node-id]") ?? [])
        .find((element) => element.dataset.atlasNodeId === nodeId);
      if (!stage || !target) {
        return;
      }
      const stageBounds = stage.getBoundingClientRect();
      const targetBounds = target.getBoundingClientRect();
      stage.scrollLeft += targetBounds.left - stageBounds.left - (stage.clientWidth - targetBounds.width) / 2;
      stage.scrollTop += targetBounds.top - stageBounds.top - (stage.clientHeight - targetBounds.height) / 2;
      rememberCanvasView();
    });
  }

  function rememberCanvasView() {
    const stage = stageRef.current;
    if (!stage) return;
    viewStatesRef.current.set(mode, {
      zoom: zoomRef.current,
      left: stage.scrollLeft,
      top: stage.scrollTop,
    });
  }

  return {
    stageRef,
    atlasZoom,
    startPan,
    movePan,
    stopPan,
    handleWheel,
    zoomAtlas,
    fitCanvas,
    focusCanvasNode,
    resetAtlasView,
    resetAtlasZoom,
    rememberCanvasView,
  };
}
