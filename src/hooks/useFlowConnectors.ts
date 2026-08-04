import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";

export type ConnectorRequest = {
  id: string;
  from: string;
  to: string;
  tone: string;
  label: string;
};

export type Connector = ConnectorRequest & { path: string; midX: number; midY: number };

type Rect = { left: number; top: number; right: number; bottom: number; cx: number; cy: number };

/**
 * Orthogonal connectors between boxes, measured from the DOM.
 *
 * Boxes nest and grow when the reader opens one, so their positions cannot be
 * computed up front the way a fixed grid can. The boxes lay themselves out in
 * normal flow, this measures where they landed, and the arrows are drawn on an
 * overlay afterwards. Anything that changes the layout — expanding a box, the
 * inspector opening, a window resize — re-measures.
 */
export function useFlowConnectors(
  canvasRef: RefObject<HTMLElement | null>,
  requests: ConnectorRequest[],
  /** Bump to force a re-measure after a change the observers cannot see. */
  revision: unknown,
): { connectors: Connector[]; width: number; height: number } {
  const [state, setState] = useState<{ connectors: Connector[]; width: number; height: number }>({
    connectors: [],
    width: 0,
    height: 0,
  });

  /*
    Read through a ref, and key the effect on the request *contents*.

    Depending on the array identity meant that a parent re-rendering with a
    fresh `areas` array gave `measure` a new identity, which re-ran the effect,
    which called setState, which re-rendered — a loop that never settled.
  */
  const requestsRef = useRef(requests);
  requestsRef.current = requests;
  const signature = useMemo(
    () => requests.map((request) => `${request.from}>${request.to}:${request.tone}`).join("|"),
    [requests],
  );

  const measure = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const origin = canvas.getBoundingClientRect();
    const rects = new Map<string, Rect>();
    for (const element of canvas.querySelectorAll<HTMLElement>("[data-flow-id]")) {
      const box = element.getBoundingClientRect();
      const left = box.left - origin.left + canvas.scrollLeft;
      const top = box.top - origin.top + canvas.scrollTop;
      rects.set(element.dataset.flowId ?? "", {
        left,
        top,
        right: left + box.width,
        bottom: top + box.height,
        cx: left + box.width / 2,
        cy: top + box.height / 2,
      });
    }

    const connectors: Connector[] = [];
    for (const request of requestsRef.current) {
      const from = rects.get(request.from);
      const to = rects.get(request.to);
      if (!from || !to || request.from === request.to) continue;
      connectors.push({ ...request, ...route(from, to) });
    }

    const width = Math.max(canvas.scrollWidth, origin.width);
    const height = Math.max(canvas.scrollHeight, origin.height);
    // Bail out when nothing moved, so a measurement can never feed a re-render.
    setState((previous) =>
      previous.width === width &&
      previous.height === height &&
      previous.connectors.length === connectors.length &&
      previous.connectors.every(
        (item, index) => item.path === connectors[index]?.path && item.id === connectors[index]?.id,
      )
        ? previous
        : { connectors, width, height },
    );
  }, [canvasRef]);

  useEffect(() => {
    measure();
    const canvas = canvasRef.current;
    if (!canvas || typeof ResizeObserver === "undefined") return;

    // A nested box changing height moves every sibling below it, so observe the
    // boxes themselves and not only the canvas.
    const observer = new ResizeObserver(() => measure());
    observer.observe(canvas);
    for (const element of canvas.querySelectorAll<HTMLElement>("[data-flow-id]")) {
      observer.observe(element);
    }
    return () => observer.disconnect();
  }, [canvasRef, measure, revision, signature]);

  return state;
}

/**
 * A path that leaves one box and enters the other on facing edges, turning at
 * right angles. Diagonal lines across a board of rectangles read as noise; the
 * elbow says "this leaves here and arrives there".
 */
function route(from: Rect, to: Rect): { path: string; midX: number; midY: number } {
  const gapX = to.left - from.right;
  const gapXReverse = from.left - to.right;

  if (gapX >= 24) {
    const midX = from.right + gapX / 2;
    return {
      path: `M ${from.right} ${from.cy} H ${midX} V ${to.cy} H ${to.left}`,
      midX,
      midY: (from.cy + to.cy) / 2,
    };
  }
  if (gapXReverse >= 24) {
    const midX = to.right + gapXReverse / 2;
    return {
      path: `M ${from.left} ${from.cy} H ${midX} V ${to.cy} H ${to.right}`,
      midX,
      midY: (from.cy + to.cy) / 2,
    };
  }

  // Stacked vertically, or overlapping horizontally: go out of the bottom.
  const startY = from.cy <= to.cy ? from.bottom : from.top;
  const endY = from.cy <= to.cy ? to.top : to.bottom;
  const midY = (startY + endY) / 2;
  return {
    path: `M ${from.cx} ${startY} V ${midY} H ${to.cx} V ${endY}`,
    midX: (from.cx + to.cx) / 2,
    midY,
  };
}
