import type { VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";

type RelationCounts = {
  confirmed: number;
  typed: number;
  inferred: number;
  candidate: number;
};

export function ContextRibbon({
  workspaceControls,
  visualMapControls,
}: {
  workspaceControls: WorkspaceControls;
  visualMapControls: VisualMapControls;
}) {
  const map = visualMapControls.currentMap;
  const selected = selectedContext(workspaceControls, visualMapControls, map);
  const next = nextCheckContext(workspaceControls, visualMapControls, map);
  const read = readContext(workspaceControls, visualMapControls, map);
  const counts = relationCounts(map?.edges ?? []);
  const trustTone = counts.candidate > 0 || counts.inferred > 0 ? "candidate" : counts.confirmed > 0 || counts.typed > 0 ? "confirmed" : "neutral";

  return (
    <div className="context-ribbon" aria-label="현재 분석 컨텍스트">
      <RibbonCell label="답 기준" value={selected.value} detail={selected.detail} tone={selected.tone} />
      <RibbonCell label="다음 행동" value={next.value} detail={next.detail} tone={next.tone} />
      <RibbonCell label={read.label} value={read.value} detail={read.detail} />
      <RibbonCell
        label="근거 상태"
        value={relationValue(counts)}
        detail={relationDetail(counts)}
        tone={trustTone}
      />
    </div>
  );
}

function RibbonCell({
  label,
  value,
  detail,
  tone = "neutral",
}: {
  label: string;
  value: string;
  detail: string;
  tone?: "confirmed" | "candidate" | "neutral";
}) {
  return (
    <div className={`context-cell ${tone}`}>
      <span>{label}</span>
      <strong title={value}>{value}</strong>
      <small title={detail}>{detail}</small>
    </div>
  );
}

function selectedContext(
  workspaceControls: WorkspaceControls,
  visualMapControls: VisualMapControls,
  map: VisualMap | null,
): { value: string; detail: string; tone: "confirmed" | "candidate" | "neutral" } {
  if (visualMapControls.selectedEdge) {
    return {
      value: edgeTitle(visualMapControls.selectedEdge, map),
      detail: edgeKindLabel(visualMapControls.selectedEdge),
      tone: visualMapControls.selectedEdge.kind.startsWith("candidate") ? "candidate" : "confirmed",
    };
  }
  if (visualMapControls.selectedNode) {
    return {
      value: nodeLabel(visualMapControls.selectedNode.id, map),
      detail: nodeKindLabel(visualMapControls.selectedNode),
      tone: visualMapControls.selectedNode.source === "projection" ? "neutral" : "confirmed",
    };
  }
  const focusId = meaningfulFocus(map);
  if (focusId) {
    const activeMap = map;
    if (!activeMap) {
      return {
        value: workspaceControls.currentWorkspace?.name ?? "전체 프로젝트",
        detail: "전체 프로젝트",
        tone: "neutral",
      };
    }
    const focusNode = activeMap.nodes.find((item) => item.id === focusId);
    return {
      value: nodeLabel(focusId, activeMap),
      detail: focusNode ? `${nodeKindLabel(focusNode)} · ${modeLabel(activeMap.mode)}` : modeLabel(activeMap.mode),
      tone: focusNode?.source === "projection" ? "neutral" : "confirmed",
    };
  }
  if (map) {
    return {
      value: modeLabel(map.mode),
      detail: workspaceControls.currentWorkspace?.name ?? "전체 프로젝트",
      tone: "neutral",
    };
  }
  if (workspaceControls.selectedCodeItem) {
    return {
      value: workspaceControls.selectedCodeItem.name,
      detail: workspaceControls.selectedCodeItem.filePath ?? workspaceControls.selectedCodeItem.kind,
      tone: "confirmed",
    };
  }
  return {
    value: workspaceControls.currentWorkspace?.name ?? (workspaceControls.canCreateWorkspace ? workspaceReadyLabel(workspaceControls) : workspaceNeedLabel(workspaceControls)),
    detail: workspaceControls.currentWorkspace
      ? "전체 프로젝트"
      : workspaceControls.canCreateWorkspace
        ? workspaceControls.repoSourceMode === "github" ? "저장소 복제" : "프로젝트 열기"
        : workspaceControls.repoSourceMode === "github" ? "GitHub URL 필요" : "로컬 폴더 필요",
    tone: workspaceControls.currentWorkspace ? "neutral" : "candidate",
  };
}

function workspaceNeedLabel(workspaceControls: WorkspaceControls): string {
  return workspaceControls.repoSourceMode === "github" ? "GitHub URL 필요" : "로컬 폴더 필요";
}

function workspaceReadyLabel(workspaceControls: WorkspaceControls): string {
  return workspaceControls.repoSourceMode === "github" ? "저장소 복제 준비" : "프로젝트 열기 준비";
}

function meaningfulFocus(map: VisualMap | null): string | null {
  const focus = map?.focus;
  return focus && focus !== "overview" && focus !== "narrow-focus" ? focus : null;
}

function nextCheckContext(
  workspaceControls: WorkspaceControls,
  visualMapControls: VisualMapControls,
  map: VisualMap | null,
): { value: string; detail: string; tone: "confirmed" | "candidate" | "neutral" } {
  if (visualMapControls.selectedEdge) {
    const candidate = visualMapControls.selectedEdge.kind.startsWith("candidate");
    return {
      value: candidate ? "후보 먼저 검토" : "근거 문장 확인",
      detail: candidate ? "직접 근거와 분리해서 판단" : visualMapControls.selectedEdge.evidence[0]?.text ?? "양끝 항목 확인",
      tone: candidate ? "candidate" : "confirmed",
    };
  }
  if (visualMapControls.selectedNode) {
    return {
      value: "관계 행 선택",
      detail: "직접/구조/후보를 분리해서 확인",
      tone: visualMapControls.selectedNode.source === "projection" ? "neutral" : "confirmed",
    };
  }
  const edgeTotal = map?.edges.length ?? 0;
  const focusId = meaningfulFocus(map);
  if (focusId) {
    return {
      value: edgeTotal > 0 ? "관계 행 선택" : "다른 대상 선택",
      detail: edgeTotal > 0 ? `${nodeLabel(focusId, map)} 기준 관계 ${edgeTotal}개` : `${nodeLabel(focusId, map)} 기준 관계 없음`,
      tone: edgeTotal > 0 ? "neutral" : "candidate",
    };
  }
  if (edgeTotal > 0) {
    return {
      value: "대상 선택",
      detail: `관계 ${edgeTotal}개 · 선택하면 근거 요약`,
      tone: "neutral",
    };
  }
  if ((map?.nodes.length ?? 0) > 0) {
    return {
      value: "카드 선택",
      detail: "주변 구조를 먼저 좁히기",
      tone: "neutral",
    };
  }
  if (!workspaceControls.currentWorkspace && workspaceControls.canCreateWorkspace) {
    return {
      value: workspaceControls.repoSourceMode === "github" ? "저장소 복제" : "프로젝트 열기",
      detail: workspaceControls.repoSourceMode === "github" ? "확인된 URL로 시작" : "확인된 폴더로 시작",
      tone: "candidate",
    };
  }
  if (!workspaceControls.currentWorkspace) {
    return {
      value: workspaceControls.repoSourceMode === "github" ? "URL 입력" : "폴더 선택",
      detail: workspaceControls.repoSourceMode === "github" ? "GitHub 저장소 복제" : "로컬 프로젝트 열기",
      tone: "candidate",
    };
  }
  return {
    value: "코드 또는 DB 읽기",
    detail: "왼쪽 연결 패널에서 시작",
    tone: "candidate",
  };
}

function readContext(
  workspaceControls: WorkspaceControls,
  visualMapControls: VisualMapControls,
  map: VisualMap | null,
): { label: string; value: string; detail: string } {
  if (!workspaceControls.currentWorkspace) {
    return workspaceControls.canCreateWorkspace
      ? {
          label: "준비 상태",
          value: workspaceReadyLabel(workspaceControls),
          detail: workspaceControls.repoSourceMode === "github" ? "저장소 복제" : "프로젝트 열기",
        }
      : {
          label: "입력 필요",
          value: workspaceControls.repoSourceMode === "github" ? "GitHub URL" : "로컬 폴더",
          detail: workspaceControls.repoSourceMode === "github" ? "URL 붙여넣기" : "폴더 선택",
        };
  }
  if (!map) {
    return {
      label: "읽기 상태",
      value: visualMapControls.snapshotSavedAt ? snapshotLabel(visualMapControls.snapshotSavedAt) : "코드/DB 연결 필요",
      detail: "코드 읽기와 DB 등록",
    };
  }
  return {
    label: "읽은 시점",
    value: snapshotLabel(visualMapControls.snapshotSavedAt),
    detail: modeLabel(map.mode),
  };
}

function relationCounts(edges: VisualEdge[]): RelationCounts {
  return edges.reduce<RelationCounts>(
    (counts, edge) => {
      if (edge.kind.startsWith("candidate")) {
        counts.candidate += 1;
      } else if (edge.kind === "code_flow") {
        counts.inferred += 1;
      } else if (edge.evidence.length === 0) {
        counts.typed += 1;
      } else {
        counts.confirmed += 1;
      }
      return counts;
    },
    { confirmed: 0, typed: 0, inferred: 0, candidate: 0 },
  );
}

function relationValue(counts: RelationCounts): string {
  const parts = [
    counts.confirmed > 0 ? `직접 ${counts.confirmed}` : null,
    counts.typed > 0 ? `구조 ${counts.typed}` : null,
    counts.candidate > 0 ? `후보 ${counts.candidate}` : null,
    counts.inferred > 0 ? `이름 단서 ${counts.inferred}` : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : "관계 없음";
}

function relationDetail(counts: RelationCounts): string {
  const total = counts.confirmed + counts.typed + counts.inferred + counts.candidate;
  return total > 0 ? `총 ${total} · 직접/구조 우선 · 후보/단서 검증` : "대상을 선택하면 근거 표시";
}

function snapshotLabel(value: string | null): string {
  if (!value) {
    return "확인 전";
  }
  const timestamp = Number(value);
  const date = Number.isFinite(timestamp) ? new Date(value.length <= 10 ? timestamp * 1000 : timestamp) : new Date(value);
  return Number.isNaN(date.getTime())
    ? "시간 확인 필요"
    : new Intl.DateTimeFormat("ko-KR", { dateStyle: "short", timeStyle: "short" }).format(date);
}

function edgeTitle(edge: VisualEdge, map: VisualMap | null): string {
  return `${nodeLabel(edge.from, map)} → ${nodeLabel(edge.to, map)}`;
}

function nodeLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  if (node) {
    return node.kind === "column" ? columnLabel(id, node.title) ?? node.title : node.title;
  }
  return columnLabel(id) ?? (id.startsWith("db:table:") ? id.slice("db:table:".length) : id);
}

function columnLabel(id: string, fallback?: string): string | null {
  if (!id.startsWith("db:column:")) {
    return null;
  }
  const body = id.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 ? `${body.slice(0, splitIndex)}.${body.slice(splitIndex + 1)}` : fallback ?? null;
}

function nodeKindLabel(node: VisualNode): string {
  if (node.kind === "api") return "API";
  if (node.kind === "table") return "테이블";
  if (node.kind === "column") return "컬럼";
  if (node.kind === "file") return "파일";
  return "코드";
}

function edgeKindLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) return "후보 근거";
  if (edge.kind === "code_flow") return "이름 단서";
  if (edge.kind === "db_constraint" || edge.kind === "db_fk") return "DB 제약";
  if (edge.kind === "code_call") return "코드 호출";
  if (edge.kind === "code_handle") return "라우트 처리";
  if (edge.kind === "contains") return "포함 관계";
  return edge.kind;
}

function modeLabel(mode: VisualMap["mode"]): string {
  if (mode === "api-flow") return "API가 닿는 코드";
  if (mode === "table-usage") return "테이블 연결";
  if (mode === "column-impact") return "컬럼 변경 범위";
  if (mode === "search-focus") return "대상 주변 근거";
  return "전체 구조";
}
