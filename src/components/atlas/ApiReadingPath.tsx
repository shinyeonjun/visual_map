import {
  Box,
  Braces,
  Database,
  FileCode2,
  GitBranch,
  Layers3,
  List,
  Table2,
  Workflow,
} from "lucide-react";
import type { ComponentType, CSSProperties } from "react";
import type { DbInventoryTable } from "../../types/workspace";
import { dbInventoryTableKey, routeMethodFromIdentity } from "../../types/workspace";
import type {
  ApiReadingAnswer,
  ApiReadingStep,
  ImpactReviewItem,
  VisualEdge,
  VisualMap,
  VisualNode,
} from "../../types/visual-map";
import { tableKeyFromDbNodeId } from "../../visual/nodeIds";
import { buildApiConnectionModel } from "./apiConnectionModel";

type ApiConnectionModel = ReturnType<typeof buildApiConnectionModel>;

export type ApiReadingView = "connections" | "layers" | "list";

const NODE_WIDTH = 156;
const NODE_HEIGHT = 126;
const NODE_GAP = 48;
const CANVAS_PAD = 24;
const NODE_TOP = 250;

const viewOptions: Array<{ id: ApiReadingView; label: string; icon: ComponentType<{ size?: number }> }> = [
  { id: "connections", label: "연결 지도", icon: Workflow },
  { id: "layers", label: "계층", icon: Layers3 },
  { id: "list", label: "목록", icon: List },
];

export function ApiReadingHeader({
  answer,
  map,
  view,
  onViewChange,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  view: ApiReadingView;
  onViewChange: (view: ApiReadingView) => void;
}) {
  const method = answer.method ?? routeMethodFromIdentity(map.focus);
  const confirmed = map.edges.filter(isConfirmedApiEdge).length;
  const candidates = map.edges.filter(isCandidateEdge).length;
  const databaseRelations = answer.dbRelations?.length ?? 0;

  return (
    <div className="api-map-heading">
      <div className="api-map-question">
        <span>
          API <i>/</i> {method ?? "ROUTE"} <i>/</i> <code>{answer.subject}</code>
        </span>
        <strong>요청이 DB까지 어떻게 이어지나요?</strong>
        <small>
          <em className="confirmed">확정 {confirmed}</em>
          <em className="confirmed">DB 연결 {databaseRelations}</em>
          <em className="candidate">후보 {candidates}</em>
          <em className={answer.unknowns.length > 0 ? "unknown" : "quiet"}>확인 안 됨 {answer.unknowns.length}</em>
        </small>
      </div>
      <div className="api-view-switch" role="group" aria-label="API 경로 보기 방식">
        {viewOptions.map(({ id, label, icon: ViewIcon }) => (
          <button
            className={view === id ? "active" : ""}
            type="button"
            data-api-view={id}
            aria-pressed={view === id}
            onClick={() => onViewChange(id)}
            key={id}
          >
            <ViewIcon size={14} />
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}

export function ApiReadingPath({
  answer,
  map,
  view,
  selectedNodeId,
  selectedEdgeId,
  dbTables,
  onSelectNode,
  onSelectEdge,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  view: ApiReadingView;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  dbTables: DbInventoryTable[];
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  if (view === "layers") {
    return (
      <ApiLayerView
        answer={answer}
        map={map}
        selectedNodeId={selectedNodeId}
        onSelectNode={onSelectNode}
      />
    );
  }
  if (view === "list") {
    return (
      <ApiListView
        answer={answer}
        map={map}
        selectedNodeId={selectedNodeId}
        selectedEdgeId={selectedEdgeId}
        onSelectNode={onSelectNode}
        onSelectEdge={onSelectEdge}
      />
    );
  }

  return (
    <ApiConnectionView
      answer={answer}
      map={map}
      selectedNodeId={selectedNodeId}
      selectedEdgeId={selectedEdgeId}
      dbTables={dbTables}
      onSelectNode={onSelectNode}
      onSelectEdge={onSelectEdge}
    />
  );
}

export function ApiConnectionView({
  answer,
  map,
  selectedNodeId,
  selectedEdgeId,
  dbTables,
  onSelectNode,
  onSelectEdge,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  dbTables: DbInventoryTable[];
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  const model = buildApiConnectionModel(answer, map);
  const graph = buildApiGraphLayout(answer, map, model);
  const method = answer.method ?? routeMethodFromIdentity(map.focus);
  const gapVisible = Boolean(model.gap && model.primaryPath.length <= 1);

  return (
    <section className="api-connection-view" aria-label={`${answer.subject} 연결 지도`}>
      <div className="api-connection-canvas" style={{ width: graph.width, height: graph.height }}>
        <svg className="api-connection-lines" viewBox={`0 0 ${graph.width} ${graph.height}`} aria-hidden="true">
          <defs>
            <marker id="api-confirmed-arrow" markerHeight="7" markerWidth="7" orient="auto" refX="6" refY="3.5">
              <path d="M0,0 L7,3.5 L0,7 Z" />
            </marker>
            <marker id="api-candidate-arrow" markerHeight="7" markerWidth="7" orient="auto" refX="6" refY="3.5">
              <path d="M0,0 L7,3.5 L0,7 Z" />
            </marker>
          </defs>
          {graph.edges.map(({ edge, from, to }) => (
            <path
              className={`api-edge-line ${isCandidateEdge(edge) ? "candidate" : "confirmed"}${selectedEdgeId === edge.id ? " selected" : ""}`}
              d={apiGraphEdgePath({ edge, from, to }, model.primaryDatabase?.edge.id ?? null)}
              markerEnd={isCandidateEdge(edge) ? "url(#api-candidate-arrow)" : "url(#api-confirmed-arrow)"}
              key={edge.id}
            />
          ))}
        </svg>

        {graph.edges.map((connection) => (
          <button
            className={`api-edge-label ${isCandidateEdge(connection.edge) ? "candidate" : "confirmed"}${selectedEdgeId === connection.edge.id ? " selected" : ""}`}
            style={apiGraphEdgeLabelStyle(connection, model.primaryDatabase?.edge.id ?? null)}
            type="button"
            data-edge-id={connection.edge.id}
            aria-label={`${connection.from.node.title} ${relationLabel(connection.edge)} ${connection.to.node.title} 근거 보기`}
            title={connection.edge.evidence[0]?.text ?? relationLabel(connection.edge)}
            onClick={() => onSelectEdge(connection.edge)}
            key={`label-${connection.edge.id}`}
          >
            {relationLabel(connection.edge)}
          </button>
        ))}

        {graph.nodes.map(({ node, item, primaryIndex, x, y }) => (
          <ApiDiagramNode
            item={item}
            node={node}
            method={primaryIndex === 0 ? method : null}
            table={dbTableForNode(node, dbTables)}
            selected={selectedNodeId === node.id}
            style={{ left: x, top: y }}
            onSelect={() => onSelectNode(node)}
            key={node.id}
          />
        ))}

        {gapVisible && model.gap ? (
          <div className="api-gap-node" style={{ left: graph.gapX, top: NODE_TOP }}>
            <GitBranch size={16} />
            <strong>{model.gap.title}</strong>
            <span>{model.gap.detail}</span>
          </div>
        ) : null}

        <div className="api-connection-legend" aria-label="연결 지도 범례">
          <span><i className="confirmed" /> 확정 연결</span>
          <span><i className="candidate" /> 후보 연결</span>
          <span><i className="unknown" /> 확인 안 됨</span>
        </div>
      </div>
      {answer.truncated || !model.primaryDatabase ? (
        <div className="api-map-notices">
          {answer.truncated ? (
            <span className="truncated">
              {answer.hiddenBranchesIsLowerBound ? "최소 " : ""}+{answer.hiddenBranches}개 관계가 엔진 표시 범위 밖에 있습니다.
            </span>
          ) : null}
          {!model.primaryDatabase ? (
            <span>현재 확정 코드 경로에 연결된 DB 근거를 찾지 못했습니다. DB 미사용이 확정된 것은 아닙니다.</span>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function ApiDiagramNode({
  item,
  node,
  method,
  table,
  selected,
  style,
  onSelect,
}: {
  item?: ApiReadingStep | ImpactReviewItem;
  node: VisualNode;
  method?: string | null;
  table?: DbInventoryTable | null;
  selected: boolean;
  style: CSSProperties;
  onSelect: () => void;
}) {
  const lane = item ? ("lane" in item ? item.lane : item.truthClass === "confirmed" ? "db-relation" : "db-candidate") : visualNodeLane(node);
  const laneMeta = apiLaneMeta(lane);
  const NodeIcon = laneMeta.icon;
  const inferred = Boolean(item && "lane" in item && item.laneBasis === "name-inferred");
  const location = item && "lane" in item ? sourceLocationLabel(item.location) : node.subtitle;
  const title = item?.title ?? node.title;

  return (
    <button
      className={`api-diagram-node ${laneMeta.tone}${selected ? " selected" : ""}`}
      style={style}
      type="button"
      data-node-id={node.id}
      aria-pressed={selected}
      aria-label={`${laneMeta.label} ${method ? `${method} ` : ""}${title} 선택`}
      onClick={onSelect}
    >
      <span className="api-node-kind">
        <NodeIcon size={14} />
        {laneMeta.label}
        {inferred ? <em title="이 역할 이름은 심볼명으로 분류했습니다">역할 추정</em> : null}
      </span>
      <strong>{method ? `${method} ${title}` : title}</strong>
      {table?.columns.length ? (
        <span className="api-node-columns">
          {table.columns.slice(0, 3).map((column) => (
            <code key={column.name}>{column.name}{column.isPrimaryKey ? " (PK)" : ""}</code>
          ))}
        </span>
      ) : (
        <small title={location ?? undefined}>{location ?? node.subtitle ?? "위치 정보 없음"}</small>
      )}
    </button>
  );
}

function ApiLayerView({
  answer,
  map,
  selectedNodeId,
  onSelectNode,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  selectedNodeId: string | null;
  onSelectNode: (node: VisualNode) => void;
}) {
  const lanes = ["route", "handler", "service-function", "repository-query", "database"];
  return (
    <section className="api-layer-view" aria-label={`${answer.subject} 계층 보기`}>
      {lanes.map((lane) => {
        const meta = apiLaneMeta(lane);
        const LaneIcon = meta.icon;
        const items: Array<ApiReadingStep | ImpactReviewItem> = lane === "database"
          ? [...(answer.dbRelations ?? []), ...answer.dbCandidates]
          : answer.steps.filter((step) => step.lane === lane);
        return (
          <section key={lane}>
            <header><LaneIcon size={14} /><strong>{meta.label}</strong><span>{items.length}</span></header>
            <div>
              {items.length === 0 ? <p>{laneEmptyMessage(lane)}</p> : null}
              {items.map((item) => {
                const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
                return node ? (
                  <button
                    className={selectedNodeId === node.id ? "selected" : ""}
                    type="button"
                    onClick={() => onSelectNode(node)}
                    key={item.id}
                  >
                    <strong>{item.title}</strong>
                    <small>{"laneBasis" in item && item.laneBasis === "name-inferred" ? "역할 추정 · " : ""}{sourceLocationLabel(item.location) ?? item.detail}</small>
                  </button>
                ) : null;
              })}
            </div>
          </section>
        );
      })}
    </section>
  );
}

function ApiListView({
  answer,
  map,
  selectedNodeId,
  selectedEdgeId,
  onSelectNode,
  onSelectEdge,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  const confirmedEdges = map.edges.filter(isConfirmedApiEdge);
  return (
    <section className="api-list-view" aria-label={`${answer.subject} 목록 보기`}>
      <header><span>순서</span><span>대상</span><span>관계 근거</span><span>위치</span></header>
      {answer.steps.map((step, index) => {
        const node = step.nodeId ? map.nodes.find((candidate) => candidate.id === step.nodeId) ?? null : null;
        const incoming = step.nodeId
          ? confirmedEdges.find((edge) => edge.to === step.nodeId) ?? null
          : null;
        return (
          <div className={node && selectedNodeId === node.id ? "selected" : ""} key={step.id}>
            <span>{String(index + 1).padStart(2, "0")}</span>
            {node ? <button type="button" onClick={() => onSelectNode(node)}>{step.title}</button> : <strong>{step.title}</strong>}
            {incoming ? (
              <button
                className={selectedEdgeId === incoming.id ? "selected" : ""}
                type="button"
                onClick={() => onSelectEdge(incoming)}
              >
                {relationLabel(incoming)}
              </button>
            ) : <span>엔진 진입점</span>}
            <code title={step.location?.path}>{sourceLocationLabel(step.location) ?? "위치 정보 없음"}</code>
          </div>
        );
      })}
      {[...(answer.dbRelations ?? []), ...answer.dbCandidates].map((item, index) => {
        const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
        const edge = item.nodeId ? map.edges.find((candidate) => isDatabaseEdge(candidate) && candidate.to === item.nodeId) ?? null : null;
        const candidate = item.truthClass !== "confirmed";
        return (
          <div className={`${candidate ? "candidate" : "confirmed"}${node && selectedNodeId === node.id ? " selected" : ""}`} key={item.id}>
            <span>{candidate ? "C" : "D"}{index + 1}</span>
            {node ? <button type="button" onClick={() => onSelectNode(node)}>{item.title}</button> : <strong>{item.title}</strong>}
            {edge ? <button type="button" onClick={() => onSelectEdge(edge)}>{relationLabel(edge)}</button> : <span>{candidate ? "후보 근거" : "확정 근거"}</span>}
            <code title={item.detail}>{item.confidence ? `후보 강도 ${candidateStrength(item.confidence)}` : candidate ? "검증 필요" : "정적 SQL 근거"}</code>
          </div>
        );
      })}
    </section>
  );
}

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

const BRANCH_START_GAP = 104;
const BRANCH_ROW_GAP = 48;

function buildApiGraphLayout(answer: ApiReadingAnswer, map: VisualMap, model: ApiConnectionModel): ApiGraphLayout {
  const nodesById = new Map(map.nodes.map((node) => [node.id, node]));
  const itemByNodeId = new Map<string, ApiReadingStep | ImpactReviewItem>();
  for (const item of [...answer.steps, ...(answer.dbRelations ?? []), ...answer.dbCandidates]) {
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
    rawPositions.set(node.id, { x: nodeX(index), y: NODE_TOP, primaryIndex: index });
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
        y: NODE_TOP + NODE_HEIGHT + BRANCH_START_GAP + (depth - 1) * (NODE_HEIGHT + BRANCH_ROW_GAP),
      });
    });
  }

  const minX = Math.min(...[...rawPositions.values()].map(({ x }) => x), CANVAS_PAD);
  const shiftX = Math.max(0, CANVAS_PAD - minX);
  const graphNodes = [...nodeIds].map((nodeId) => {
    const node = nodesById.get(nodeId)!;
    const position = rawPositions.get(nodeId) ?? { x: nodeX(0), y: NODE_TOP };
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
  const maxBottom = Math.max(...graphNodes.map(({ y }) => y + NODE_HEIGHT), NODE_TOP + NODE_HEIGHT);
  const minimumWidth = CANVAS_PAD * 2 + Math.max(2, primaryEntries.length) * NODE_WIDTH + Math.max(0, Math.max(2, primaryEntries.length) - 1) * NODE_GAP;
  const gapX = nodeX(1) + shiftX;

  return {
    nodes: graphNodes,
    edges: graphEdges,
    width: Math.max(minimumWidth, maxRight + CANVAS_PAD),
    height: Math.max(720, maxBottom + 112),
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

function apiGraphEdgePath(connection: ApiGraphEdge, primaryDatabaseEdgeId: string | null): string {
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

function apiGraphEdgeLabelStyle(connection: ApiGraphEdge, primaryDatabaseEdgeId: string | null): CSSProperties {
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

function apiLaneMeta(lane: string): {
  label: string;
  tone: string;
  icon: ComponentType<{ size?: number }>;
} {
  if (lane === "route") return { label: "API / Route", tone: "route", icon: Braces };
  if (lane === "handler") return { label: "Handler", tone: "handler", icon: Box };
  if (lane === "repository-query") return { label: "Repository / Query", tone: "repository", icon: Database };
  if (lane === "database") return { label: "DB Table", tone: "database", icon: Table2 };
  if (lane === "db-relation") return { label: "DB Table · 확정", tone: "database", icon: Table2 };
  if (lane === "db-candidate") return { label: "DB Table · 후보", tone: "database", icon: Table2 };
  return { label: "Service / Function", tone: "service", icon: FileCode2 };
}

function relationLabel(edge: VisualEdge): string {
  if (edge.kind === "code_handle") return "HANDLES";
  if (edge.kind === "code_call") return "CALLS";
  if (edge.kind === "code_db_read") return "READS";
  if (edge.kind === "code_db_write") return "WRITES";
  if (isCandidateEdge(edge)) return "DB 후보";
  return edge.kind;
}

function isConfirmedApiEdge(edge: VisualEdge): boolean {
  return edge.kind === "code_handle" || edge.kind === "code_call";
}

function isCandidateEdge(edge: VisualEdge): boolean {
  return edge.kind.startsWith("candidate");
}

function isDatabaseEdge(edge: VisualEdge): boolean {
  return isCandidateEdge(edge) || edge.kind === "code_db_read" || edge.kind === "code_db_write";
}

function sourceLocationLabel(location?: { path: string; line?: number | null } | null): string | null {
  if (!location) return null;
  return `${location.path}${location.line ? `:${location.line}` : ""}`;
}

function dbTableForNode(node: VisualNode, tables: DbInventoryTable[]): DbInventoryTable | null {
  const tableKey = tableKeyFromDbNodeId(node.id);
  return tableKey ? tables.find((table) => dbInventoryTableKey(table) === tableKey) ?? null : null;
}

function visualNodeLane(node: VisualNode): string {
  const kind = node.kind.toLowerCase();
  if (node.layer === "db" || kind === "table") return "database";
  if (node.layer === "api" || kind === "api" || kind === "route") return "route";
  if (kind.includes("handler")) return "handler";
  if (kind.includes("repository")) return "repository-query";
  return "service-function";
}

function laneEmptyMessage(lane: string): string {
  if (lane === "handler") return "확정 HANDLES 대상을 찾지 못했습니다.";
  if (lane === "database") return "현재 확정 경로에서 DB 연결 근거를 찾지 못했습니다.";
  return "이 역할로 분류된 항목이 없습니다.";
}

function candidateStrength(confidence: string): string {
  if (confidence === "high") return "강함";
  if (confidence === "medium") return "중간";
  return "약함";
}
