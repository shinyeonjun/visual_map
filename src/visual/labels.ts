import type { VisualEdge, VisualMap } from "../types/visual-map";

export type VisualEdgeTruthClass = "confirmed" | "structural" | "candidate" | "inferred";

export function visualMapModeLabel(mode: VisualMap["mode"]): string {
  if (mode === "api-flow") return "API가 닿는 코드";
  if (mode === "table-usage") return "테이블 연결";
  if (mode === "column-impact") return "컬럼 변경 범위";
  if (mode === "search-focus") return "대상 주변 근거";
  if (mode === "composition") return "관계 분석";
  return "전체 구조";
}

export function visualNodeKindLabel(kind: string, source?: string): string {
  if (kind === "group-domain") return "구조 영역";
  if (kind === "api") return "API";
  if (kind === "table") return "테이블";
  if (kind === "column") return "컬럼";
  if (kind === "constraint") return "제약";
  if (kind === "index") return "인덱스";
  if (kind === "view") return "뷰";
  if (kind === "trigger") return "트리거";
  if (kind === "routine") return "DB 함수/프로시저";
  if (kind === "file") return "파일";
  return source === "db" ? "DB 객체" : "코드";
}

export function visualEdgeKindLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) return "후보 근거";
  if (edge.kind.startsWith("structural_")) return "구조 관계";
  if (edge.kind === "contains" || edge.kind === "group_contains") return "포함 관계";
  if (edge.kind === "db_constraint" || edge.kind === "db_fk" || edge.kind.endsWith("_db_fk")) return "DB 제약";
  if (edge.kind === "db_index") return "DB 인덱스";
  if (edge.kind === "db_dependency") return "DB 의존성";
  if (edge.kind === "db_trigger") return "DB 트리거";
  if (edge.kind.startsWith("db_")) return "DB 관계";
  if (edge.kind === "code_db_read") return "DB 조회";
  if (edge.kind === "code_db_write") return "DB 변경";
  if (edge.kind === "code_db_uses_column") return "컬럼 사용";
  if (edge.kind === "code_call" || edge.kind.endsWith("_code_call")) return "코드 호출";
  if (edge.kind === "code_handle" || edge.kind.endsWith("_code_handle")) return "라우트 처리";
  if (edge.kind === "code_flow") return "이름 단서";
  return "관계";
}

export function visualEdgeTruthClass(edge: VisualEdge): VisualEdgeTruthClass {
  if (edge.kind.startsWith("candidate")) return "candidate";
  if (edge.kind === "code_flow") return "inferred";
  if (edge.kind === "contains" || edge.kind === "group_contains" || edge.kind.startsWith("structural_")) {
    return "structural";
  }
  return edge.evidence.length > 0 ? "confirmed" : "structural";
}
