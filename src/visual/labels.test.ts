import { describe, expect, it } from "vitest";
import { visualEdgeKindLabel, visualMapModeLabel } from "./labels";

describe("visual labels", () => {
  it("names composition and semantic DB relationships explicitly", () => {
    expect(visualMapModeLabel("composition")).toBe("관계 분석");
    expect(visualEdgeKindLabel(edge("code_db_read"))).toBe("DB 조회");
    expect(visualEdgeKindLabel(edge("code_db_write"))).toBe("DB 변경");
    expect(visualEdgeKindLabel(edge("code_db_uses_column"))).toBe("컬럼 사용");
  });
});

function edge(kind: string) {
  return {
    id: kind,
    from: "code:loadOrder",
    to: "db:table:main.orders",
    kind,
    confidence: null,
    evidence: [],
  };
}
