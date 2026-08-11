/**
 * Where the overview canvas puts things, and how wide the world has to be.
 *
 * Geometry only: no React, no DOM. Areas size themselves from their contents,
 * so this can estimate but never know a final height — the measured rectangles
 * in `useAreaRects` are what relation routing actually uses.
 */

import type { MapArea, MapView } from "./types";

export type Rect = { x: number; y: number; width: number; height: number };

/** The smallest useful canvas. Larger maps expand from their actual layout. */
const MIN_WORLD_WIDTH = 940;
const MIN_WORLD_HEIGHT = 720;
/** Where a top-level area sits when the reader has not moved it yet. */
export const DEFAULT_AREA_WIDTH = 232;
const CANVAS_PADDING = 48;
const COLUMN_GAP = 40;
const ROW_GAP = 48;

/** An orthogonal route between two boxes, plus where to hang its count. */
export function elbow(from: Rect | undefined, to: Rect | undefined) {
  if (!from || !to) return null;
  const fromCenterY = from.y + from.height / 2;
  const toCenterY = to.y + to.height / 2;

  if (to.x >= from.x + from.width) {
    const startX = from.x + from.width;
    const endX = to.x;
    const midX = (startX + endX) / 2;
    return {
      d: `M${startX} ${fromCenterY} L${midX} ${fromCenterY} L${midX} ${toCenterY} L${endX} ${toCenterY}`,
      midX,
      midY: Math.min(fromCenterY, toCenterY) + Math.abs(fromCenterY - toCenterY) / 2,
    };
  }

  const startY = from.y + from.height;
  const endY = to.y;
  const fromCenterX = from.x + from.width / 2;
  const toCenterX = to.x + to.width / 2;
  const midY = (startY + endY) / 2;
  return {
    d: `M${fromCenterX} ${startY} L${fromCenterX} ${midY} L${toCenterX} ${midY} L${toCenterX} ${endY}`,
    /*
      Sit above the source rather than halfway across. The midpoint between two
      centres can land on whatever card happens to be between the rows, while
      the run leaving the source is in the gap the grid already left empty.
    */
    midX: fromCenterX,
    midY,
  };
}

export function rectMapsEqual(left: Map<string, Rect>, right: Map<string, Rect>): boolean {
  if (left.size !== right.size) return false;
  for (const [id, next] of right) {
    const current = left.get(id);
    if (!current) return false;
    if (
      Math.abs(current.x - next.x) > 0.25 ||
      Math.abs(current.y - next.y) > 0.25 ||
      Math.abs(current.width - next.width) > 0.25 ||
      Math.abs(current.height - next.height) > 0.25
    ) {
      return false;
    }
  }
  return true;
}

export function areaItemCount(area: MapArea): number {
  return area.nodes.length + area.hiddenNodeCount + area.areas.reduce((sum, child) => sum + areaItemCount(child), 0);
}

function estimateAreaHeight(area: MapArea): number {
  const featureRows = Math.min(area.areas.length, 4);
  const featureHeight = featureRows > 0 ? 16 + featureRows * 52 : 0;
  const traceHeight = area.trace ? 28 : 0;
  return 146 + featureHeight + traceHeight;
}

/**
 * How many areas go side by side.
 *
 * `fit` scales to whichever axis is tighter, so a grid far wider than the
 * viewport is fitted by width and throws the remaining height away. The canvas
 * sits between two side panels and is close to square, so a squarer grid
 * spends that height on legible text instead of empty background.
 */
function defaultColumnCount(areaCount: number): number {
  if (areaCount <= 4) return Math.max(1, areaCount);
  if (areaCount <= 9) return 3;
  if (areaCount <= 20) return 4;
  if (areaCount <= 40) return 5;
  if (areaCount <= 72) return 6;
  return 7;
}

/** A deterministic, non-overlapping grid when stored positions are absent. */
export function defaultPositions(areas: MapArea[]): Array<{ x: number; y: number }> {
  const positions: Array<{ x: number; y: number }> = [];
  const columns = defaultColumnCount(areas.length);
  let y = 72;
  for (let start = 0; start < areas.length; start += columns) {
    const row = areas.slice(start, start + columns);
    let x = 40;
    let rowHeight = 0;
    row.forEach((area, offset) => {
      positions[start + offset] = { x, y };
      x += (area.width ?? DEFAULT_AREA_WIDTH) + COLUMN_GAP;
      rowHeight = Math.max(rowHeight, estimateAreaHeight(area));
    });
    y += rowHeight + ROW_GAP;
  }
  return positions;
}

export function mapWorld(view: MapView, fallbackPositions: Array<{ x: number; y: number }>) {
  let maxX = MIN_WORLD_WIDTH - CANVAS_PADDING;
  let maxY = MIN_WORLD_HEIGHT - CANVAS_PADDING;
  view.areas.forEach((area, index) => {
    const position = area.position ?? fallbackPositions[index] ?? { x: CANVAS_PADDING, y: CANVAS_PADDING };
    maxX = Math.max(maxX, position.x + (area.width ?? DEFAULT_AREA_WIDTH));
    maxY = Math.max(maxY, position.y + estimateAreaHeight(area));
  });
  return {
    width: Math.ceil(maxX + CANVAS_PADDING),
    height: Math.ceil(maxY + CANVAS_PADDING),
  };
}
