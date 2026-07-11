import { Network, Search, SlidersHorizontal, Star, Table2, Workflow } from "lucide-react";
import type { ComponentType } from "react";

type Icon = ComponentType<{ size?: number }>;

export const workbenchModes: [Icon, string, string, string][] = [
  [Network, "atlas", "전체 구조", "API·코드·DB 요약"],
  [Workflow, "api-flow", "API가 닿는 코드", "라우트 → 코드"],
  [Table2, "table-usage", "테이블 연결", "코드 후보 · PK/FK"],
  [SlidersHorizontal, "column-impact", "컬럼 변경 범위", "직접/후보 근거 분리"],
  [Search, "search-focus", "대상 주변 근거", "선택 항목 중심"],
];

export { Star };
