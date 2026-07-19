import { dbInventoryTableKey } from "../../types/workspace";
import type { DbInventoryTable } from "../../types/workspace";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  columnLabelFromNodeId,
  columnRefFromNodeId,
  tableKeyFromDbNodeId as tableKeyFromNodeId,
} from "../../visual/nodeIds";
import { visualEdgeKindLabel as edgeKindLabel } from "../../visual/labels";
import type { RelationSummary } from "./ArchitectureMap";

export type RelationTone = "confirmed" | "typed" | "candidate" | "inferred";

export type RelationLedgerRow = {
  edge: VisualEdge;
  from: string;
  fromTitle: string;
  to: string;
  toTitle: string;
  label: string;
  tone: RelationTone;
  evidence: string;
};

export type RelationBeam = {
  edge: VisualEdge;
  x1: number;
  x2: number;
  y1: number;
  y2: number;
  tone: RelationTone;
  active: boolean;
  label: string;
};

export const AT_GUTTER_WIDTH = 88;
export const AT_LANE_WIDTH = 144;
export const AT_LANE_GAP = 8;
export const AT_LANE_PAD_X = 6;

export function edgeTouchesNode(edge: VisualEdge, node: VisualNode | null): boolean {
  if (!node) {
    return false;
  }
  if (edge.from === node.id || edge.to === node.id) {
    return true;
  }
  if (node.kind !== "table" || !node.id.startsWith("db:table:")) {
    return false;
  }
  const tableKey = node.id.slice("db:table:".length);
  const columnPrefix = `db:column:${tableKey}:`;
  return edge.from.startsWith(columnPrefix) || edge.to.startsWith(columnPrefix);
}

export function edgeTouchesNodeId(edge: VisualEdge, nodeId: string): boolean {
  if (edge.from === nodeId || edge.to === nodeId) {
    return true;
  }
  const tableKey = tableKeyFromNodeId(nodeId);
  return tableKey ? edgeTouchesTable(edge, tableKey) : false;
}

export function edgeTouchesTable(edge: VisualEdge, tableKey: string): boolean {
  return nodeTouchesTable(edge.from, tableKey) || nodeTouchesTable(edge.to, tableKey);
}

export function nodeTouchesTable(nodeId: string, tableKey: string): boolean {
  return nodeId === `db:table:${tableKey}` || nodeId.startsWith(`db:column:${tableKey}:`);
}

export function nodesShareTableOrId(a: string, b: string): boolean {
  if (a === b) {
    return true;
  }
  const aTable = tableKeyFromNodeId(a);
  return Boolean(aTable && nodeTouchesTable(b, aTable));
}

export function nodeLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  if (!node) {
    return columnLabelFromNodeId(id) ?? (id.startsWith("db:table:") ? id.slice("db:table:".length) : id);
  }
  if (node?.kind === "column") {
    const tableKey = tableKeyFromNodeId(id);
    return tableKey ? `${tableKey}.${node.title}` : node.title;
  }
  return node.title;
}

function compactRelationEndpointLabel(label: string): string {
  const parts = label.split(".");
  return parts.length >= 3 ? parts.slice(-2).join(".") : label;
}

export function takeWithPinned<T>(items: T[], pinnedIds: Set<string>, key: (item: T) => string, limit: number): T[] {
  const pinned = items.filter((item) => pinnedIds.has(key(item)));
  const rest = items.filter((item) => !pinnedIds.has(key(item))).slice(0, Math.max(0, limit - pinned.length));
  return [...pinned, ...rest].slice(0, limit);
}

export function idsInItems<T>(ids: Set<string>, items: T[], key: (item: T) => string): Set<string> {
  return new Set(items.map(key).filter((id) => ids.has(id)));
}

export function codeIdsFromNodeIds(nodeIds: Array<string | null | undefined>): Set<string> {
  return new Set(
    nodeIds
      .filter((id): id is string => Boolean(id?.startsWith("code:")))
      .map((id) => id.slice("code:".length)),
  );
}

export function tableKeysFromNodeIds(nodeIds: Array<string | null | undefined>): Set<string> {
  return new Set(
    nodeIds
      .map((id) => (id ? tableKeyFromNodeId(id) : null))
      .filter((id): id is string => Boolean(id)),
  );
}

export function columnNamesForTableFromNodeIds(nodeIds: Array<string | null | undefined>, tableKey: string): Set<string> {
  return new Set(
    nodeIds
      .map((id) => (id ? columnRefFromNodeId(id) : null))
      .filter((ref): ref is { tableKey: string; columnName: string } => Boolean(ref && ref.tableKey === tableKey))
      .map((ref) => ref.columnName),
  );
}

export function atlasCodeKindRank(kind: string): number {
  const key = kind.trim().toLowerCase();
  if (key === "handler" || key === "controller" || key === "function" || key === "method") {
    return 0;
  }
  if (key === "service") {
    return 1;
  }
  if (key === "repository") {
    return 2;
  }
  return key === "class" ? 3 : 4;
}

export function filterCodeItemsByMap<T extends { id: string }>(items: T[], focusedNodeIds: Set<string>): T[] {
  const filtered = items.filter((item) => focusedNodeIds.has(`code:${item.id}`));
  return filtered.length > 0 ? filtered : items;
}

export function filterTablesByMap(items: DbInventoryTable[], focusedNodeIds: Set<string>): DbInventoryTable[] {
  const filtered = items.filter((item) => {
    const tableKey = dbInventoryTableKey(item);
    return focusedNodeIds.has(`db:table:${tableKey}`) || Array.from(focusedNodeIds).some((id) => id.startsWith(`db:column:${tableKey}:`));
  });
  return filtered.length > 0 ? filtered : items;
}

export function rankNodeItems<T>(
  items: T[],
  relationCounts: Map<string, RelationSummary>,
  nodeId: (item: T) => string,
  selectedNodeId: string | null,
): T[] {
  return [...items].sort((a, b) => {
    const aId = nodeId(a);
    const bId = nodeId(b);
    if (selectedNodeId) {
      if (aId === selectedNodeId) return -1;
      if (bId === selectedNodeId) return 1;
    }
    return relationScore(relationCounts.get(bId)) - relationScore(relationCounts.get(aId));
  });
}

function relationScore(summary?: RelationSummary): number {
  return summary ? summary.confirmed * 100 + summary.typed * 60 + summary.candidate * 30 + summary.inferred * 15 : 0;
}

export function buildRelationCounts(map: VisualMap | null): Map<string, RelationSummary> {
  const counts = new Map<string, RelationSummary>();
  if (!map) {
    return counts;
  }

  for (const edge of map.edges) {
    for (const nodeId of relationCountNodeIds(edge)) {
      addRelation(counts, nodeId, edge);
    }
  }
  return counts;
}

function relationCountNodeIds(edge: VisualEdge): string[] {
  return Array.from(new Set([edge.from, edge.to, tableAggregateNodeId(edge.from), tableAggregateNodeId(edge.to)].filter(Boolean) as string[]));
}

function tableAggregateNodeId(nodeId: string): string | null {
  const tableKey = tableKeyFromNodeId(nodeId);
  const tableId = tableKey ? `db:table:${tableKey}` : null;
  return tableId && tableId !== nodeId ? tableId : null;
}

function addRelation(counts: Map<string, RelationSummary>, nodeId: string, edge: VisualEdge) {
  const summary = counts.get(nodeId) ?? { confirmed: 0, typed: 0, inferred: 0, candidate: 0 };
  const tone = relationTone(edge);
  if (tone === "candidate") {
    summary.candidate += 1;
  } else if (tone === "inferred") {
    summary.inferred += 1;
  } else if (tone === "typed") {
    summary.typed += 1;
  } else {
    summary.confirmed += 1;
  }
  counts.set(nodeId, summary);
}

export function relationLedgerRows(
  map: VisualMap | null,
  selectedEdge: VisualEdge | null,
  selectedNode: VisualNode | null,
  selectedFocusId: string | null,
): RelationLedgerRow[] {
  return [...relationLedgerScopedEdges(map, selectedEdge, selectedNode, selectedFocusId)]
    .sort((a, b) => relationLedgerRank(a, selectedEdge, selectedNode, selectedFocusId) - relationLedgerRank(b, selectedEdge, selectedNode, selectedFocusId))
    .slice(0, 5)
    .map((edge) => {
      const tone = relationTone(edge);
      const from = nodeLabel(edge.from, map);
      const to = nodeLabel(edge.to, map);
      return {
        edge,
        from: compactRelationEndpointLabel(from),
        fromTitle: from,
        to: compactRelationEndpointLabel(to),
        toTitle: to,
        label: relationLabel(tone),
        tone,
        evidence: relationEvidenceText(edge, tone),
      };
    });
}

export function relationLedgerScopedEdges(
  map: VisualMap | null,
  selectedEdge: VisualEdge | null,
  selectedNode: VisualNode | null,
  selectedFocusId: string | null,
): VisualEdge[] {
  if (!map) {
    return [];
  }
  if (selectedEdge) {
    return map.edges.filter((edge) => edge.id === selectedEdge.id);
  }
  if (selectedNode) {
    return map.edges.filter((edge) => edgeTouchesNode(edge, selectedNode));
  }
  if (selectedFocusId) {
    return map.edges.filter((edge) => edgeTouchesNodeId(edge, selectedFocusId));
  }
  return map.edges;
}

function relationEvidenceText(edge: VisualEdge, tone: RelationTone): string {
  const evidence = edge.evidence[0]?.text?.trim();
  if (evidence) {
    return readableRelationEvidence(evidence, edge, tone);
  }
  if (tone === "candidate") {
    return edge.confidence ? `후보 근거 · 단서 ${confidenceLabel(edge.confidence)}` : "후보 근거 · 직접 검증 대기";
  }
  if (tone === "inferred") {
    return "이름 단서 · 호출 근거 대기";
  }
  return `${edgeKindLabel(edge)} · 구조 정보 기준`;
}

function readableRelationEvidence(evidence: string, edge: VisualEdge, tone: RelationTone): string {
  if (/[가-힣]/.test(evidence)) {
    return evidence;
  }

  const lower = evidence.toLowerCase();
  if (edge.kind === "code_handle") {
    return "코드 엔진 HANDLES로 확인한 Route → Handler 근거";
  }
  if (edge.kind === "code_call" || lower.includes("calls from")) {
    if (lower.includes("route") && lower.includes("service")) {
      return "라우트에서 서비스로 이어지는 호출 근거";
    }
    if (lower.includes("service") && lower.includes("repository")) {
      return "서비스에서 저장소로 이어지는 호출 근거";
    }
    return "읽은 코드 호출 근거";
  }
  if (edge.kind === "db_fk" || lower.includes("foreign key") || lower.startsWith("fk ")) {
    return "DB FK 제약으로 확인된 구조 근거";
  }
  if (tone === "candidate" || lower.includes("name match") || lower.includes("table name match")) {
    return edge.confidence ? `이름 단서가 맞아 후보로 연결 · ${confidenceLabel(edge.confidence)}` : "이름 단서가 맞아 후보로 연결";
  }
  return evidence;
}

function relationLedgerRank(edge: VisualEdge, selectedEdge: VisualEdge | null, selectedNode: VisualNode | null, selectedFocusId: string | null): number {
  if (selectedEdge?.id === edge.id) {
    return -20;
  }
  if (edgeTouchesNode(edge, selectedNode)) {
    return -10 + relationRank(edge);
  }
  if (selectedFocusId && edgeTouchesNodeId(edge, selectedFocusId)) {
    return -10 + relationRank(edge);
  }
  return relationRank(edge);
}

export function buildRelationBeams({
  map,
  routeCards,
  codeCards,
  tableCards,
  bands,
  selectedEdge,
  selectedNode,
  selectedFocusId,
}: {
  map: VisualMap | null;
  routeCards: { id: string }[];
  codeCards: { id: string }[];
  tableCards: DbInventoryTable[];
  bands: Array<"api" | "code" | "db">;
  selectedEdge: VisualEdge | null;
  selectedNode: VisualNode | null;
  selectedFocusId: string | null;
}): RelationBeam[] {
  if (!map) {
    return [];
  }
  const visibleEdges = prioritizedBeamEdges(map.edges, selectedEdge, selectedNode, selectedFocusId);
  return visibleEdges.flatMap((edge) => {
    const from = nodePosition(edge.from, routeCards, codeCards, tableCards);
    const to = nodePosition(edge.to, routeCards, codeCards, tableCards);
    if (!from || !to) {
      return [];
    }
    const tone = relationTone(edge);
    return [
      {
        edge,
        x1: laneCenterX(from.lane),
        x2: laneCenterX(to.lane),
        y1: bandCenterPercent(bands, from.band),
        y2: bandCenterPercent(bands, to.band),
        tone,
        active: selectedEdge?.id === edge.id || edgeTouchesNode(edge, selectedNode) || Boolean(selectedFocusId && edgeTouchesNodeId(edge, selectedFocusId)),
        label: `${relationLabel(tone)} 관계: ${nodeLabel(edge.from, map)} → ${nodeLabel(edge.to, map)}`,
      },
    ];
  });
}

function prioritizedBeamEdges(
  edges: VisualEdge[],
  selectedEdge: VisualEdge | null,
  selectedNode: VisualNode | null,
  selectedFocusId: string | null,
): VisualEdge[] {
  const focused = edges.filter(
    (edge) =>
      selectedEdge?.id === edge.id ||
      edgeTouchesNode(edge, selectedNode) ||
      Boolean(selectedFocusId && edgeTouchesNodeId(edge, selectedFocusId)),
  );
  return uniqueEdges([
    ...focused.sort((a, b) => beamFocusRank(a, selectedEdge) - beamFocusRank(b, selectedEdge)),
    ...[...edges].sort((a, b) => relationRank(a) - relationRank(b)),
  ]).slice(0, 12);
}

function beamFocusRank(edge: VisualEdge, selectedEdge: VisualEdge | null): number {
  return (selectedEdge?.id === edge.id ? -20 : 0) + relationRank(edge);
}

function uniqueEdges(edges: VisualEdge[]): VisualEdge[] {
  const seen = new Set<string>();
  return edges.filter((edge) => {
    if (seen.has(edge.id)) {
      return false;
    }
    seen.add(edge.id);
    return true;
  });
}

function nodePosition(
  nodeId: string,
  routeCards: { id: string }[],
  codeCards: { id: string }[],
  tableCards: DbInventoryTable[],
): { band: "api" | "code" | "db"; lane: number } | null {
  if (nodeId.startsWith("code:")) {
    const codeId = nodeId.slice("code:".length);
    const routeIndex = routeCards.findIndex((item) => item.id === codeId);
    if (routeIndex >= 0) {
      return { band: "api", lane: routeIndex };
    }
    const codeIndex = codeCards.findIndex((item) => item.id === codeId);
    return codeIndex >= 0 ? { band: "code", lane: codeIndex } : null;
  }
  const tableKey = tableKeyFromNodeId(nodeId);
  if (!tableKey) {
    return null;
  }
  const tableIndex = tableCards.findIndex((table) => dbInventoryTableKey(table) === tableKey);
  return tableIndex >= 0 ? { band: "db", lane: tableIndex } : null;
}

export function tableKeyFromFocusedTable(focusId: string): string | null {
  return focusId.startsWith("db:table:") ? focusId.slice("db:table:".length) : null;
}

export function relationFocusIdFromMapFocus(focusId: string): string | null {
  return focusId.startsWith("code:") || focusId.startsWith("db:table:") || focusId.startsWith("db:column:") ? focusId : null;
}

function laneCenterX(lane: number): number {
  // ponytail: mirrors fixed CSS lane sizes; measure DOM only if card widths become variable.
  return AT_GUTTER_WIDTH + AT_LANE_PAD_X + lane * (AT_LANE_WIDTH + AT_LANE_GAP) + AT_LANE_WIDTH / 2;
}

function bandCenterPercent(bands: Array<"api" | "code" | "db">, target: "api" | "code" | "db"): number {
  const weights = bands.map((band) => (band === "db" ? 1.6 : 1));
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  let before = 0;
  for (let index = 0; index < bands.length; index += 1) {
    if (bands[index] === target) {
      return ((before + weights[index] / 2) / total) * 100;
    }
    before += weights[index];
  }
  return 50;
}

function relationTone(edge: VisualEdge): RelationTone {
  if (edge.kind.startsWith("candidate")) {
    return "candidate";
  }
  if (edge.kind.startsWith("structural_")) {
    return "typed";
  }
  if (edge.kind === "contains" || edge.kind === "group_contains") {
    return "typed";
  }
  if (edge.kind === "code_flow") {
    return "inferred";
  }
  return edge.evidence.length > 0 ? "confirmed" : "typed";
}

function relationRank(edge: VisualEdge): number {
  const tone = relationTone(edge);
  if (tone === "confirmed") {
    return 0;
  }
  if (tone === "typed") {
    return 1;
  }
  return tone === "candidate" ? 2 : 3;
}

function relationLabel(tone: RelationTone): string {
  if (tone === "confirmed") {
    return "직접";
  }
  if (tone === "typed") {
    return "구조";
  }
  return tone === "candidate" ? "후보" : "이름 단서";
}

function confidenceLabel(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (normalized === "high") return "강함";
  if (normalized === "medium") return "보통";
  if (normalized === "low") return "약함";
  return value;
}

export function compactPath(path?: string | null): string | null {
  if (!path) {
    return null;
  }
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  const file = parts[parts.length - 1];
  return file && parts.length > 1 ? `.../${file}` : file ?? null;
}

export function columnMeta(column: DbInventoryTable["columns"][number]): string {
  if (column.isPrimaryKey) {
    return "PK";
  }
  if (column.isForeignKey) {
    return "FK";
  }
  return column.dataType ?? "";
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
