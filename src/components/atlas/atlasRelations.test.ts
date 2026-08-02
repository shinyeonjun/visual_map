import { describe, expect, it } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  buildRelationCounts,
  filterCodeItemsByMap,
  orderItemsByRelations,
  relationFocusIdFromMapFocus,
  relationLedgerRows,
  takeWithPinned,
} from "./atlasRelations";

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

function mapWithEdges(edges: VisualMap["edges"]): VisualMap {
  return {
    id: "map",
    workspaceId: "workspace",
    mode: "atlas",
    focus: "",
    nodes: [],
    edges,
    warnings: [],
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

  it("shows the relationship meaning separately from its trust tone", () => {
    const map: VisualMap = {
      id: "semantic-map",
      workspaceId: "workspace",
      mode: "composition",
      focus: "code:handler",
      nodes,
      edges: [edge("read", "code_db_read", "code:handler", "db:table:public.orders", "explicit SQL")],
      warnings: [],
    };

    expect(relationLedgerRows(map, null, null, null)[0]).toMatchObject({
      label: "DB 조회",
      tone: "confirmed",
      to: "public.orders",
    });
  });

  it("keeps a pinned item visible without exceeding the display cap", () => {
    const items = ["a", "b", "c", "d"];
    expect(takeWithPinned(items, new Set(["d"]), (item) => item, 2)).toEqual(["d", "a"]);
  });

  it("does not present inventory items that are absent from the current map", () => {
    const items = [{ id: "route" }, { id: "missing" }];
    expect(filterCodeItemsByMap(items, new Set(["code:route"]))).toEqual([{ id: "route" }]);
    expect(filterCodeItemsByMap(items, new Set(["code:other"]))).toEqual([]);
  });

  it("keeps database object focus available to the relation ledger", () => {
    expect(relationFocusIdFromMapFocus("db:view:active-orders")).toBe("db:view:active-orders");
    expect(relationFocusIdFromMapFocus("group:package:app")).toBeNull();
  });

  it("puts a code call chain in source-to-target order", () => {
    const items = [{ id: "query" }, { id: "handler" }, { id: "service" }];
    const ordered = orderItemsByRelations(
      items,
      mapWithEdges([
        edge("h-s", "code_call", "code:handler", "code:service"),
        edge("s-q", "code_call", "code:service", "code:query"),
      ]),
      (item) => `code:${item.id}`,
    );

    expect(ordered.map((item) => item.id)).toEqual(["handler", "service", "query"]);
  });

  it("keeps disconnected items after the connected flow", () => {
    const items = [{ id: "orphan" }, { id: "handler" }, { id: "service" }];
    const ordered = orderItemsByRelations(
      items,
      mapWithEdges([edge("h-s", "code_call", "code:handler", "code:service")]),
      (item) => `code:${item.id}`,
    );

    expect(ordered.map((item) => item.id)).toEqual(["handler", "service", "orphan"]);
  });

  it("does not let candidate edges reorder the confirmed flow", () => {
    const items = [{ id: "candidate-target" }, { id: "handler" }, { id: "service" }];
    const ordered = orderItemsByRelations(
      items,
      mapWithEdges([
        edge("h-s", "code_call", "code:handler", "code:service"),
        edge("h-candidate", "candidate_table", "code:handler", "code:candidate-target"),
      ]),
      (item) => `code:${item.id}`,
    );

    expect(ordered.map((item) => item.id)).toEqual(["handler", "service", "candidate-target"]);
  });
});
