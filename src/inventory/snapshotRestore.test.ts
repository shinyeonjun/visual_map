import { describe, expect, it } from "vitest";
import type { InventorySnapshot } from "../types/visual-map";
import { codeInventoryFromSnapshot, dbInventoryFromSnapshot } from "./snapshotRestore";

describe("code inventory snapshot restore", () => {
  it("restores only confirmed calls for route ranking", () => {
    const snapshot: InventorySnapshot = {
      workspaceId: "workspace-1",
      savedAt: "1",
      items: [],
      links: [
        { id: "high", from: "code:a", to: "code:b", kind: "code_call", truthClass: "confirmed", evidence: [] },
        { id: "weak", from: "code:a", to: "code:c", kind: "code_call", truthClass: "candidate", evidence: [] },
      ],
    };

    expect(codeInventoryFromSnapshot(snapshot, "shop").calls).toEqual([{ from: "a", to: "b" }]);
  });
});

describe("database inventory snapshot restore", () => {
  it("rebuilds view, trigger, and routine dependents from stable confirmed links", () => {
    const tableId = "db:table:public.orders";
    const columnId = "db:column:public.orders:status";
    const viewId = "db:view:active-orders";
    const triggerId = "db:trigger:orders-status";
    const routineId = "db:routine:refresh-orders";
    const snapshot: InventorySnapshot = {
      workspaceId: "workspace-1",
      savedAt: "1",
      items: [
        {
          id: tableId,
          kind: "table",
          name: "orders",
          layer: "data",
          source: "db",
          path: "public",
          qualifiedName: "sqlite:shop:main:public:table:orders",
        },
        {
          id: columnId,
          kind: "column",
          name: "status",
          layer: "data",
          source: "db",
          parentId: tableId,
          qualifiedName: "sqlite:shop:main:public:column:orders:status",
        },
        {
          id: viewId,
          kind: "view",
          name: "active_orders",
          layer: "data",
          source: "db",
          qualifiedName: "sqlite:shop:main:public:view:active_orders",
        },
        {
          id: triggerId,
          kind: "trigger",
          name: "trg_orders_status",
          layer: "data",
          source: "db",
          parentId: tableId,
          qualifiedName: "sqlite:shop:main:public:trigger:orders:trg_orders_status",
        },
        {
          id: routineId,
          kind: "routine",
          name: "refresh_orders",
          layer: "data",
          source: "db",
          qualifiedName: "sqlite:shop:main:public:routine:refresh_orders",
        },
      ],
      links: [
        {
          id: "view-status",
          from: viewId,
          to: columnId,
          kind: "db_dependency",
          truthClass: "confirmed",
          evidence: [
            { kind: "db-relation", text: "view_depends_on" },
            { kind: "db-column-key", text: "sqlite:shop:main:public:column:orders:status" },
          ],
        },
        {
          id: "table-trigger",
          from: tableId,
          to: triggerId,
          kind: "db_trigger",
          truthClass: "confirmed",
          evidence: [{ kind: "db-relation", text: "table_has_trigger" }],
        },
        {
          id: "routine-table",
          from: routineId,
          to: tableId,
          kind: "db_dependency",
          truthClass: "confirmed",
          evidence: [{ kind: "db-relation", text: "routine_depends_on" }],
        },
      ],
    };

    const inventory = dbInventoryFromSnapshot(snapshot, "profile-1");

    expect(inventory.tables[0].dependents).toEqual([
      {
        key: "sqlite:shop:main:public:routine:refresh_orders",
        kind: "routine",
        name: "refresh_orders",
        relation: "routine_depends_on",
        columnKeys: [],
      },
      {
        key: "sqlite:shop:main:public:trigger:orders:trg_orders_status",
        kind: "trigger",
        name: "trg_orders_status",
        relation: "table_has_trigger",
        columnKeys: [],
      },
      {
        key: "sqlite:shop:main:public:view:active_orders",
        kind: "view",
        name: "active_orders",
        relation: "view_depends_on",
        columnKeys: ["sqlite:shop:main:public:column:orders:status"],
      },
    ]);
  });

  it("does not invent a dependent when the stable DB object key is invalid", () => {
    const snapshot: InventorySnapshot = {
      workspaceId: "workspace-1",
      savedAt: "1",
      items: [
        { id: "db:table:orders", kind: "table", name: "orders", layer: "data", source: "db" },
        {
          id: "db:view:legacy",
          kind: "view",
          name: "legacy_view",
          layer: "data",
          source: "db",
          qualifiedName: "legacy_view",
        },
      ],
      links: [
        {
          id: "legacy-link",
          from: "db:view:legacy",
          to: "db:table:orders",
          kind: "db_dependency",
          truthClass: "confirmed",
          evidence: [],
        },
      ],
    };

    expect(dbInventoryFromSnapshot(snapshot, "profile-1").tables[0].dependents).toEqual([]);
  });
});
