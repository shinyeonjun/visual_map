import { describe, expect, it } from "vitest";
import type { VisualEdge } from "../types/visual-map";
import { visualEdgeKindLabel, visualEdgeTruthClass, visualMapModeLabel } from "./labels";

describe("visual labels", () => {
  it("names composition and semantic DB relationships explicitly", () => {
    expect(visualMapModeLabel("composition")).toBe("관계 분석");
    expect(visualEdgeKindLabel(edge("code_db_read"))).toBe("DB 조회");
    expect(visualEdgeKindLabel(edge("code_db_write"))).toBe("DB 변경");
    expect(visualEdgeKindLabel(edge("code_db_uses_column"))).toBe("컬럼 사용");
  });

  it("classifies edge trust without promoting structural evidence", () => {
    const evidence = [{ kind: "engine", text: "읽은 구조" }];

    expect(visualEdgeTruthClass(edge("code_call", evidence))).toBe("confirmed");
    expect(visualEdgeTruthClass(edge("code_call"))).toBe("structural");
    expect(visualEdgeTruthClass(edge("contains", evidence))).toBe("structural");
    expect(visualEdgeTruthClass(edge("candidate_code_db", evidence))).toBe("candidate");
    expect(visualEdgeTruthClass(edge("code_flow", evidence))).toBe("inferred");
  });
});

function edge(kind: string, evidence: VisualEdge["evidence"] = []): VisualEdge {
  return {
    id: kind,
    from: "code:loadOrder",
    to: "db:table:main.orders",
    kind,
    confidence: null,
    evidence,
  };
}
