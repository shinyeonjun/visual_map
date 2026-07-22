export type CanvasGuide = {
  question: string;
  action: string;
  basis: string;
};

export type AtlasInventoryCounts = {
  routes: number;
  code: number;
  files: number;
  tables: number;
  columns: number;
};

export function atlasCanvasFacts({
  mode,
  mapNodes,
  mapEdges,
  mapWarnings,
  routes,
  code,
  files,
  tables,
  columns,
  searchSummary,
}: {
  mode: string;
  mapNodes: number;
  mapEdges: number;
  mapWarnings: number;
  routes: number;
  code: number;
  files: number;
  tables: number;
  columns: number;
  searchSummary: string | null;
}): string {
  if (mode === "search-focus" && searchSummary) {
    return searchSummary;
  }
  if (mapNodes > 0 || mapEdges > 0) {
    return `항목 ${mapNodes}개 · 관계 ${mapEdges}개${mapWarnings > 0 ? ` · 경고 ${mapWarnings}개` : ""}`;
  }
  return inventoryFactsText({ routes, code, files, tables, columns });
}

export function atlasReadOrder(mode: string, counts: AtlasInventoryCounts): string {
  if (mode === "api-flow") {
    return counts.routes > 0 && counts.code > 0 ? "확인 순서: API → 코드" : inventoryReadOrder(counts);
  }
  if (mode === "table-usage") {
    return counts.code > 0 && counts.tables > 0 ? "확인 순서: 테이블 → 코드 후보" : inventoryReadOrder(counts);
  }
  if (mode === "column-impact") {
    return counts.code > 0 && counts.tables > 0 ? "확인 순서: 컬럼 → 영향 후보" : inventoryReadOrder(counts);
  }
  if (mode === "search-focus") {
    return "확인 순서: 검색 대상 → 주변 근거";
  }
  return inventoryReadOrder(counts);
}

export function atlasModePurpose(mode: string, counts: AtlasInventoryCounts): string {
  if (mode === "api-flow") {
    if (counts.routes > 0) {
      return "API가 어떤 코드까지 이어지는지 봅니다";
    }
    return counts.files > 0 && counts.code === 0 ? "API 라우트 없음 · 파일 구조부터 봅니다" : "API 라우트 없음 · 실제 코드만 봅니다";
  }
  if (mode === "table-usage") {
    if (counts.tables > 0 && counts.columns === 0) {
      return "컬럼 대기 · 테이블 목록만 봅니다";
    }
    return counts.tables > 0 && (counts.routes > 0 || counts.code > 0)
      ? "테이블 연결과 제약을 분리해서 봅니다"
      : counts.tables > 0
        ? "테이블 구조와 DB 제약만 봅니다"
        : "DB 연결 전 코드 구조를 봅니다";
  }
  if (mode === "column-impact") {
    if (counts.tables === 0) {
      return "DB를 연결하면 컬럼 답이 열립니다";
    }
    if (counts.columns === 0) {
      return "컬럼을 읽으면 변경 범위가 열립니다";
    }
    return counts.routes > 0 || counts.code > 0
      ? "컬럼 변경의 직접/후보 근거를 봅니다"
      : "컬럼 제약과 DB 내부 구조를 봅니다";
  }
  if (mode === "search-focus") {
    return "검색한 대상만 좁혀 봅니다";
  }
  if (counts.routes === 0 && counts.code === 0 && counts.files > 0 && counts.tables > 0) {
    return "코드 심볼 없음 · 파일과 DB 구조부터 봅니다";
  }
  if (counts.routes === 0 && counts.code === 0 && counts.files > 0) {
    return "코드 심볼 없음 · 파일 구조부터 봅니다";
  }
  if (counts.routes === 0 && counts.code > 0 && counts.tables > 0) {
    return "API 라우트 없음 · 코드 구조부터 봅니다";
  }
  if (counts.routes === 0 && counts.code > 0) {
    return "API 라우트 없음 · 실제 코드만 봅니다";
  }
  if (counts.routes === 0 && counts.tables > 0 && counts.columns === 0) {
    return "컬럼 연결 전 테이블 목록만 봅니다";
  }
  if (counts.routes === 0 && counts.tables > 0) {
    return "코드 연결 전 DB 구조를 봅니다";
  }
  return "API·코드·DB 전체 구조를 봅니다";
}

export function atlasCanvasGuide({
  mode,
  counts,
  readOrder,
  relationTotal,
  selectedEdge,
  selectedNode,
  selectedTableNeedsColumns,
}: {
  mode: string;
  counts: AtlasInventoryCounts;
  readOrder: string;
  relationTotal: number;
  selectedEdge: boolean;
  selectedNode: boolean;
  selectedTableNeedsColumns: boolean;
}): CanvasGuide {
  if (selectedEdge) {
    return {
      question: "이 관계는 근거가 있나",
      action: "근거 보기",
      basis: "양끝 항목 확인",
    };
  }
  if (selectedNode) {
    if (selectedTableNeedsColumns) {
      return {
        question: "테이블 구조가 충분한가",
        action: "DB 컬럼 보강",
        basis: "테이블 목록만 있음",
      };
    }
    if (relationTotal === 0) {
      return {
        question: "이 대상에 연결이 있나",
        action: "요약 확인",
        basis: "관계 없음",
      };
    }
    return {
      question: "선택 대상 영향",
      action: "관계 행 선택",
      basis: relationTotal > 0 ? `관계 ${relationTotal}개` : "관계 없음",
    };
  }
  if (mode === "api-flow") {
    return {
      question: "이 API가 어디까지 닿나",
      action: "API 카드 선택",
      basis: readOrder,
    };
  }
  if (mode === "table-usage") {
    if (counts.tables > 0 && counts.columns === 0) {
      return {
        question: "테이블 구조가 충분한가",
        action: "컬럼 구조 보강",
        basis: readOrder,
      };
    }
    return {
      question: counts.code > 0 ? "이 테이블과 연결된 코드는?" : "테이블 키 구조",
      action: "테이블 카드 선택",
      basis: readOrder,
    };
  }
  if (mode === "column-impact") {
    if (counts.tables > 0 && counts.columns === 0) {
      return {
        question: "영향 근거가 있는가",
        action: "DB 컬럼 보강",
        basis: readOrder,
      };
    }
    return {
      question: counts.code > 0 ? "이 컬럼 변경 범위는?" : "컬럼 제약",
      action: "컬럼 선택",
      basis: readOrder,
    };
  }
  if (mode === "search-focus") {
    return {
      question: "이 대상 주변에 뭐가 있나",
      action: "검색 결과 선택",
      basis: readOrder,
    };
  }
  return {
    question: "먼저 볼 덩어리는?",
    action: relationTotal > 0 ? "카드 선택" : "프로젝트/DB 연결",
    basis: readOrder,
  };
}

function inventoryReadOrder(counts: AtlasInventoryCounts): string {
  const parts = [
    counts.routes > 0 ? "API" : null,
    counts.code > 0 ? "코드" : null,
    counts.files > 0 && counts.code === 0 ? "파일" : null,
    counts.tables > 0 ? "DB" : null,
  ].filter(Boolean);
  return `확인 순서: ${parts.length > 0 ? parts.join(" → ") : "코드/DB 연결"}`;
}

function inventoryFactsText(counts: AtlasInventoryCounts): string {
  const parts = [
    counts.routes > 0 ? `API ${counts.routes}개` : null,
    counts.code > 0 ? `코드 ${counts.code}개` : null,
    counts.files > 0 ? `파일 ${counts.files}개` : null,
    counts.tables > 0 ? `테이블 ${counts.tables}개` : null,
    counts.tables > 0 ? (counts.columns > 0 ? `컬럼 ${counts.columns}개` : "컬럼 대기") : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : "코드/DB 연결";
}

export function atlasModeTitle(mode: string, counts: AtlasInventoryCounts): string {
  if (mode === "api-flow") {
    return "API가 닿는 코드";
  }
  if (mode === "table-usage") {
    if (counts.tables > 0 && counts.columns === 0) {
      return "테이블 목록";
    }
    return counts.routes > 0 || counts.code > 0 ? "테이블 연결" : "테이블 구조";
  }
  if (mode === "column-impact") {
    if (counts.tables > 0 && counts.columns === 0) {
      return "컬럼 대기";
    }
    return counts.routes > 0 || counts.code > 0 ? "컬럼 변경 범위" : "컬럼 제약";
  }
  if (mode === "search-focus") {
    return "대상 주변 근거";
  }
  return "전체 구조";
}
