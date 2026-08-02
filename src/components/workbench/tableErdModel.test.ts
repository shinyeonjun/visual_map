import { describe, expect, it } from "vitest";
import type { VisualMap } from "../../types/visual-map";
import type { DbInventoryTable } from "../../types/workspace";
import { buildTableErdModel } from "./tableErdModel";
import { buildColumnImpactCascade } from "./columnImpactModel";
import type { ImpactReviewBoard, ImpactReviewItem } from "../../types/visual-map";

function ordersTable(): DbInventoryTable {
  return {
    schema: "public",
    name: "orders",
    columns: [
      { name: "total", dataType: "numeric", isPrimaryKey: false, isForeignKey: false },
      { name: "id", dataType: "bigint", isPrimaryKey: true, isForeignKey: false },
      { name: "user_id", dataType: "bigint", isPrimaryKey: false, isForeignKey: true },
    ],
    foreignKeys: [{
      name: "fk_orders_user",
      columns: ["user_id"],
      referencedSchema: "public",
      referencedTable: "users",
      referencedColumns: ["id"],
    }],
    inboundForeignKeys: [{
      name: "fk_items_order",
      tableSchema: "public",
      table: "order_items",
      columns: ["order_id"],
      referencedTable: "orders",
      referencedColumns: ["id"],
    }],
  };
}

function usageMap(focus: string): VisualMap {
  return {
    id: "map",
    workspaceId: "w",
    mode: "table-usage",
    focus,
    nodes: [],
    edges: [
      { id: "r", from: "code:fn", to: focus, kind: "code_db_read", evidence: [] },
      { id: "w", from: "code:fn2", to: focus, kind: "code_db_write", evidence: [] },
      { id: "c", from: "code:fn3", to: focus, kind: "candidate_table", evidence: [] },
    ],
    warnings: [],
  };
}

describe("table ERD model", () => {
  it("builds FK neighbors in both directions with PK-first columns", () => {
    const focus = "db:table:public.orders";
    const model = buildTableErdModel(usageMap(focus), [ordersTable()])!;

    expect(model.tableLabel).toBe("public.orders");
    expect(model.columns[0].name).toBe("id");
    expect(model.outbound).toHaveLength(1);
    expect(model.outbound[0].label).toBe("public.users");
    expect(model.outbound[0].viaLabel).toBe("user_id → id");
    expect(model.inbound).toHaveLength(1);
    expect(model.inbound[0].label).toBe("public.order_items");
    expect(model.reads).toBe(1);
    expect(model.writes).toBe(1);
    expect(model.candidateUses).toBe(1);
  });

  it("returns null when the focus table is not in the inventory", () => {
    expect(buildTableErdModel(usageMap("db:table:public.missing"), [ordersTable()])).toBeNull();
  });
});

describe("column impact cascade", () => {
  it("groups direct items into structure, code, and api tiers", () => {
    const board: ImpactReviewBoard = {
      subject: "orders.total",
      scope: "column",
      lanes: [{
        id: "direct",
        order: 0,
        title: "직접 영향",
        description: "",
        tone: "confirmed",
        total: 3,
        hidden: 0,
        emptyMessage: "",
        items: [
          item("idx", { nodeId: "db:table:public.orders", kind: "index" }),
          item("fn", { kind: "function", location: { path: "src/orders.ts", line: 3 } }),
          item("api", { kind: "route" }),
        ],
      }],
      markdownSummary: "",
    };

    const model = buildColumnImpactCascade(board)!;
    expect(model.tiers.map(({ id }) => id)).toEqual(["structure", "code", "api"]);
    expect(model.total).toBe(3);
  });

  it("returns null when there are no direct items", () => {
    const board: ImpactReviewBoard = {
      subject: "orders.total",
      scope: "column",
      lanes: [{ id: "direct", order: 0, title: "", description: "", tone: "confirmed", total: 0, hidden: 0, emptyMessage: "", items: [] }],
      markdownSummary: "",
    };
    expect(buildColumnImpactCascade(board)).toBeNull();
  });
});

function item(id: string, overrides: Partial<ImpactReviewItem>): ImpactReviewItem {
  return {
    id,
    kind: "function",
    title: id,
    detail: id,
    truthClass: "confirmed",
    rank: 1,
    evidence: [],
    ...overrides,
  };
}
