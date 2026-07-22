import {
  Box,
  Braces,
  Database,
  FileCode2,
  GitBranch,
  Layers3,
  List,
  Network,
  Table2,
  Workflow,
} from "lucide-react";
import { useEffect, useState } from "react";
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

function ApiConnectionView({
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
  const [branchesOpen, setBranchesOpen] = useState(false);
  useEffect(() => setBranchesOpen(false), [map.focus]);
  const model = buildApiConnectionModel(answer, map);
  const method = answer.method ?? routeMethodFromIdentity(map.focus);
  const visibleNodes = model.primaryDatabase
    ? [...model.primaryPath, model.primaryDatabase]
    : model.primaryPath;
  const gapVisible = Boolean(model.gap && model.primaryPath.length <= 1);
  const slotCount = Math.max(2, visibleNodes.length + (gapVisible ? 1 : 0));
  const width = CANVAS_PAD * 2 + slotCount * NODE_WIDTH + Math.max(0, slotCount - 1) * NODE_GAP;
  const drawerHeight = branchesOpen && model.additionalEdges.length > 0 ? 178 : 0;
  const height = 720 + drawerHeight;
  const primaryDatabaseIndex = model.primaryDatabase ? visibleNodes.length - 1 : -1;
  const databaseSourceIndex = model.primaryDatabase
    ? model.primaryPath.findIndex(({ node }) => node.id === model.primaryDatabase?.edge.from)
    : -1;

  return (
    <section className="api-connection-view" aria-label={`${answer.subject} 연결 지도`}>
      <div className="api-connection-canvas" style={{ width, height }}>
        <svg className="api-connection-lines" viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
          <defs>
            <marker id="api-confirmed-arrow" markerHeight="7" markerWidth="7" orient="auto" refX="6" refY="3.5">
              <path d="M0,0 L7,3.5 L0,7 Z" />
            </marker>
            <marker id="api-candidate-arrow" markerHeight="7" markerWidth="7" orient="auto" refX="6" refY="3.5">
              <path d="M0,0 L7,3.5 L0,7 Z" />
            </marker>
          </defs>
          {model.primaryEdges.map((edge, index) => {
            const startX = nodeX(index) + NODE_WIDTH;
            const endX = nodeX(index + 1);
            const y = NODE_TOP + NODE_HEIGHT / 2;
            return (
              <path
                className={`api-edge-line confirmed${selectedEdgeId === edge.id ? " selected" : ""}`}
                d={`M ${startX} ${y} L ${endX} ${y}`}
                markerEnd="url(#api-confirmed-arrow)"
                key={edge.id}
              />
            );
          })}
          {model.primaryDatabase && databaseSourceIndex >= 0 ? (
            <path
              className={`api-edge-line ${isCandidateEdge(model.primaryDatabase.edge) ? "candidate" : "confirmed"}${selectedEdgeId === model.primaryDatabase.edge.id ? " selected" : ""}`}
              d={candidateCurve(databaseSourceIndex, primaryDatabaseIndex)}
              markerEnd={isCandidateEdge(model.primaryDatabase.edge) ? "url(#api-candidate-arrow)" : "url(#api-confirmed-arrow)"}
            />
          ) : null}
        </svg>

        {model.primaryPath.map(({ item, node }, index) => (
          <ApiDiagramNode
            item={item}
            node={node}
            method={index === 0 ? method : null}
            selected={selectedNodeId === node.id}
            style={{ left: nodeX(index), top: NODE_TOP }}
            onSelect={() => onSelectNode(node)}
            key={node.id}
          />
        ))}

        {model.primaryEdges.map((edge, index) => (
          <button
            className={`api-edge-label confirmed${selectedEdgeId === edge.id ? " selected" : ""}`}
            style={{ left: nodeX(index) + NODE_WIDTH + 3, top: NODE_TOP + 38 }}
            type="button"
            data-edge-id={edge.id}
            title={edge.evidence[0]?.text ?? relationLabel(edge)}
            onClick={() => onSelectEdge(edge)}
            key={`label-${edge.id}`}
          >
            {relationLabel(edge)}
          </button>
        ))}

        {model.primaryDatabase ? (
          <>
            <ApiDiagramNode
              item={model.primaryDatabase.item}
              node={model.primaryDatabase.node}
              table={dbTableForNode(model.primaryDatabase.node, dbTables)}
              selected={selectedNodeId === model.primaryDatabase.node.id}
              style={{ left: nodeX(primaryDatabaseIndex), top: NODE_TOP }}
              onSelect={() => onSelectNode(model.primaryDatabase!.node)}
            />
            <button
              className={`api-candidate-label${!isCandidateEdge(model.primaryDatabase.edge) ? " confirmed" : ""}${selectedEdgeId === model.primaryDatabase.edge.id ? " selected" : ""}`}
              type="button"
              data-edge-id={model.primaryDatabase.edge.id}
              style={{ left: nodeX(databaseSourceIndex) + NODE_WIDTH / 2, top: NODE_TOP + NODE_HEIGHT + 82 }}
              title={model.primaryDatabase.edge.evidence[0]?.text ?? relationLabel(model.primaryDatabase.edge)}
              onClick={() => onSelectEdge(model.primaryDatabase!.edge)}
            >
              {relationLabel(model.primaryDatabase.edge)}
            </button>
          </>
        ) : null}

        {gapVisible && model.gap ? (
          <div className="api-gap-node" style={{ left: nodeX(1), top: NODE_TOP }}>
            <GitBranch size={16} />
            <strong>{model.gap.title}</strong>
            <span>{model.gap.detail}</span>
          </div>
        ) : null}

        {model.additionalEdges.length > 0 ? (
          <button
            className="api-branch-toggle"
            type="button"
            aria-expanded={branchesOpen}
            onClick={() => setBranchesOpen(!branchesOpen)}
          >
            <Network size={13} />
            +{model.additionalEdges.length} 연결
          </button>
        ) : null}

        {branchesOpen && model.additionalEdges.length > 0 ? (
          <ApiBranchDrawer
            edges={model.additionalEdges}
            map={map}
            selectedEdgeId={selectedEdgeId}
            onSelectEdge={onSelectEdge}
          />
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
  item: ApiReadingStep | ImpactReviewItem;
  node: VisualNode;
  method?: string | null;
  table?: DbInventoryTable | null;
  selected: boolean;
  style: CSSProperties;
  onSelect: () => void;
}) {
  const lane = "lane" in item ? item.lane : item.truthClass === "confirmed" ? "db-relation" : "db-candidate";
  const laneMeta = apiLaneMeta(lane);
  const NodeIcon = laneMeta.icon;
  const inferred = "laneBasis" in item && item.laneBasis === "name-inferred";
  const location = "lane" in item ? sourceLocationLabel(item.location) : node.subtitle;

  return (
    <button
      className={`api-diagram-node ${laneMeta.tone}${selected ? " selected" : ""}`}
      style={style}
      type="button"
      data-node-id={node.id}
      aria-pressed={selected}
      aria-label={`${laneMeta.label} ${method ? `${method} ` : ""}${item.title} 선택`}
      onClick={onSelect}
    >
      <span className="api-node-kind">
        <NodeIcon size={14} />
        {laneMeta.label}
        {inferred ? <em title="이 역할 이름은 심볼명으로 분류했습니다">역할 추정</em> : null}
      </span>
      <strong>{method ? `${method} ${item.title}` : item.title}</strong>
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

function ApiBranchDrawer({
  edges,
  map,
  selectedEdgeId,
  onSelectEdge,
}: {
  edges: VisualEdge[];
  map: VisualMap;
  selectedEdgeId: string | null;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  return (
    <section className="api-branch-drawer" aria-label="추가 연결">
      <header>
        <strong>추가 연결</strong>
        <span>주 경로 밖의 실제 관계 {edges.length}개</span>
      </header>
      <div>
        {edges.map((edge) => (
          <button
            className={`${isCandidateEdge(edge) ? "candidate" : "confirmed"}${selectedEdgeId === edge.id ? " selected" : ""}`}
            type="button"
            data-edge-id={edge.id}
            onClick={() => onSelectEdge(edge)}
            key={edge.id}
          >
            <code>{nodeTitle(edge.from, map)}</code>
            <span>{relationLabel(edge)}</span>
            <code>{nodeTitle(edge.to, map)}</code>
          </button>
        ))}
      </div>
    </section>
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

function nodeX(index: number): number {
  return CANVAS_PAD + index * (NODE_WIDTH + NODE_GAP);
}

function candidateCurve(sourceIndex: number, targetIndex: number): string {
  const startX = nodeX(sourceIndex) + NODE_WIDTH / 2;
  const startY = NODE_TOP + NODE_HEIGHT;
  const endX = nodeX(targetIndex) + NODE_WIDTH / 2;
  const endY = NODE_TOP + NODE_HEIGHT;
  const curveY = startY + 104;
  return `M ${startX} ${startY} C ${startX} ${curveY}, ${endX} ${curveY}, ${endX} ${endY}`;
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

function nodeTitle(nodeId: string, map: VisualMap): string {
  return map.nodes.find((node) => node.id === nodeId)?.title ?? nodeId;
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
