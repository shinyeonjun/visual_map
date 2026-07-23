import { ArrowLeft, ChevronRight, Cog, FileText, Layers3, List, Network, Table2 } from "lucide-react";
import { useState } from "react";
import type { CSSProperties } from "react";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { visualEdgeTruthClass, visualNodeKindLabel as nodeKindLabel } from "../../visual/labels";

const OVERVIEW_GROUP_LIMIT = 7;
const OVERVIEW_CONNECTION_LIMIT = 10;
const ARCHITECTURE_MAP_WIDTH = 720;
const ARCHITECTURE_CARD_WIDTH = 170;
const ARCHITECTURE_CARD_HEIGHT = 88;
const ARCHITECTURE_ROW_STEP = 106;
const ARCHITECTURE_CARD_TOP = 58;

type ArchitectureLane = "api" | "code" | "db";

type PositionedDomain = {
  node: VisualNode;
  summary: DomainCardSummary | null;
  lane: ArchitectureLane;
  x: number;
  y: number;
  index: number;
};

type ArchitectureConnection = {
  edges: VisualEdge[];
  representative: VisualEdge;
  from: PositionedDomain;
  to: PositionedDomain;
  tone: "confirmed" | "typed" | "candidate" | "inferred";
};

const architectureLanes: ArchitectureLane[] = ["api", "code", "db"];
const architectureLaneX: Record<ArchitectureLane, number> = { api: 14, code: 275, db: 536 };
const architectureLaneLabel: Record<ArchitectureLane, string> = {
  api: "API 경계",
  code: "코드 영역",
  db: "DB 스키마",
};

export type RelationSummary = {
  confirmed: number;
  typed: number;
  inferred: number;
  candidate: number;
};

type DomainCardSummary = {
  api: number;
  code: number;
  db: number;
  topApi: string;
  topCode: string;
  topDb: string;
};

export function ArchitectureMap({
  map,
  relationCounts,
  selectedNodeId,
  selectedEdgeId,
  onBack,
  onOpenGroup,
  onOpenMember,
  onSelectEdge,
}: {
  map: VisualMap;
  relationCounts: Map<string, RelationSummary>;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  onBack: () => void;
  onOpenGroup: (node: VisualNode) => void;
  onOpenMember: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  const groupNodes = map.nodes.filter((node) => node.kind === "group-domain");
  const [showAllGroups, setShowAllGroups] = useState(false);
  const detailGroup = map.focus.startsWith("group:") ? groupNodes.find((node) => node.id === map.focus) ?? null : null;
  const visibleGroupNodes = groupNodes.slice(0, OVERVIEW_GROUP_LIMIT);
  const hiddenGroupCount = Math.max(0, groupNodes.length - visibleGroupNodes.length);

  if (!detailGroup) {
    return (
      <section className="at-architecture" aria-label="패키지와 DB 스키마 기반 전체 구조">
        <div className="at-architecture-notes" aria-label="전체 구조 표시 범위">
          {map.warnings.map((warning) => (
            <span key={warning}>{warning}</span>
          ))}
        </div>
        {showAllGroups ? (
          <ArchitectureGroupList
            nodes={groupNodes}
            relationCounts={relationCounts}
            onOpenGroup={onOpenGroup}
            onBackToMap={() => setShowAllGroups(false)}
          />
        ) : (
          <ArchitectureOverviewMap
            map={map}
            nodes={visibleGroupNodes}
            hiddenGroupCount={hiddenGroupCount}
            relationCounts={relationCounts}
            selectedEdgeId={selectedEdgeId}
            onOpenGroup={onOpenGroup}
            onSelectEdge={onSelectEdge}
            onShowAll={() => setShowAllGroups(true)}
          />
        )}
      </section>
    );
  }

  const members = map.nodes.filter((node) => node.id !== detailGroup.id && !node.id.startsWith("group:"));
  const api = members.filter((node) => node.layer === "api");
  const code = members.filter((node) => node.source === "code" && node.layer !== "api");
  const db = members.filter((node) => node.source === "db" && node.kind === "table");

  return (
    <section className="at-architecture at-architecture-detail" aria-label={`${detailGroup.title} 구조 영역 상세`}>
      <div className="at-domain-detail-head">
        <button type="button" data-atlas-action="overview" onClick={onBack} aria-label="전체 구조로 돌아가기"><ArrowLeft size={14} /> 전체 구조</button>
        <span>선택 구조 영역</span>
        <strong>{detailGroup.title}</strong>
        <small>{detailGroup.subtitle?.split("|")[0] ?? "구조 영역 항목"}</small>
      </div>
      <div className="at-architecture-notes" aria-label="구조 영역 상세 표시 범위">
        {map.warnings.map((warning) => (
          <span key={warning}>{warning}</span>
        ))}
      </div>
      <ArchitectureMemberBand number="1" label="API" nodes={api} selectedNodeId={selectedNodeId} relationCounts={relationCounts} onOpen={onOpenMember} />
      <ArchitectureMemberBand number="2" label="코드" nodes={code} selectedNodeId={selectedNodeId} relationCounts={relationCounts} onOpen={onOpenMember} />
      <ArchitectureMemberBand number="3" label="DB" nodes={db} selectedNodeId={selectedNodeId} relationCounts={relationCounts} onOpen={onOpenMember} />
    </section>
  );
}

function ArchitectureOverviewMap({
  map,
  nodes,
  hiddenGroupCount,
  relationCounts,
  selectedEdgeId,
  onOpenGroup,
  onSelectEdge,
  onShowAll,
}: {
  map: VisualMap;
  nodes: VisualNode[];
  hiddenGroupCount: number;
  relationCounts: Map<string, RelationSummary>;
  selectedEdgeId: string | null;
  onOpenGroup: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
  onShowAll: () => void;
}) {
  const positions = layoutOverviewNodes(nodes);
  const connections = groupOverviewConnections(map, positions);
  const visibleConnections = connections.slice(0, OVERVIEW_CONNECTION_LIMIT);
  const visibleEdgeCount = visibleConnections.reduce((sum, connection) => sum + connection.edges.length, 0);
  const hiddenEdgeCount = Math.max(0, map.edges.length - visibleEdgeCount);
  const rows = Math.max(1, ...architectureLanes.map((lane) => positions.filter((position) => position.lane === lane).length));
  const mapHeight = ARCHITECTURE_CARD_TOP + rows * ARCHITECTURE_ROW_STEP + (visibleConnections.length === 0 ? 58 : 12);

  return (
    <>
      <div className="at-architecture-overview-head">
        <div>
          <Network size={16} aria-hidden="true" />
          <strong>영역 연결 지도</strong>
          <span>{nodes.length}개 핵심 영역 · 관계 {map.edges.length.toLocaleString("ko-KR")}개</span>
        </div>
        <div className="at-architecture-legend" aria-label="관계 판단 범례">
          <span className="confirmed">확정</span>
          <span className="typed">구조</span>
          <span className="candidate">후보</span>
        </div>
      </div>
      <div className="at-architecture-map-scroll">
        <div className="at-architecture-map" style={{ height: mapHeight }}>
          {architectureLanes.map((lane) => (
            <div
              className={`at-architecture-lane ${lane}`}
              key={lane}
              style={{ left: architectureLaneX[lane] - 10 }}
              aria-hidden="true"
            >
              <span>{architectureLaneLabel[lane]}</span>
              <small>{positions.filter((position) => position.lane === lane).length}</small>
            </div>
          ))}
          <svg
            className="at-architecture-connections"
            width={ARCHITECTURE_MAP_WIDTH}
            height={mapHeight}
            viewBox={`0 0 ${ARCHITECTURE_MAP_WIDTH} ${mapHeight}`}
            aria-label={`표시된 구조 영역 관계 ${visibleEdgeCount}개`}
          >
            <defs>
              {(["confirmed", "typed", "candidate", "inferred"] as const).map((tone) => (
                <marker
                  className={`at-architecture-marker ${tone}`}
                  id={`architecture-arrow-${tone}`}
                  key={tone}
                  markerWidth="7"
                  markerHeight="7"
                  refX="6"
                  refY="3.5"
                  orient="auto"
                  markerUnits="strokeWidth"
                >
                  <path d="M 0 0 L 7 3.5 L 0 7 z" />
                </marker>
              ))}
            </defs>
            {visibleConnections.map((connection, connectionIndex) => {
              const selected = connection.edges.some((edge) => edge.id === selectedEdgeId);
              const path = architectureConnectionPath(connection, mapHeight, connectionIndex);
              const fromTitle = connection.from.node.title;
              const toTitle = connection.to.node.title;
              const relationLabel = architectureConnectionLabel(connection);
              return (
                <g
                  className={`at-architecture-edge ${connection.tone} ${selected ? "selected" : ""}`}
                  data-architecture-edge={connection.representative.id}
                  role="button"
                  tabIndex={0}
                  aria-label={`${fromTitle}에서 ${toTitle} 관계. ${relationLabel}`}
                  aria-pressed={selected}
                  key={`${connection.representative.id}:${connection.edges.length}`}
                  onClick={() => onSelectEdge(connection.representative)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onSelectEdge(connection.representative);
                    }
                  }}
                >
                  <title>{`${fromTitle} → ${toTitle} · ${relationLabel}`}</title>
                  <path className="at-architecture-edge-hit" d={path} />
                  <path
                    className="at-architecture-edge-line"
                    d={path}
                    markerEnd={`url(#architecture-arrow-${connection.tone})`}
                  />
                </g>
              );
            })}
          </svg>
          {positions.map((position) => (
            <ArchitectureGroupCard
              compact
              key={position.node.id}
              node={position.node}
              index={position.index}
              summary={position.summary}
              relationSummary={relationCounts.get(position.node.id)}
              lane={position.lane}
              style={{ left: position.x, top: position.y }}
              onOpen={() => onOpenGroup(position.node)}
            />
          ))}
          {visibleConnections.length === 0 && (
            <div className="at-architecture-no-relations" role="status">
              <strong>영역 간 연결 근거가 없습니다</strong>
              <span>각 영역의 상세 항목은 열어볼 수 있습니다.</span>
            </div>
          )}
        </div>
      </div>
      {(hiddenGroupCount > 0 || hiddenEdgeCount > 0) && (
        <div className="at-architecture-overflow">
          <span>
            {hiddenGroupCount > 0 && `영역 ${hiddenGroupCount.toLocaleString("ko-KR")}개`}
            {hiddenGroupCount > 0 && hiddenEdgeCount > 0 && " · "}
            {hiddenEdgeCount > 0 && `관계 ${hiddenEdgeCount.toLocaleString("ko-KR")}개`}
            {" 표시 범위 밖"}
          </span>
          {hiddenGroupCount > 0 && (
            <button className="at-architecture-more" type="button" onClick={onShowAll}>
              <List size={14} aria-hidden="true" /> 전체 영역 목록
            </button>
          )}
        </div>
      )}
    </>
  );
}

function ArchitectureGroupList({
  nodes,
  relationCounts,
  onOpenGroup,
  onBackToMap,
}: {
  nodes: VisualNode[];
  relationCounts: Map<string, RelationSummary>;
  onOpenGroup: (node: VisualNode) => void;
  onBackToMap: () => void;
}) {
  return (
    <>
      <div className="at-architecture-list-head">
        <div><List size={15} aria-hidden="true" /><strong>전체 구조 영역</strong><span>{nodes.length.toLocaleString("ko-KR")}개</span></div>
        <button type="button" onClick={onBackToMap}><Network size={14} aria-hidden="true" /> 연결 지도로 돌아가기</button>
      </div>
      <div className="at-domain-grid at-domain-list">
        {nodes.map((node, index) => (
          <ArchitectureGroupCard
            key={node.id}
            node={node}
            index={index}
            summary={parseDomainCardSummary(node.subtitle)}
            relationSummary={relationCounts.get(node.id)}
            onOpen={() => onOpenGroup(node)}
          />
        ))}
      </div>
    </>
  );
}

function ArchitectureGroupCard({
  node,
  index,
  summary,
  relationSummary,
  compact = false,
  lane,
  style,
  onOpen,
}: {
  node: VisualNode;
  index: number;
  summary: DomainCardSummary | null;
  relationSummary?: RelationSummary;
  compact?: boolean;
  lane?: ArchitectureLane;
  style?: CSSProperties;
  onOpen: () => void;
}) {
  const summaryLabel = summary ? `API ${summary.api}, 코드 ${summary.code}, DB ${summary.db}` : node.subtitle ?? "요약 없음";
  if (compact) {
    const primaryFact = architecturePrimaryFact(summary, lane);
    return (
      <button
        className={`at-domain-card at-domain-map-card ${lane ?? "code"}`}
        type="button"
        style={style}
        aria-label={`${node.title} 구조 영역 열기. ${summaryLabel}`}
        title={`${node.title} 구조 영역 상세 열기`}
        onClick={onOpen}
      >
        <div className="at-domain-map-card-head">
          <Layers3 size={14} aria-hidden="true" />
          <strong>{node.title}</strong>
          <RelationBadge summary={relationSummary} />
        </div>
        {summary ? (
          <div className="at-domain-map-counts" aria-label="구조 영역 항목 수">
            <span><b>API</b>{summary.api}</span>
            <span><b>코드</b>{summary.code}</span>
            <span><b>DB</b>{summary.db}</span>
          </div>
        ) : (
          <small>{node.subtitle ?? "표시할 요약이 없습니다"}</small>
        )}
        <div className="at-domain-map-fact">
          <code title={primaryFact || "대표 항목 없음"}>{primaryFact || "대표 항목 없음"}</code>
          <ChevronRight size={13} aria-hidden="true" />
        </div>
      </button>
    );
  }
  return (
    <button
      className="at-domain-card"
      type="button"
      aria-label={`${node.title} 구조 영역 열기. ${summaryLabel}`}
      title={`${node.title} 구조 영역 상세 열기`}
      onClick={onOpen}
    >
      <div className="at-domain-head">
        <span aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
        <Layers3 size={15} aria-hidden="true" />
        <strong>{node.title}</strong>
        <RelationBadge summary={relationSummary} />
      </div>
      {summary ? (
        <>
          <div className="at-domain-counts" aria-label="구조 영역 항목 수">
            <span><b>API</b><strong>{summary.api}</strong></span>
            <span><b>코드</b><strong>{summary.code}</strong></span>
            <span><b>DB</b><strong>{summary.db}</strong></span>
          </div>
          <div className="at-domain-facts">
            <span><b>API</b><code title={summary.topApi || "API 없음"}>{summary.topApi || "없음"}</code></span>
            <span><b>코드</b><code title={summary.topCode || "코드 없음"}>{summary.topCode || "없음"}</code></span>
            <span><b>DB</b><code title={summary.topDb || "DB 없음"}>{summary.topDb || "없음"}</code></span>
          </div>
        </>
      ) : (
        <small>{node.subtitle ?? "표시할 요약이 없습니다"}</small>
      )}
      <span className="at-domain-open">상세 보기 <ChevronRight size={13} aria-hidden="true" /></span>
    </button>
  );
}

function ArchitectureMemberBand({
  number,
  label,
  nodes,
  selectedNodeId,
  relationCounts,
  onOpen,
}: {
  number: string;
  label: string;
  nodes: VisualNode[];
  selectedNodeId: string | null;
  relationCounts: Map<string, RelationSummary>;
  onOpen: (node: VisualNode) => void;
}) {
  return (
    <section className="at-domain-band" data-domain-band={number} aria-label={`${label} ${nodes.length}개`}>
      <header>
        <span>{number}</span>
        <strong>{label}</strong>
        <small>{nodes.length}개</small>
      </header>
      <div>
        {nodes.map((node) => (
          <button
            type="button"
            className={`at-domain-member ${selectedNodeId === node.id ? "selected" : ""}`}
            key={node.id}
            aria-pressed={selectedNodeId === node.id}
            aria-label={`${node.title} ${nodeKindLabel(node.kind, node.source)} 선택`}
            title={`${node.title} · ${node.subtitle ?? nodeKindLabel(node.kind, node.source)}`}
            onClick={() => onOpen(node)}
          >
            {node.source === "db" ? <Table2 size={14} /> : node.kind === "file" ? <FileText size={14} /> : <Cog size={14} />}
            <strong>{node.title}</strong>
            <span>{nodeKindLabel(node.kind, node.source)}</span>
            <RelationBadge summary={relationCounts.get(node.id)} />
            {node.subtitle && <small>{compactPath(node.subtitle) ?? node.subtitle}</small>}
          </button>
        ))}
        {nodes.length === 0 && <span className="at-domain-band-empty">이 계층에 읽힌 항목이 없습니다</span>}
      </div>
    </section>
  );
}

export function RelationBadge({ summary }: { summary?: RelationSummary }) {
  if (!summary) {
    return null;
  }
  const total = summary.confirmed + summary.typed + summary.inferred + summary.candidate;
  if (total === 0) {
    return null;
  }
  const dominant =
    summary.confirmed > 0
      ? { label: "확정", count: summary.confirmed }
      : summary.typed > 0
        ? { label: "구조", count: summary.typed }
        : summary.candidate > 0
          ? { label: "후보", count: summary.candidate }
          : { label: "이름 단서", count: summary.inferred };
  const badgeLabel = dominant.label === "이름 단서" ? "단서" : dominant.label;
  const label = `${badgeLabel} ${dominant.count}/${total}`;
  const tone = summary.confirmed > 0 ? "confirmed" : summary.typed > 0 ? "typed" : summary.candidate > 0 ? "candidate" : "inferred";
  const title = `카드 선택 시 답 화면 열기 · 관계 ${total}개 · 확정 ${summary.confirmed} · 구조 ${summary.typed} · 후보 ${summary.candidate} · 이름 단서 ${summary.inferred}`;
  return (
    <span className={`at-relation-badge ${tone}`} title={title} aria-label={title}>
      {label}
    </span>
  );
}

function parseDomainCardSummary(value?: string | null): DomainCardSummary | null {
  if (!value) {
    return null;
  }
  const [counts, topApi = "", topCode = "", topDb = ""] = value.split("|");
  const match = /^API (\d+) · 코드 (\d+) · DB (\d+)$/.exec(counts);
  if (!match) {
    return null;
  }
  return {
    api: Number(match[1]),
    code: Number(match[2]),
    db: Number(match[3]),
    topApi,
    topCode,
    topDb,
  };
}

function compactPath(path?: string | null): string | null {
  if (!path) {
    return null;
  }
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  const file = parts[parts.length - 1];
  return file && parts.length > 1 ? `.../${file}` : file ?? null;
}

function layoutOverviewNodes(nodes: VisualNode[]): PositionedDomain[] {
  const byLane: Record<ArchitectureLane, Array<Omit<PositionedDomain, "x" | "y">>> = {
    api: [],
    code: [],
    db: [],
  };
  nodes.forEach((node, index) => {
    const summary = parseDomainCardSummary(node.subtitle);
    const lane = architectureLane(summary);
    byLane[lane].push({ node, summary, lane, index });
  });
  const maxRows = Math.max(1, ...architectureLanes.map((lane) => byLane[lane].length));
  return architectureLanes
    .flatMap((lane) => {
      const verticalOffset = ((maxRows - byLane[lane].length) * ARCHITECTURE_ROW_STEP) / 2;
      return byLane[lane].map((domain, row) => ({
        ...domain,
        x: architectureLaneX[lane],
        y: ARCHITECTURE_CARD_TOP + verticalOffset + row * ARCHITECTURE_ROW_STEP,
      }));
    })
    .sort((left, right) => left.index - right.index);
}

function architectureLane(summary: DomainCardSummary | null): ArchitectureLane {
  if (!summary) {
    return "code";
  }
  if (summary.api > 0) {
    return "api";
  }
  if (summary.db > 0 && summary.db >= summary.code) {
    return "db";
  }
  return "code";
}

function groupOverviewConnections(map: VisualMap, positions: PositionedDomain[]): ArchitectureConnection[] {
  const positionById = new Map(positions.map((position) => [position.node.id, position]));
  const grouped = new Map<string, VisualEdge[]>();
  map.edges.forEach((edge) => {
    if (!positionById.has(edge.from) || !positionById.has(edge.to) || edge.from === edge.to) {
      return;
    }
    const pair = [edge.from, edge.to].sort().join("\u0000");
    const edges = grouped.get(pair) ?? [];
    edges.push(edge);
    grouped.set(pair, edges);
  });
  return [...grouped.values()]
    .map((edges) => {
      edges.sort((left, right) => architectureEdgeRank(left) - architectureEdgeRank(right));
      const representative = edges[0];
      return {
        edges,
        representative,
        from: positionById.get(representative.from)!,
        to: positionById.get(representative.to)!,
        tone: architectureEdgeTone(representative),
      };
    })
    .sort((left, right) => {
      const rank = architectureEdgeRank(left.representative) - architectureEdgeRank(right.representative);
      if (rank !== 0) {
        return rank;
      }
      const leftLaneSpan = Math.abs(architectureLanes.indexOf(left.from.lane) - architectureLanes.indexOf(left.to.lane));
      const rightLaneSpan = Math.abs(architectureLanes.indexOf(right.from.lane) - architectureLanes.indexOf(right.to.lane));
      if (leftLaneSpan !== rightLaneSpan) {
        return rightLaneSpan - leftLaneSpan;
      }
      const leftSpan = Math.abs(left.from.x - left.to.x) + Math.abs(left.from.y - left.to.y);
      const rightSpan = Math.abs(right.from.x - right.to.x) + Math.abs(right.from.y - right.to.y);
      return leftSpan - rightSpan;
    });
}

function architectureConnectionPath(connection: ArchitectureConnection, mapHeight: number, track: number): string {
  const fromY = connection.from.y + ARCHITECTURE_CARD_HEIGHT / 2;
  const toY = connection.to.y + ARCHITECTURE_CARD_HEIGHT / 2;
  if (connection.from.x === connection.to.x) {
    const useLeft = connection.from.lane === "db";
    const fromX = connection.from.x + (useLeft ? 0 : ARCHITECTURE_CARD_WIDTH);
    const toX = connection.to.x + (useLeft ? 0 : ARCHITECTURE_CARD_WIDTH);
    const trackOffset = 18 + (track % 8) * 5;
    const loopX = fromX + (useLeft ? -trackOffset : trackOffset);
    return `M ${fromX} ${fromY} C ${loopX} ${fromY}, ${loopX} ${toY}, ${toX} ${toY}`;
  }
  const movingRight = connection.to.x > connection.from.x;
  const fromX = connection.from.x + (movingRight ? ARCHITECTURE_CARD_WIDTH : 0);
  const toX = connection.to.x + (movingRight ? 0 : ARCHITECTURE_CARD_WIDTH);
  const laneSpan = Math.abs(architectureLanes.indexOf(connection.from.lane) - architectureLanes.indexOf(connection.to.lane));
  if (laneSpan > 1) {
    const routeY = mapHeight - 8 - (track % 3) * 4;
    const fromElbow = fromX + (movingRight ? 24 : -24);
    const toElbow = toX + (movingRight ? -24 : 24);
    return `M ${fromX} ${fromY} C ${fromElbow} ${fromY}, ${fromElbow} ${routeY}, ${fromElbow} ${routeY} L ${toElbow} ${routeY} C ${toElbow} ${routeY}, ${toElbow} ${toY}, ${toX} ${toY}`;
  }
  const control = Math.min(112, Math.abs(toX - fromX) * 0.42);
  return `M ${fromX} ${fromY} C ${fromX + (movingRight ? control : -control)} ${fromY}, ${toX + (movingRight ? -control : control)} ${toY}, ${toX} ${toY}`;
}

function architectureConnectionLabel(connection: ArchitectureConnection): string {
  const toneLabel = connection.tone === "confirmed"
    ? "확정"
    : connection.tone === "typed"
      ? "구조"
      : connection.tone === "candidate"
        ? "후보"
        : "이름 단서";
  return connection.edges.length > 1
    ? `${toneLabel} 관계 ${connection.edges.length.toLocaleString("ko-KR")}개 묶음`
    : `${toneLabel} 관계`;
}

function architectureEdgeTone(edge: VisualEdge): ArchitectureConnection["tone"] {
  const truthClass = visualEdgeTruthClass(edge);
  return truthClass === "structural" ? "typed" : truthClass;
}

function architectureEdgeRank(edge: VisualEdge): number {
  const tone = architectureEdgeTone(edge);
  return tone === "confirmed" ? 0 : tone === "typed" ? 1 : tone === "candidate" ? 2 : 3;
}

function architecturePrimaryFact(summary: DomainCardSummary | null, lane?: ArchitectureLane): string {
  if (!summary) {
    return "";
  }
  if (lane === "api") {
    return summary.topApi || summary.topCode || summary.topDb;
  }
  if (lane === "db") {
    return summary.topDb || summary.topCode || summary.topApi;
  }
  return summary.topCode || summary.topApi || summary.topDb;
}
