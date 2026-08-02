import { ArrowLeft, ArrowRight, Cog, Database, FileCode2, GitBranch, Table2 } from "lucide-react";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { visualEdgeKindLabel, visualEdgeTruthClass } from "../../visual/labels";

type ImpactLink = {
  node: VisualNode;
  edges: VisualEdge[];
  tone: "confirmed" | "structural" | "candidate" | "inferred";
};

type CodeImpactProjection = {
  focus: VisualNode;
  incoming: ImpactLink[];
  outgoing: ImpactLink[];
  database: ImpactLink[];
};

export function buildCodeImpactProjection(map: VisualMap, focusId: string): CodeImpactProjection | null {
  const focus = map.nodes.find((node) => node.id === focusId);
  if (!focus) return null;

  const nodesById = new Map(map.nodes.map((node) => [node.id, node]));
  const incoming = new Map<string, ImpactLink>();
  const outgoing = new Map<string, ImpactLink>();
  const database = new Map<string, ImpactLink>();

  for (const edge of map.edges) {
    const from = nodesById.get(edge.from);
    const to = nodesById.get(edge.to);
    if (!from || !to) continue;
    if (edge.to === focusId && from.source !== "db") addLink(incoming, from, edge);
    if (edge.from === focusId && to.source !== "db") addLink(outgoing, to, edge);
    if (edge.from === focusId && to.source === "db") addLink(database, to, edge);
    if (edge.to === focusId && from.source === "db") addLink(database, from, edge);
  }

  return {
    focus,
    incoming: sortLinks([...incoming.values()]),
    outgoing: sortLinks([...outgoing.values()]),
    database: sortLinks([...database.values()]),
  };
}

export function CodeImpactMap({
  map,
  focusId,
  fallbackNode,
  selectedNodeId,
  selectedEdgeId,
  onSelectNode,
  onSelectEdge,
}: {
  map: VisualMap;
  focusId: string;
  fallbackNode?: VisualNode | null;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  const projection = buildCodeImpactProjection(map, focusId);
  const focus = projection?.focus ?? fallbackNode ?? {
    id: focusId,
    kind: "function",
    title: focusId.replace(/^code:/, "") || "선택 코드",
    layer: "code",
    source: "code",
  };
  const incoming = projection?.incoming ?? [];
  const outgoing = projection?.outgoing ?? [];
  const database = projection?.database ?? [];
  const disconnected = incoming.length === 0 && outgoing.length === 0 && database.length === 0;
  const sideRows = Math.max(1, incoming.length, outgoing.length);
  const sideHeight = Math.max(260, sideRows * 88 + 24);
  const databaseHeight = Math.max(156, 66 + Math.max(1, Math.ceil(database.length / 4)) * 58);
  const canvasHeight = sideHeight + databaseHeight;
  const focusTop = Math.max(20, (sideHeight - 92) / 2);
  const width = 1060;
  const leftX = 26;
  const centerX = 404;
  const rightX = 782;
  const centerWidth = 250;
  const sideWidth = 300;
  const positions = new Map<string, { x: number; y: number; width: number }>();
  incoming.forEach((link, index) => positions.set(`in:${link.node.id}`, { x: leftX, y: 20 + index * 88, width: sideWidth }));
  outgoing.forEach((link, index) => positions.set(`out:${link.node.id}`, { x: rightX, y: 20 + index * 88, width: sideWidth }));

  return (
    <section className={`at-code-impact${disconnected ? " at-disconnected-focus" : ""}`} aria-label={`${focus.title} 코드 영향 지도`}>
      <header className="at-code-impact-head">
        <div>
          <span><FileCode2 size={15} /> 코드 변경 영향</span>
          <strong>{focus.title}</strong>
          <small>{focus.subtitle ?? "선택한 코드 기준 직접 1단계 관계"}</small>
        </div>
        <div className="at-code-impact-counts" aria-label="코드 영향 요약">
          <span><b>{incoming.length}</b> 호출자</span>
          <span><b>{outgoing.length}</b> 호출 대상</span>
          <span><b>{database.length}</b> DB</span>
        </div>
      </header>

      <div className="at-code-impact-scroll">
        <div className="at-code-impact-canvas" style={{ width, minHeight: canvasHeight }}>
          <svg className="at-code-impact-lines" width={width} height={canvasHeight} viewBox={`0 0 ${width} ${canvasHeight}`} aria-hidden="true">
            {incoming.map((link) => {
              const position = positions.get(`in:${link.node.id}`)!;
              return <ImpactPath key={`in:${link.node.id}`} x1={position.x + position.width} y1={position.y + 45} x2={centerX} y2={focusTop + 46} tone={link.tone} />;
            })}
            {outgoing.map((link) => {
              const position = positions.get(`out:${link.node.id}`)!;
              return <ImpactPath key={`out:${link.node.id}`} x1={centerX + centerWidth} y1={focusTop + 46} x2={position.x} y2={position.y + 45} tone={link.tone} />;
            })}
          </svg>

          <div className="at-code-impact-lane incoming" style={{ left: leftX, width: sideWidth }}>
            <span className="at-code-impact-lane-title"><ArrowRight size={13} /> 호출하는 API·코드</span>
            <div className="at-code-impact-items">
              {incoming.length > 0 ? incoming.map((link) => (
                <ImpactLinkCard link={link} direction="incoming" selectedNodeId={selectedNodeId} selectedEdgeId={selectedEdgeId} onSelectNode={onSelectNode} onSelectEdge={onSelectEdge} key={link.node.id} />
              )) : <EmptyImpactSlot text="확인된 호출자를 찾지 못했습니다." />}
            </div>
          </div>

          <button
            className={`at-code-impact-focus ${selectedNodeId === focus.id ? "selected" : ""}`}
            style={{ left: centerX, top: focusTop, width: centerWidth }}
            type="button"
            aria-pressed={selectedNodeId === focus.id}
            onClick={() => onSelectNode(focus)}
          >
            <span><Cog size={14} /> 선택 코드</span>
            <strong>{focus.title}</strong>
            <small>{focus.subtitle ?? focus.location?.path ?? "소스 위치 없음"}</small>
          </button>

          <div className="at-code-impact-lane outgoing" style={{ left: rightX, width: sideWidth }}>
            <span className="at-code-impact-lane-title"><ArrowLeft size={13} /> 호출하는 코드</span>
            <div className="at-code-impact-items">
              {outgoing.length > 0 ? outgoing.map((link) => (
                <ImpactLinkCard link={link} direction="outgoing" selectedNodeId={selectedNodeId} selectedEdgeId={selectedEdgeId} onSelectNode={onSelectNode} onSelectEdge={onSelectEdge} key={link.node.id} />
              )) : <EmptyImpactSlot text="확인된 호출 대상을 찾지 못했습니다." />}
            </div>
          </div>

          <section className="at-code-impact-database" aria-label="DB 영향">
            <header><Database size={14} /><strong>DB 영향</strong><span>직접 READ / WRITE 근거</span></header>
            <div>
              {database.length > 0 ? database.map((link) => (
                <ImpactLinkCard link={link} direction="database" selectedNodeId={selectedNodeId} selectedEdgeId={selectedEdgeId} onSelectNode={onSelectNode} onSelectEdge={onSelectEdge} key={link.node.id} />
              )) : <EmptyImpactSlot text="선택 코드에 직접 연결된 DB 근거가 없습니다. 미사용 확정은 아닙니다." />}
            </div>
          </section>
        </div>
      </div>

      <div className="at-code-impact-legend" aria-label="코드 영향 지도 범례">
        <span className="confirmed">실선 확정</span>
        <span className="structural">회색 구조</span>
        <span className="candidate">주황 점선 후보</span>
        <span className="inferred">흐린 점선 이름 단서</span>
      </div>
      {disconnected ? (
        <p className="at-code-impact-disconnected-note">
          <strong>확인된 직접 관계가 없습니다</strong>
          <small>
            {Math.max(0, map.nodes.length - 1) > 0
              ? `같은 분석 범위의 ${Math.max(0, map.nodes.length - 1).toLocaleString("ko-KR")}개 항목은 관계 근거가 없어 지도에서 분리했습니다.`
              : "오른쪽 근거에서 소스 위치와 다음 확인 항목을 볼 수 있습니다."}
          </small>
        </p>
      ) : null}
    </section>
  );
}

function ImpactLinkCard({
  link,
  direction,
  selectedNodeId,
  selectedEdgeId,
  onSelectNode,
  onSelectEdge,
}: {
  link: ImpactLink;
  direction: "incoming" | "outgoing" | "database";
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  return (
    <article className={`at-code-impact-link ${link.tone} ${direction}`}>
      <button className={`at-code-impact-node ${selectedNodeId === link.node.id ? "selected" : ""}`} type="button" onClick={() => onSelectNode(link.node)}>
        {direction === "database" ? <Table2 size={14} /> : link.node.layer === "api" ? <GitBranch size={14} /> : <FileCode2 size={14} />}
        <strong title={link.node.title}>{link.node.title}</strong>
        <small title={link.node.subtitle ?? undefined}>{link.node.subtitle ?? link.node.location?.path ?? "위치 정보 없음"}</small>
      </button>
      <div className="at-code-impact-edge-list">
        {link.edges.map((edge) => (
          <button className={selectedEdgeId === edge.id ? "selected" : ""} type="button" onClick={() => onSelectEdge(edge)} key={edge.id}>
            {impactRelationLabel(edge)}
          </button>
        ))}
      </div>
    </article>
  );
}

function EmptyImpactSlot({ text }: { text: string }) {
  return <p className="at-code-impact-empty">{text}</p>;
}

function ImpactPath({ x1, y1, x2, y2, tone }: { x1: number; y1: number; x2: number; y2: number; tone: ImpactLink["tone"] }) {
  const bend = (x1 + x2) / 2;
  return <path className={`at-code-impact-path ${tone}`} d={`M ${x1} ${y1} C ${bend} ${y1}, ${bend} ${y2}, ${x2} ${y2}`} />;
}

function addLink(target: Map<string, ImpactLink>, node: VisualNode, edge: VisualEdge) {
  const current = target.get(node.id);
  if (current) {
    current.edges.push(edge);
    current.tone = strongerTone(current.tone, edgeTone(edge));
    return;
  }
  target.set(node.id, { node, edges: [edge], tone: edgeTone(edge) });
}

function sortLinks(links: ImpactLink[]): ImpactLink[] {
  return links.sort((left, right) => {
    const tone = toneRank(left.tone) - toneRank(right.tone);
    return tone || left.node.title.localeCompare(right.node.title, "ko-KR") || left.node.id.localeCompare(right.node.id);
  });
}

function edgeTone(edge: VisualEdge): ImpactLink["tone"] {
  return visualEdgeTruthClass(edge);
}

function strongerTone(left: ImpactLink["tone"], right: ImpactLink["tone"]): ImpactLink["tone"] {
  return toneRank(left) <= toneRank(right) ? left : right;
}

function toneRank(tone: ImpactLink["tone"]): number {
  return tone === "confirmed" ? 0 : tone === "structural" ? 1 : tone === "candidate" ? 2 : 3;
}

function impactRelationLabel(edge: VisualEdge): string {
  if (edge.kind === "code_handle") return "HANDLES";
  if (edge.kind === "code_call") return "CALLS";
  if (edge.kind === "code_db_read") return "READS";
  if (edge.kind === "code_db_write") return "WRITES";
  return visualEdgeKindLabel(edge);
}
