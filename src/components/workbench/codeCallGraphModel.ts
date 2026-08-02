import type { CSSProperties } from "react";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { visualEdgeTruthClass } from "../../visual/labels";

export const CALL_NODE_WIDTH = 156;
export const CALL_NODE_HEIGHT = 126;
const COLUMN_GAP = 108;
const ROW_GAP = 22;
const DATA_ROW_GAP = 88;
const DATA_ITEM_GAP = 44;
const CANVAS_PAD = 24;
const HEADING_BAND = 40;
const SIDE_LIMIT = 6;
const DATA_LIMIT = 4;

export type CallGraphConnection = {
  edge: VisualEdge;
  node: VisualNode;
};

export type CodeCallGraphModel = {
  focus: VisualNode;
  callers: CallGraphConnection[];
  callees: CallGraphConnection[];
  dataTargets: CallGraphConnection[];
  hiddenCallers: number;
  hiddenCallees: number;
  hiddenDataTargets: number;
  totalConnections: number;
};

/**
 * Builds the bidirectional call view around one code focus:
 * incoming edges become callers (left), outgoing code edges become callees
 * (right), and outgoing data edges become data targets (bottom). Only edges
 * that exist in the map are shown; nothing is inferred here.
 */
export function buildCodeCallGraphModel(focusId: string, map: VisualMap | null): CodeCallGraphModel | null {
  if (!map) return null;
  const nodesById = new Map(map.nodes.map((node) => [node.id, node]));
  const focus = nodesById.get(focusId);
  if (!focus) return null;

  const callers: CallGraphConnection[] = [];
  const callees: CallGraphConnection[] = [];
  const dataTargets: CallGraphConnection[] = [];
  const orderedEdges = [...map.edges].sort(compareEdgesByTruth);
  for (const edge of orderedEdges) {
    if (edge.from === focusId && edge.to !== focusId) {
      const node = nodesById.get(edge.to);
      if (!node) continue;
      if (isDataTargetConnection(edge, node)) {
        dataTargets.push({ edge, node });
      } else {
        callees.push({ edge, node });
      }
    } else if (edge.to === focusId && edge.from !== focusId) {
      const node = nodesById.get(edge.from);
      if (!node) continue;
      callers.push({ edge, node });
    }
  }

  const totalConnections = callers.length + callees.length + dataTargets.length;
  if (totalConnections === 0) return null;

  return {
    focus,
    callers: callers.slice(0, SIDE_LIMIT),
    callees: callees.slice(0, SIDE_LIMIT),
    dataTargets: dataTargets.slice(0, DATA_LIMIT),
    hiddenCallers: Math.max(0, callers.length - SIDE_LIMIT),
    hiddenCallees: Math.max(0, callees.length - SIDE_LIMIT),
    hiddenDataTargets: Math.max(0, dataTargets.length - DATA_LIMIT),
    totalConnections,
  };
}

function isDataTargetConnection(edge: VisualEdge, node: VisualNode): boolean {
  if (edge.kind === "code_db_read" || edge.kind === "code_db_write" || edge.kind === "code_db_uses_column") return true;
  const kind = node.kind.toLowerCase();
  return node.layer === "db" || node.layer === "database" || kind === "table" || kind === "column";
}

const TRUTH_ORDER: Record<string, number> = { confirmed: 0, structural: 1, inferred: 2, candidate: 3 };

function compareEdgesByTruth(left: VisualEdge, right: VisualEdge): number {
  const leftOrder = TRUTH_ORDER[visualEdgeTruthClass(left)] ?? 4;
  const rightOrder = TRUTH_ORDER[visualEdgeTruthClass(right)] ?? 4;
  return leftOrder - rightOrder || left.id.localeCompare(right.id);
}

export type CallGraphPlaced = {
  connection: CallGraphConnection;
  x: number;
  y: number;
};

type CodeCallGraphLayout = {
  width: number;
  height: number;
  focus: { x: number; y: number };
  callers: CallGraphPlaced[];
  callees: CallGraphPlaced[];
  dataTargets: CallGraphPlaced[];
  headings: Array<{ x: number; label: string }>;
};

export function buildCodeCallGraphLayout(model: CodeCallGraphModel): CodeCallGraphLayout {
  const stackHeight = (count: number): number =>
    count === 0 ? 0 : count * CALL_NODE_HEIGHT + (count - 1) * ROW_GAP;

  const mainHeight = Math.max(stackHeight(model.callers.length), stackHeight(model.callees.length), CALL_NODE_HEIGHT);
  const top = CANVAS_PAD + HEADING_BAND;
  const centerY = top + mainHeight / 2;

  const callerX = CANVAS_PAD;
  const focusX = CANVAS_PAD + CALL_NODE_WIDTH + COLUMN_GAP;
  const calleeX = CANVAS_PAD + 2 * (CALL_NODE_WIDTH + COLUMN_GAP);

  const placeColumn = (items: CallGraphConnection[], x: number): CallGraphPlaced[] => {
    const startY = centerY - stackHeight(items.length) / 2;
    return items.map((connection, index) => ({
      connection,
      x,
      y: startY + index * (CALL_NODE_HEIGHT + ROW_GAP),
    }));
  };

  const callers = placeColumn(model.callers, callerX);
  const callees = placeColumn(model.callees, calleeX);
  const focus = { x: focusX, y: centerY - CALL_NODE_HEIGHT / 2 };

  const dataCount = model.dataTargets.length;
  const dataRowWidth = dataCount === 0 ? 0 : dataCount * CALL_NODE_WIDTH + (dataCount - 1) * DATA_ITEM_GAP;
  const dataY = top + mainHeight + DATA_ROW_GAP;
  const dataStartX = Math.max(CANVAS_PAD, focusX + CALL_NODE_WIDTH / 2 - dataRowWidth / 2);
  const dataTargets = model.dataTargets.map((connection, index) => ({
    connection,
    x: dataStartX + index * (CALL_NODE_WIDTH + DATA_ITEM_GAP),
    y: dataY,
  }));

  const columnsRight = calleeX + CALL_NODE_WIDTH;
  const dataRight = dataCount === 0 ? 0 : dataStartX + dataRowWidth;
  const width = Math.max(columnsRight, dataRight) + CANVAS_PAD;
  const height = dataCount > 0 ? dataY + CALL_NODE_HEIGHT + CANVAS_PAD : top + mainHeight + CANVAS_PAD;

  return {
    width,
    height,
    focus,
    callers,
    callees,
    dataTargets,
    headings: [
      { x: callerX + CALL_NODE_WIDTH / 2, label: "이 코드를 부르는 곳" },
      { x: focusX + CALL_NODE_WIDTH / 2, label: "선택한 코드" },
      { x: calleeX + CALL_NODE_WIDTH / 2, label: "이 코드가 부르는 대상" },
    ],
  };
}

/** Horizontal S-curve between a side card and the focus card. */
export function callGraphSidePath(
  side: "caller" | "callee",
  placed: { x: number; y: number },
  focus: { x: number; y: number },
): string {
  const cardMidY = placed.y + CALL_NODE_HEIGHT / 2;
  const focusMidY = focus.y + CALL_NODE_HEIGHT / 2;
  const startX = side === "caller" ? placed.x + CALL_NODE_WIDTH : focus.x + CALL_NODE_WIDTH;
  const endX = side === "caller" ? focus.x : placed.x;
  const startY = side === "caller" ? cardMidY : focusMidY;
  const endY = side === "caller" ? focusMidY : cardMidY;
  const pull = Math.min(72, Math.max(24, (endX - startX) / 2));
  return `M ${startX} ${startY} C ${startX + pull} ${startY}, ${endX - pull} ${endY}, ${endX} ${endY}`;
}

/** Vertical S-curve from the focus card down to a data target card. */
export function callGraphDataPath(focus: { x: number; y: number }, target: { x: number; y: number }): string {
  const endX = target.x + CALL_NODE_WIDTH / 2;
  const focusCenter = focus.x + CALL_NODE_WIDTH / 2;
  const limit = CALL_NODE_WIDTH / 2 - 18;
  const shift = Math.max(-limit, Math.min(limit, (endX - focusCenter) * 0.2));
  const startX = focusCenter + shift;
  const startY = focus.y + CALL_NODE_HEIGHT;
  const endY = target.y;
  const pull = Math.min(96, Math.max(24, (endY - startY) / 2));
  return `M ${startX} ${startY} C ${startX} ${startY + pull}, ${endX} ${endY - pull}, ${endX} ${endY}`;
}

export function callGraphSideLabelStyle(
  side: "caller" | "callee",
  placed: { x: number; y: number },
  focus: { x: number; y: number },
): CSSProperties {
  const cardMidY = placed.y + CALL_NODE_HEIGHT / 2;
  const focusMidY = focus.y + CALL_NODE_HEIGHT / 2;
  const left = side === "caller"
    ? (placed.x + CALL_NODE_WIDTH + focus.x) / 2
    : (focus.x + CALL_NODE_WIDTH + placed.x) / 2;
  return { left, top: (cardMidY + focusMidY) / 2, transform: "translate(-50%, -50%)" };
}

export function callGraphDataLabelStyle(focus: { x: number; y: number }, target: { x: number; y: number }): CSSProperties {
  const endX = target.x + CALL_NODE_WIDTH / 2;
  const focusCenter = focus.x + CALL_NODE_WIDTH / 2;
  return {
    left: (focusCenter + endX) / 2,
    top: (focus.y + CALL_NODE_HEIGHT + target.y) / 2,
    transform: "translate(-50%, -50%)",
  };
}
