import { ArrowLeft, ChevronRight, Cog, FileText, Layers3, Table2 } from "lucide-react";
import { useState } from "react";
import type { VisualMap, VisualNode } from "../../types/visual-map";
import { visualNodeKindLabel as nodeKindLabel } from "../../visual/labels";

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
  onBack,
  onOpenGroup,
  onOpenMember,
}: {
  map: VisualMap;
  relationCounts: Map<string, RelationSummary>;
  selectedNodeId: string | null;
  onBack: () => void;
  onOpenGroup: (node: VisualNode) => void;
  onOpenMember: (node: VisualNode) => void;
}) {
  const groupNodes = map.nodes.filter((node) => node.kind === "group-domain");
  const [showAllGroups, setShowAllGroups] = useState(false);
  const detailGroup = map.focus.startsWith("group:") ? groupNodes.find((node) => node.id === map.focus) ?? null : null;
  const visibleGroupNodes = showAllGroups ? groupNodes : groupNodes.slice(0, 7);
  const hiddenGroupCount = Math.max(0, groupNodes.length - visibleGroupNodes.length);

  if (!detailGroup) {
    return (
      <section className="at-architecture" aria-label="패키지와 DB 스키마 기반 전체 구조">
        <div className="at-architecture-notes" aria-label="전체 구조 표시 범위">
          {map.warnings.map((warning) => (
            <span key={warning}>{warning}</span>
          ))}
        </div>
        <div className="at-domain-grid">
          {visibleGroupNodes.map((node, index) => {
            const summary = parseDomainCardSummary(node.subtitle);
            return (
              <button
                className="at-domain-card"
                type="button"
                key={node.id}
                aria-label={`${node.title} 구조 영역 열기. ${summary ? `API ${summary.api}, 코드 ${summary.code}, DB ${summary.db}` : node.subtitle ?? "요약 없음"}`}
                title={`${node.title} 구조 영역 상세 열기`}
                onClick={() => onOpenGroup(node)}
              >
                <div className="at-domain-head">
                  <span aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                  <Layers3 size={15} />
                  <strong>{node.title}</strong>
                  <RelationBadge summary={relationCounts.get(node.id)} />
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
                <span className="at-domain-open">상세 보기 <ChevronRight size={13} /></span>
              </button>
            );
          })}
        </div>
        {hiddenGroupCount > 0 && (
          <button className="at-architecture-more" type="button" onClick={() => setShowAllGroups(true)}>
            +{hiddenGroupCount.toLocaleString("ko-KR")}개 구조 영역 모두 보기
          </button>
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
            aria-label={`${node.title} ${nodeKindLabel(node.kind)} 선택`}
            title={`${node.title} · ${node.subtitle ?? nodeKindLabel(node.kind)}`}
            onClick={() => onOpen(node)}
          >
            {node.source === "db" ? <Table2 size={14} /> : node.kind === "file" ? <FileText size={14} /> : <Cog size={14} />}
            <strong>{node.title}</strong>
            <span>{nodeKindLabel(node.kind)}</span>
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
      ? { label: "직접", count: summary.confirmed }
      : summary.typed > 0
        ? { label: "구조", count: summary.typed }
        : summary.candidate > 0
          ? { label: "후보", count: summary.candidate }
          : { label: "이름 단서", count: summary.inferred };
  const badgeLabel = dominant.label === "이름 단서" ? "단서" : dominant.label;
  const label = `${badgeLabel} ${dominant.count}/${total}`;
  const tone = summary.confirmed > 0 ? "confirmed" : summary.typed > 0 ? "typed" : summary.candidate > 0 ? "candidate" : "inferred";
  const title = `카드 선택 시 답 화면 열기 · 관계 ${total}개 · 직접 ${summary.confirmed} · 구조 ${summary.typed} · 후보 ${summary.candidate} · 이름 단서 ${summary.inferred}`;
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
