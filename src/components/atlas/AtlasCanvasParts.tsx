import type { ReactNode } from "react";
import type { VisualEdge, VisualNode } from "../../types/visual-map";
import {
  edgeTouchesNode,
  type RelationBeam,
  type RelationLedgerRow,
  type RelationTone,
} from "./atlasRelations";

const RELATION_ACTION_LABEL: Record<RelationTone, string> = {
  confirmed: "1차 근거",
  typed: "구조 근거",
  candidate: "검증 필요",
  inferred: "이름 단서",
};

export function RelationBeams({
  beams,
  viewBoxWidth,
  onSelect,
}: {
  beams: RelationBeam[];
  viewBoxWidth: number;
  onSelect: (edge: VisualEdge) => void;
}) {
  return (
    <svg
      className="at-relation-beams"
      aria-label="관계선"
      preserveAspectRatio="none"
      viewBox={`0 0 ${viewBoxWidth} 100`}
    >
      <defs>
        <marker id="at-beam-arrow" markerHeight="6" markerWidth="6" orient="auto" refX="5" refY="3">
          <path d="M0,0 L6,3 L0,6 Z" fill="context-stroke" />
        </marker>
      </defs>
      {beams.map((beam) => (
        <path
          aria-label={beam.label}
          className={`at-beam ${beam.tone} ${beam.active ? "active" : ""}`}
          key={beam.edge.id}
          markerEnd="url(#at-beam-arrow)"
          role="button"
          tabIndex={0}
          d={beam.path}
          onClick={() => onSelect(beam.edge)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onSelect(beam.edge);
            }
          }}
        />
      ))}
    </svg>
  );
}

export function RelationLedger({
  rows,
  selectedEdgeId,
  selectedNode,
  hasSelectedTarget,
  emptyReason,
  total,
  onSelect,
}: {
  rows: RelationLedgerRow[];
  selectedEdgeId: string | null;
  selectedNode: VisualNode | null;
  hasSelectedTarget: boolean;
  emptyReason?: string;
  total: number;
  onSelect: (edge: VisualEdge) => void;
}) {
  const title = selectedEdgeId ? "선택한 관계" : hasSelectedTarget ? "먼저 볼 관계" : "관계 우선순위";
  const hint = selectedEdgeId
    ? "근거와 양끝 항목 확인"
    : hasSelectedTarget
      ? "확정/구조 우선 · 후보 검증"
      : "확정/구조 우선 · 후보/이름 단서 검증";
  const hintTitle = "확정=읽은 근거, 구조=DB/FK/호출 구조, 후보/이름 단서=검증 필요";
  const emptyText = emptyReason ?? (hasSelectedTarget ? "이 대상과 연결된 관계가 없습니다." : "아직 표시할 관계가 없습니다.");
  const emptyNext = relationEmptyNextStep(emptyReason, hasSelectedTarget);
  const hidden = Math.max(0, total - rows.length);
  const countText = hidden > 0 ? `${rows.length}개 표시 · +${hidden}` : `${rows.length}개 전체`;

  return (
    <div className="at-edge-ledger" aria-label={title}>
      <div className="at-edge-ledger-head">
        <strong>{title}</strong>
        <span title={hintTitle}>{hint}</span>
        <em title={`${rows.length}개 표시 / 전체 ${total}개`}>{countText}</em>
      </div>
      {rows.length > 0 && (
        <div className="at-edge-columns" aria-hidden="true">
          <span>관계</span>
          <span>기준</span>
          <span />
          <span>연결 대상</span>
          <span>왜 연결됐나</span>
          <span>판단</span>
        </div>
      )}
      {rows.map((row) => (
        <button
          className={`at-edge-row ${row.tone} ${row.edge.id === selectedEdgeId ? "selected" : ""}${edgeTouchesNode(row.edge, selectedNode) ? " node-related" : ""}`}
          aria-pressed={row.edge.id === selectedEdgeId}
          key={row.edge.id}
          type="button"
          aria-label={`${row.label} 관계. 기준: ${row.fromTitle}. 연결 대상: ${row.toTitle}. 판단: ${relationLedgerAction(row.tone)}. 근거: ${row.evidence}`}
          title={`${row.label} · ${row.fromTitle} → ${row.toTitle} · ${relationLedgerAction(row.tone)} · ${row.evidence}`}
          onClick={() => onSelect(row.edge)}
        >
          <span className="at-edge-tone">{row.label}</span>
          <code data-label="기준" title={row.fromTitle}>{row.from}</code>
          <i aria-hidden="true" />
          <code data-label="연결 대상" title={row.toTitle}>{row.to}</code>
          <small>{row.evidence}</small>
          <b className="at-edge-action">{row.edge.id === selectedEdgeId ? "선택됨" : relationLedgerAction(row.tone)}</b>
        </button>
      ))}
      {rows.length === 0 && (
        <span className="at-edge-empty">
          <b>{emptyText}</b>
          <small>{emptyNext}</small>
        </span>
      )}
    </div>
  );
}

function relationLedgerAction(tone: RelationTone): string {
  return RELATION_ACTION_LABEL[tone];
}

function relationEmptyNextStep(emptyReason: string | undefined, hasSelectedTarget: boolean): string {
  if (emptyReason?.includes("컬럼 구조") || emptyReason?.includes("컬럼을 읽으면")) {
    return "DB 카드에서 컬럼을 보강하면 FK와 영향 근거를 확인할 수 있습니다.";
  }
  if (hasSelectedTarget) {
    return "다른 카드나 상단 검색으로 범위를 넓혀 주변 관계를 다시 확인하세요.";
  }
  return "카드를 선택하거나 상단 검색으로 API, 코드, 테이블, 컬럼을 먼저 좁히세요.";
}


export function Band({
  num,
  label,
  total,
  shown,
  last,
  children,
}: {
  num: string;
  label: string;
  total: number;
  shown: number;
  last?: boolean;
  children: ReactNode;
}) {
  const hidden = Math.max(0, total - shown);
  const countText = hidden > 0 ? `${shown}개 표시 · +${hidden}` : `${shown}개 전체`;
  return (
    <section className={`at-band ${last ? "last" : ""}`}>
      <div className="at-gutter">
        <span className="at-num">{num}</span>
        <h3>{label}</h3>
        <small title={`${shown}개 표시 / 전체 ${total}개`}>{countText}</small>
      </div>
      <div className="at-cards">{children}</div>
    </section>
  );
}
