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
import { useLayoutEffect, useRef } from "react";
import type { ComponentType, CSSProperties } from "react";
import type { DbInventoryTable } from "../../types/workspace";
import { dbInventoryTableKey, routeDisplayName, routeMethodFromIdentity } from "../../types/workspace";
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
import {
  API_GRAPH_NODE_TOP,
  apiGraphEdgeLabelStyle,
  apiGraphEdgePath,
  buildApiGraphLayout,
} from "./apiReadingGraph";

export type ApiReadingView = "connections" | "layers" | "list";

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
  const clientRequests = answer.clientRequests?.length ?? 0;
  const clientRequestTone = clientRequests === 0
    ? "quiet"
    : answer.clientRequests?.some((item) => item.truthClass !== "confirmed")
      ? "candidate"
      : "confirmed";

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
          <em className={clientRequestTone}>클라이언트 요청 {clientRequests}</em>
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
  const viewRef = useRef<HTMLElement>(null);

  useLayoutEffect(() => {
    if (viewRef.current) {
      viewRef.current.scrollLeft = 0;
      viewRef.current.scrollTop = 0;
    }
  }, [answer.subject, map.focus]);

  return (
    <section ref={viewRef} className="api-connection-view" aria-label={`${answer.subject} 연결 지도`}>
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

        {graph.nodes.map(({ node, item, x, y }) => (
          <ApiDiagramNode
            item={item}
            node={node}
            method={item && "lane" in item && item.lane === "route" ? method : null}
            table={dbTableForNode(node, dbTables)}
            selected={selectedNodeId === node.id}
            style={{ left: x, top: y }}
            onSelect={() => onSelectNode(node)}
            key={node.id}
          />
        ))}

        {gapVisible && model.gap ? (
          <div className="api-gap-node" style={{ left: graph.gapX, top: API_GRAPH_NODE_TOP }}>
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
      {model.collapsedEdges.length > 0 ? (
        <details className="api-collapsed-relations">
          <summary>보조 관계 {model.collapsedEdges.length}개 · 겹침 방지를 위해 접혀 있음</summary>
          <div>
            {model.collapsedEdges.map((edge) => {
              const from = map.nodes.find((node) => node.id === edge.from);
              const to = map.nodes.find((node) => node.id === edge.to);
              return (
                <button type="button" key={edge.id} onClick={() => onSelectEdge(edge)}>
                  <strong>{from?.title ?? edge.from}</strong>
                  <span>{relationLabel(edge)}</span>
                  <strong>{to?.title ?? edge.to}</strong>
                </button>
              );
            })}
          </div>
        </details>
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
  const lane = item
    ? ("lane" in item
      ? item.lane
      : item.kind === "client-request"
        ? "client-request"
        : item.truthClass === "confirmed" ? "db-relation" : "db-candidate")
    : visualNodeLane(node);
  const laneMeta = apiLaneMeta(lane);
  const NodeIcon = laneMeta.icon;
  const inferred = Boolean(item && "lane" in item && item.laneBasis === "name-inferred");
  const location = item && "lane" in item ? sourceLocationLabel(item.location) : node.subtitle;
  const title = item?.title ?? node.title;
  const displayTitle = method ? routeDisplayName(title, method) : title;

  return (
    <button
      className={`api-diagram-node ${laneMeta.tone}${selected ? " selected" : ""}`}
      style={style}
      type="button"
      data-node-id={node.id}
      aria-pressed={selected}
      aria-label={`${laneMeta.label} ${displayTitle} 선택`}
      onClick={onSelect}
    >
      <span className="api-node-kind">
        <NodeIcon size={14} />
        {laneMeta.label}
        {inferred ? <em title="이 역할 이름은 심볼명으로 분류했습니다">역할 추정</em> : null}
      </span>
      <strong>{displayTitle}</strong>
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

function apiLaneMeta(lane: string): {
  label: string;
  tone: string;
  icon: ComponentType<{ size?: number }>;
} {
  if (lane === "route") return { label: "Backend API", tone: "route", icon: Braces };
  if (lane === "client-request") return { label: "Client Request", tone: "client-request", icon: GitBranch };
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
  if (edge.kind === "client_request") return "REQUESTS";
  if (edge.kind === "code_db_read") return "READS";
  if (edge.kind === "code_db_write") return "WRITES";
  if (isCandidateEdge(edge)) return "DB 후보";
  return edge.kind;
}

function isConfirmedApiEdge(edge: VisualEdge): boolean {
  return edge.kind === "code_handle" || edge.kind === "code_call";
}

function isCandidateEdge(edge: VisualEdge): boolean {
  return edge.kind.startsWith("candidate") || edge.confidence === "candidate";
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
