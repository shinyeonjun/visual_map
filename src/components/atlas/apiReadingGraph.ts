import type { CSSProperties } from "react";
import type {
  ApiReadingAnswer,
  ApiReadingStep,
  ImpactReviewItem,
  VisualEdge,
  VisualMap,
  VisualNode,
} from "../../types/visual-map";
import type { ApiConnectionModel } from "./apiConnectionModel";

const NODE_WIDTH = 156;
const NODE_HEIGHT = 126;
const NODE_GAP = 48;
const CANVAS_PAD = 24;
export const API_GRAPH_NODE_TOP = 72;
const BRANCH_START_GAP = 64;
const BRANCH_ROW_GAP = 48;

type ApiGraphNode = {
  node: VisualNode;
  item?: ApiReadingStep | ImpactReviewItem;
  x: number;
  y: number;
  primaryIndex?: number;
};

type ApiGraphEdge = {
  edge: VisualEdge;
  from: ApiGraphNode;
  to: ApiGraphNode;
};

type ApiGraphLayout = {
  nodes: ApiGraphNode[];
  edges: ApiGraphEdge[];
  width: number;
  height: number;
  gapX: number;
};

export function buildApiGraphLayout(
  answer: ApiReadingAnswer,
  map: VisualMap,
  model: ApiConnectionModel,
): ApiGraphLayout {
  const nodesById = new Map(map.nodes.map((node) => [node.id, node]));
  const itemByNodeId = new Map<string, ApiReadingStep | ImpactReviewItem>();
  for (const item of [
    ...answer.steps,
    ...(answer.clientRequests ?? []),
    ...(answer.dbRelations ?? []),
    ...answer.dbCandidates,
  ]) {
    if (item.nodeId) itemByNodeId.set(item.nodeId, item);
  }
  for (const { item, node } of model.primaryPath) itemByNodeId.set(node.id, item);
  if (model.primaryDatabase) itemByNodeId.set(model.primaryDatabase.node.id, model.primaryDatabase.item);

  const primaryEntries = [
    ...model.primaryPath,
    ...(model.primaryDatabase ? [model.primaryDatabase] : []),
  ];
  const primaryIndex = new Map(primaryEntries.map(({ node }, index) => [node.id, index]));
  const edgeById = new Map<string, VisualEdge>();
  for (const edge of [
    ...model.primaryEdges,
    ...(model.primaryDatabase ? [model.primaryDatabase.edge] : []),
    ...model.additionalEdges,
  ]) {
    if (nodesById.has(edge.from) && nodesById.has(edge.to)) edgeById.set(edge.id, edge);
  }
  const edges = [...edgeById.values()];
  const nodeIds = new Set<string>(primaryEntries.map(({ node }) => node.id));
  for (const edge of edges) {
    nodeIds.add(edge.from);
    nodeIds.add(edge.to);
  }

  const outgoing = new Map<string, VisualEdge[]>();
  for (const edge of edges) {
    const bucket = outgoing.get(edge.from) ?? [];
    bucket.push(edge);
    outgoing.set(edge.from, bucket);
  }
  for (const bucket of outgoing.values()) bucket.sort((left, right) => left.id.localeCompare(right.id));

  const columns = new Map<string, number>();
  const depths = new Map<string, number>();
  const primaryQueue = primaryEntries.map(({ node }, index) => {
    columns.set(node.id, index);
    depths.set(node.id, 0);
    return node.id;
  });
  assignReachableColumns(primaryQueue, columns, depths, outgoing, primaryIndex);

  let nextColumn = primaryEntries.length;
  for (const nodeId of nodeIds) {
    if (columns.has(nodeId)) continue;
    columns.set(nodeId, nextColumn);
    depths.set(nodeId, 1);
    assignReachableColumns([nodeId], columns, depths, outgoing, primaryIndex);
    nextColumn += 1;
  }

  const rawPositions = new Map<string, { x: number; y: number; primaryIndex?: number }>();
  for (const { node } of primaryEntries) {
    const index = primaryIndex.get(node.id) ?? 0;
    rawPositions.set(node.id, { x: nodeX(index), y: API_GRAPH_NODE_TOP, primaryIndex: index });
  }

  const branchGroups = new Map<string, string[]>();
  for (const nodeId of nodeIds) {
    if (primaryIndex.has(nodeId)) continue;
    const key = `${columns.get(nodeId) ?? 0}:${depths.get(nodeId) ?? 1}`;
    const group = branchGroups.get(key) ?? [];
    group.push(nodeId);
    branchGroups.set(key, group);
  }
  for (const [key, group] of branchGroups) {
    const [columnText, depthText] = key.split(":");
    const column = Number(columnText);
    const depth = Number(depthText);
    group.sort((left, right) => (nodesById.get(left)?.title ?? left).localeCompare(nodesById.get(right)?.title ?? right, "ko-KR") || left.localeCompare(right));
    group.forEach((nodeId, index) => {
      rawPositions.set(nodeId, {
        x: nodeX(column) + (index - (group.length - 1) / 2) * (NODE_WIDTH + NODE_GAP),
        y: API_GRAPH_NODE_TOP + NODE_HEIGHT + BRANCH_START_GAP + (depth - 1) * (NODE_HEIGHT + BRANCH_ROW_GAP),
      });
    });
  }

  const rowGroups = new Map<number, string[]>();
  for (const [nodeId, position] of rawPositions) {
    const row = rowGroups.get(position.y) ?? [];
    row.push(nodeId);
    rowGroups.set(position.y, row);
  }
  for (const group of rowGroups.values()) {
    group.sort((left, right) => (rawPositions.get(left)?.x ?? 0) - (rawPositions.get(right)?.x ?? 0));
    let right = Number.NEGATIVE_INFINITY;
    for (const nodeId of group) {
      const position = rawPositions.get(nodeId);
      if (!position) continue;
      position.x = Math.max(position.x, right + NODE_GAP);
      right = position.x + NODE_WIDTH;
    }
  }

  const minX = Math.min(...[...rawPositions.values()].map(({ x }) => x), CANVAS_PAD);
  const shiftX = Math.max(0, CANVAS_PAD - minX);
  const graphNodes = [...nodeIds].map((nodeId) => {
    const node = nodesById.get(nodeId)!;
    const position = rawPositions.get(nodeId) ?? { x: nodeX(0), y: API_GRAPH_NODE_TOP };
    return {
      node,
      item: itemByNodeId.get(nodeId),
      x: position.x + shiftX,
      y: position.y,
      primaryIndex: position.primaryIndex,
    };
  });
  const graphNodesById = new Map(graphNodes.map((node) => [node.node.id, node]));
  const graphEdges = edges.flatMap((edge) => {
    const from = graphNodesById.get(edge.from);
    const to = graphNodesById.get(edge.to);
    return from && to ? [{ edge, from, to }] : [];
  });
  const maxRight = Math.max(...graphNodes.map(({ x }) => x + NODE_WIDTH), CANVAS_PAD);
  const maxBottom = Math.max(...graphNodes.map(({ y }) => y + NODE_HEIGHT), API_GRAPH_NODE_TOP + NODE_HEIGHT);
  const primaryCount = Math.max(2, primaryEntries.length);
  const minimumWidth = CANVAS_PAD * 2 + primaryCount * NODE_WIDTH + (primaryCount - 1) * NODE_GAP;
  const gapX = nodeX(1) + shiftX;

  return {
    nodes: graphNodes,
    edges: graphEdges,
    width: Math.max(minimumWidth, maxRight + CANVAS_PAD),
    height: Math.max(560, maxBottom + 72),
    gapX,
  };
}

function assignReachableColumns(
  starts: string[],
  columns: Map<string, number>,
  depths: Map<string, number>,
  outgoing: Map<string, VisualEdge[]>,
  primaryIndex: Map<string, number>,
): void {
  const queue = [...starts];
  while (queue.length > 0) {
    const from = queue.shift()!;
    const column = columns.get(from) ?? 0;
    const depth = depths.get(from) ?? 0;
    for (const edge of outgoing.get(from) ?? []) {
      if (primaryIndex.has(edge.to) || columns.has(edge.to)) continue;
      columns.set(edge.to, column);
      depths.set(edge.to, depth + 1);
      queue.push(edge.to);
    }
  }
}

function nodeX(index: number): number {
  return CANVAS_PAD + index * (NODE_WIDTH + NODE_GAP);
}

export function apiGraphEdgePath(connection: ApiGraphEdge, primaryDatabaseEdgeId: string | null): string {
  const { edge, from, to } = connection;
  if (edge.id === primaryDatabaseEdgeId && from.y === to.y) {
    const startX = from.x + NODE_WIDTH / 2;
    const endX = to.x + NODE_WIDTH / 2;
    const bottom = from.y + NODE_HEIGHT;
    const curveY = bottom + 104;
    return `M ${startX} ${bottom} C ${startX} ${curveY}, ${endX} ${curveY}, ${endX} ${to.y + NODE_HEIGHT}`;
  }
  if (Math.abs(from.y - to.y) < 4) {
    const forward = to.x >= from.x;
    const startX = forward ? from.x + NODE_WIDTH : from.x;
    const endX = forward ? to.x : to.x + NODE_WIDTH;
    const y = from.y + NODE_HEIGHT / 2;
    return `M ${startX} ${y} L ${endX} ${y}`;
  }
  const downward = to.y > from.y;
  const startX = from.x + NODE_WIDTH / 2;
  const endX = to.x + NODE_WIDTH / 2;
  const startY = downward ? from.y + NODE_HEIGHT : from.y;
  const endY = downward ? to.y : to.y + NODE_HEIGHT;
  const curveY = (startY + endY) / 2;
  return `M ${startX} ${startY} C ${startX} ${curveY}, ${endX} ${curveY}, ${endX} ${endY}`;
}

export function apiGraphEdgeLabelStyle(
  connection: ApiGraphEdge,
  primaryDatabaseEdgeId: string | null,
): CSSProperties {
  const { edge, from, to } = connection;
  if (edge.id === primaryDatabaseEdgeId && from.y === to.y) {
    return { left: (from.x + to.x + NODE_WIDTH) / 2, top: from.y + NODE_HEIGHT + 82, transform: "translateX(-50%)" };
  }
  if (Math.abs(from.y - to.y) < 4) {
    return { left: Math.min(from.x + NODE_WIDTH, to.x + NODE_WIDTH) + 3, top: from.y + 38 };
  }
  return {
    left: (from.x + to.x + NODE_WIDTH) / 2,
    top: (from.y + to.y + NODE_HEIGHT) / 2,
    transform: "translate(-50%, -50%)",
  };
}
