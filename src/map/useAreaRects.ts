import { useLayoutEffect, useState } from "react";
import type { RefObject } from "react";
import type { MapDetail } from "./detail";
import { rectMapsEqual } from "./layout";
import type { Rect } from "./layout";
import { flattenAreas } from "./types";
import type { MapView } from "./types";

/**
 * Where every area actually landed.
 *
 * Areas size themselves from their contents, so a relation cannot be routed
 * from stored coordinates alone — an area holding three members is a different
 * shape from one holding twelve. Measured after layout and re-measured when
 * anything resizes.
 */
export function useAreaRects(stageRef: RefObject<HTMLDivElement | null>, view: MapView, detail: MapDetail) {
  const [rects, setRects] = useState<Map<string, Rect>>(new Map());
  const signature =
    `${detail}:` +
    flattenAreas(view.areas)
      .map((area) => area.id)
      .join("|");

  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    let animationFrame: number | null = null;
    const measure = () => {
      const origin = stage.getBoundingClientRect();
      const scale = origin.width / stage.offsetWidth || 1;
      const next = new Map<string, Rect>();
      for (const element of stage.querySelectorAll<HTMLElement>("[data-area-id]")) {
        const id = element.dataset.areaId;
        if (!id) continue;
        const box = element.getBoundingClientRect();
        next.set(id, {
          x: (box.left - origin.left) / scale,
          y: (box.top - origin.top) / scale,
          width: box.width / scale,
          height: box.height / scale,
        });
      }
      setRects((current) => (rectMapsEqual(current, next) ? current : next));
    };

    const scheduleMeasure = () => {
      if (typeof requestAnimationFrame === "undefined") {
        measure();
        return;
      }
      if (animationFrame !== null) return;
      animationFrame = requestAnimationFrame(() => {
        animationFrame = null;
        measure();
      });
    };

    scheduleMeasure();
    if (typeof ResizeObserver === "undefined") {
      return () => {
        if (animationFrame !== null && typeof cancelAnimationFrame !== "undefined") {
          cancelAnimationFrame(animationFrame);
        }
      };
    }
    const observer = new ResizeObserver(scheduleMeasure);
    observer.observe(stage);
    for (const element of stage.querySelectorAll("[data-area-id]")) observer.observe(element);
    return () => {
      observer.disconnect();
      if (animationFrame !== null && typeof cancelAnimationFrame !== "undefined") {
        cancelAnimationFrame(animationFrame);
      }
    };
  }, [signature, stageRef]);

  return rects;
}
