import type { VisualEdge, VisualMap } from "../types/visual-map";

export function visualMapModeLabel(mode: VisualMap["mode"]): string {
  if (mode === "api-flow") return "API가 닿는 코드";
  if (mode === "table-usage") return "테이블 연결";
  if (mode === "column-impact") return "컬럼 변경 범위";
  if (mode === "search-focus") return "대상 주변 근거";
  return "전체 구조";
}

export function visualNodeKindLabel(kind: string): string {
  if (kind === "group-domain") return "구조 영역";
  if (kind === "api") return "API";
  if (kind === "table") return "테이블";
  if (kind === "column") return "컬럼";
  if (kind === "file") return "파일";
  return "코드";
}

export function visualEdgeKindLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) return "후보 근거";
  if (edge.kind.startsWith("structural_")) return "구조 관계";
  if (edge.kind === "contains" || edge.kind === "group_contains") return "포함 관계";
  if (edge.kind === "db_constraint" || edge.kind === "db_fk") return "DB 제약";
  if (edge.kind === "code_call") return "코드 호출";
  if (edge.kind === "code_handle") return "라우트 처리";
  if (edge.kind === "code_flow") return "이름 단서";
  return "관계";
}
