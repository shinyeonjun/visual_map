import { describe, expect, it } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { buildRelationCounts, relationLedgerRows, takeWithPinned } from "./atlasRelations";

const nodes: VisualNode[] = [
  { id: "code:route", kind: "api", title: "GET /orders", layer: "api", source: "code" },
  { id: "code:handler", kind: "function", title: "getOrders", layer: "code", source: "code" },
  { id: "db:table:public.orders", kind: "table", title: "orders", layer: "data", source: "db" },
];

function edge(id: string, kind: string, from: string, to: string, evidence = ""): VisualEdge {
  return {
    id,
    kind,
    from,
    to,
    confidence: kind.startsWith("candidate") ? "high" : null,
    evidence: evidence ? [{ kind: "test", text: evidence }] : [],
  };
}

describe("atlas relation policy", () => {
  it("separates confirmed, structural, candidate, and inferred counts", () => {
    const map: VisualMap = {
      id: "map",
      workspaceId: "workspace",
      mode: "atlas",
      focus: "overview",
      nodes,
      edges: [
        edge("confirmed", "code_handle", "code:route", "code:handler", "HANDLES"),
        edge("typed", "group_contains", "group:package:app", "code:handler"),
        edge("candidate", "candidate_table", "code:handler", "db:table:public.orders", "table name match"),
        edge("inferred", "code_flow", "code:route", "code:handler"),
      ],
      warnings: [],
    };

    expect(buildRelationCounts(map).get("code:handler")).toEqual({
      confirmed: 1,
      typed: 1,
      candidate: 1,
      inferred: 1,
    });
    expect(relationLedgerRows(map, null, null, null).map((row) => row.tone)).toEqual([
      "confirmed",
      "typed",
      "candidate",
      "inferred",
    ]);
  });

  it("keeps a pinned item visible without exceeding the display cap", () => {
    const items = ["a", "b", "c", "d"];
    expect(takeWithPinned(items, new Set(["d"]), (item) => item, 2)).toEqual(["d", "a"]);
  });
});
