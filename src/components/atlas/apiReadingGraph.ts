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

export const API_GRAPH_NODE_WIDTH = 156;
export const API_GRAPH_NODE_HEIGHT = 126;
export const API_GRAPH_NODE_GAP = 48;
const CANVAS_PAD = 24;
export const API_GRAPH_NODE_TOP = 72;
const BRANCH_START_GAP = 76;
const BRANCH_ROW_GAP = 64;

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

  const depths = new Map<string, number>();
  const primaryQueue = primaryEntries.map(({ node }) => {
    depths.set(node.id, 0);
    return node.id;
  });
  assignReachableDepths(primaryQueue, depths, outgoing, primaryIndex);
  for (const nodeId of nodeIds) {
    if (!depths.has(nodeId)) depths.set(nodeId, 1);
  }

  const rawPositions = new Map<string, { x: number; y: number; primaryIndex?: number }>();
  for (const { node } of primaryEntries) {
    const index = primaryIndex.get(node.id) ?? 0;
    rawPositions.set(node.id, { x: nodeX(index), y: API_GRAPH_NODE_TOP, primaryIndex: index });
  }

  const incoming = new Map<string, string[]>();
  for (const edge of edges) {
    const parents = incoming.get(edge.to) ?? [];
    parents.push(edge.from);
    incoming.set(edge.to, parents);
  }
  const maxDepth = Math.max(1, ...[...depths.values()]);
  const branchRows = new Map<number, string[]>();
  for (let depth = 1; depth <= maxDepth; depth += 1) {
    branchRows.set(
      depth,
      [...nodeIds].filter((nodeId) => !primaryIndex.has(nodeId) && depths.get(nodeId) === depth),
    );
  }

  // A pair of forward/backward barycenter sweeps keeps branches readable when
  // several parents share the same depth. The primary path remains fixed.
  for (let sweep = 0; sweep < 2; sweep += 1) {
    for (let depth = 1; depth <= maxDepth; depth += 1) {
      const row = branchRows.get(depth) ?? [];
      row.sort((left, right) => compareByAnchor(left, right, incoming, rawPositions, nodesById));
      placeBranchRow(row, depth, incoming, rawPositions);
    }
    for (let depth = maxDepth - 1; depth >= 1; depth -= 1) {
      const row = branchRows.get(depth) ?? [];
      row.sort((left, right) => compareByAnchor(left, right, outgoing, rawPositions, nodesById));
      placeBranchRow(row, depth, incoming, rawPositions);
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
  const maxRight = Math.max(...graphNodes.map(({ x }) => x + API_GRAPH_NODE_WIDTH), CANVAS_PAD);
  const maxBottom = Math.max(...graphNodes.map(({ y }) => y + API_GRAPH_NODE_HEIGHT), API_GRAPH_NODE_TOP + API_GRAPH_NODE_HEIGHT);
  const primaryCount = Math.max(2, primaryEntries.length);
  const minimumWidth = CANVAS_PAD * 2 + primaryCount * API_GRAPH_NODE_WIDTH + (primaryCount - 1) * API_GRAPH_NODE_GAP;
  const gapX = nodeX(1) + shiftX;

  return {
    nodes: graphNodes,
    edges: graphEdges,
    width: Math.max(minimumWidth, maxRight + CANVAS_PAD),
    height: Math.max(360, maxBottom + 104),
    gapX,
  };
}

function compareByAnchor(
  left: string,
  right: string,
  neighbors: Map<string, VisualEdge[]> | Map<string, string[]>,
  positions: Map<string, { x: number; y: number; primaryIndex?: number }>,
  nodesById: Map<string, VisualNode>,
): number {
  const leftAnchor = nodeBarycenter(left, neighbors, positions);
  const rightAnchor = nodeBarycenter(right, neighbors, positions);
  return leftAnchor - rightAnchor
    || (nodesById.get(left)?.title ?? left).localeCompare(nodesById.get(right)?.title ?? right, "ko-KR")
    || left.localeCompare(right);
}

function placeBranchRow(
  row: string[],
  depth: number,
  incoming: Map<string, string[]>,
  positions: Map<string, { x: number; y: number; primaryIndex?: number }>,
): void {
  let right = Number.NEGATIVE_INFINITY;
  for (const nodeId of row) {
    const desiredX = nodeAnchor(nodeId, incoming, positions) - API_GRAPH_NODE_WIDTH / 2;
    const x = Math.max(desiredX, right + API_GRAPH_NODE_GAP);
    positions.set(nodeId, {
      x,
      y: API_GRAPH_NODE_TOP + API_GRAPH_NODE_HEIGHT + BRANCH_START_GAP + (depth - 1) * (API_GRAPH_NODE_HEIGHT + BRANCH_ROW_GAP),
    });
    right = x + API_GRAPH_NODE_WIDTH;
  }
}

function nodeBarycenter(
  nodeId: string,
  neighbors: Map<string, VisualEdge[]> | Map<string, string[]>,
  positions: Map<string, { x: number; y: number; primaryIndex?: number }>,
): number {
  const entries = neighbors.get(nodeId) ?? [];
  const neighborIds = entries.map((entry) => typeof entry === "string" ? entry : entry.to);
  const centers = neighborIds
    .map((neighborId) => positions.get(neighborId))
    .filter((position): position is { x: number; y: number; primaryIndex?: number } => Boolean(position))
    .map((position) => position.x + API_GRAPH_NODE_WIDTH / 2);
  return centers.length > 0
    ? centers.reduce((sum, center) => sum + center, 0) / centers.length
    : nodeX(0) + API_GRAPH_NODE_WIDTH / 2;
}

function assignReachableDepths(
  starts: string[],
  depths: Map<string, number>,
  outgoing: Map<string, VisualEdge[]>,
  primaryIndex: Map<string, number>,
): void {
  const queue = [...starts];
  while (queue.length > 0) {
    const from = queue.shift()!;
    const depth = depths.get(from) ?? 0;
    for (const edge of outgoing.get(from) ?? []) {
      if (primaryIndex.has(edge.to)) continue;
      const nextDepth = depth + 1;
      const currentDepth = depths.get(edge.to);
      if (currentDepth !== undefined && currentDepth <= nextDepth) continue;
      depths.set(edge.to, nextDepth);
      queue.push(edge.to);
    }
  }
}

function nodeAnchor(
  nodeId: string,
  incoming: Map<string, string[]>,
  positions: Map<string, { x: number; y: number; primaryIndex?: number }>,
): number {
  const parents = (incoming.get(nodeId) ?? []).filter((parent) => positions.has(parent));
  if (parents.length === 0) return nodeX(0) + API_GRAPH_NODE_WIDTH / 2;
  return parents.reduce((sum, parent) => {
    const position = positions.get(parent)!;
    return sum + position.x + API_GRAPH_NODE_WIDTH / 2;
  }, 0) / parents.length;
}

function nodeX(index: number): number {
  return CANVAS_PAD + index * (API_GRAPH_NODE_WIDTH + API_GRAPH_NODE_GAP);
}

const SAME_ROW_ADJACENT_GAP = API_GRAPH_NODE_GAP + 14;

type ApiGraphEdgeAnchors = {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
  kind: "adjacent" | "same-row-arc" | "cross-row";
};

function isSameRow(from: ApiGraphNode, to: ApiGraphNode): boolean {
  return Math.abs(from.y - to.y) < 4;
}

function sameRowFacingGap(from: ApiGraphNode, to: ApiGraphNode): number {
  return to.x >= from.x
    ? to.x - (from.x + API_GRAPH_NODE_WIDTH)
    : from.x - (to.x + API_GRAPH_NODE_WIDTH);
}

function sameRowArcDepth(isPrimaryDatabase: boolean, span: number): number {
  const depth = 24 + span * 0.03;
  return isPrimaryDatabase ? Math.min(56, depth + 8) : Math.min(44, depth);
}

function fanOutX(from: ApiGraphNode, targetCenterX: number): number {
  const center = from.x + API_GRAPH_NODE_WIDTH / 2;
  const limit = API_GRAPH_NODE_WIDTH / 2 - 18;
  const shift = Math.max(-limit, Math.min(limit, (targetCenterX - center) * 0.12));
  return center + shift;
}

export function apiGraphEdgePath(connection: ApiGraphEdge, primaryDatabaseEdgeId: string | null): string {
  const { edge, from, to } = connection;
  const anchors = apiGraphEdgeAnchors(connection, primaryDatabaseEdgeId);
  if (anchors.kind === "adjacent") {
    return `M ${anchors.startX} ${anchors.startY} L ${anchors.endX} ${anchors.endY}`;
  }
  if (anchors.kind === "same-row-arc") {
    const startX = anchors.startX;
    const endX = anchors.endX;
    const bottom = anchors.startY;
    const isPrimaryDatabase = edge.id === primaryDatabaseEdgeId;
    const curveY = bottom + sameRowArcDepth(isPrimaryDatabase, Math.abs(endX - startX));
    return `M ${startX} ${bottom} C ${startX} ${curveY}, ${endX} ${curveY}, ${endX} ${anchors.endY}`;
  }
  const startX = anchors.startX;
  const endX = anchors.endX;
  const startY = anchors.startY;
  const endY = anchors.endY;
  const downward = to.y > from.y;
  const pull = Math.min(120, Math.max(28, Math.abs(endY - startY) / 2)) * (downward ? 1 : -1);
  return `M ${startX} ${startY} C ${startX} ${startY + pull}, ${endX} ${endY - pull}, ${endX} ${endY}`;
}

export function apiGraphEdgeAnchors(
  connection: ApiGraphEdge,
  primaryDatabaseEdgeId: string | null,
): ApiGraphEdgeAnchors {
  const { edge, from, to } = connection;
  if (isSameRow(from, to)) {
    const isPrimaryDatabase = edge.id === primaryDatabaseEdgeId;
    if (!isPrimaryDatabase && sameRowFacingGap(from, to) <= SAME_ROW_ADJACENT_GAP) {
      const forward = to.x >= from.x;
      return {
        startX: forward ? from.x + API_GRAPH_NODE_WIDTH : from.x,
        startY: from.y + API_GRAPH_NODE_HEIGHT / 2,
        endX: forward ? to.x : to.x + API_GRAPH_NODE_WIDTH,
        endY: from.y + API_GRAPH_NODE_HEIGHT / 2,
        kind: "adjacent",
      };
    }
    return {
      startX: from.x + API_GRAPH_NODE_WIDTH / 2,
      startY: from.y + API_GRAPH_NODE_HEIGHT,
      endX: to.x + API_GRAPH_NODE_WIDTH / 2,
      endY: to.y + API_GRAPH_NODE_HEIGHT,
      kind: "same-row-arc",
    };
  }
  const downward = to.y > from.y;
  const endX = to.x + API_GRAPH_NODE_WIDTH / 2;
  return {
    startX: fanOutX(from, endX),
    startY: downward ? from.y + API_GRAPH_NODE_HEIGHT : from.y,
    endX,
    endY: downward ? to.y : to.y + API_GRAPH_NODE_HEIGHT,
    kind: "cross-row",
  };
}

export function apiGraphEdgeLabelStyle(
  connection: ApiGraphEdge,
  primaryDatabaseEdgeId: string | null,
): CSSProperties {
  const { edge, from, to } = connection;
  if (isSameRow(from, to)) {
    const isPrimaryDatabase = edge.id === primaryDatabaseEdgeId;
    if (!isPrimaryDatabase && sameRowFacingGap(from, to) <= SAME_ROW_ADJACENT_GAP) {
      return { left: Math.min(from.x + API_GRAPH_NODE_WIDTH, to.x + API_GRAPH_NODE_WIDTH) + 3, top: from.y + 38 };
    }
    const startX = from.x + API_GRAPH_NODE_WIDTH / 2;
    const endX = to.x + API_GRAPH_NODE_WIDTH / 2;
    const depth = sameRowArcDepth(isPrimaryDatabase, Math.abs(endX - startX));
    return {
      left: (startX + endX) / 2,
      top: from.y + API_GRAPH_NODE_HEIGHT + depth * 0.75,
      transform: "translate(-50%, -50%)",
    };
  }
  const endX = to.x + API_GRAPH_NODE_WIDTH / 2;
  const startX = fanOutX(from, endX);
  return {
    left: (startX + endX) / 2,
    top: (from.y + to.y + API_GRAPH_NODE_HEIGHT) / 2,
    transform: "translate(-50%, -50%)",
  };
}
