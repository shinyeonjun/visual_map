import { describe, expect, it } from "vitest";
import type { CodeInventory, DbInventory } from "../../types/workspace";
import { buildTargetCatalog, firstAvailableTargetKind, targetKindForMode } from "./targetModel";

describe("targetModel", () => {
  it("maps each target type to its automatic answer mode", () => {
    const catalog = buildTargetCatalog(codeInventory(), dbInventory());

    expect(catalog.api[0]).toMatchObject({
      title: "/api/orders",
      focusId: "code:route-orders",
      mode: "api-flow",
    });
    expect(catalog.code[0]).toMatchObject({
      title: "loadOrders",
      focusId: "code:function-load-orders",
      mode: "search-focus",
    });
    expect(catalog.table[0]).toMatchObject({
      title: "public.orders",
      focusId: "db:table:public.orders",
      mode: "table-usage",
    });
    expect(catalog.column[0]).toMatchObject({
      title: "id",
      group: "public.orders",
      focusId: "db:column:public.orders:id",
      mode: "column-impact",
    });
  });

  it("derives the browser category from the answer mode", () => {
    expect(targetKindForMode("api-flow")).toBe("api");
    expect(targetKindForMode("search-focus")).toBe("code");
    expect(targetKindForMode("table-usage")).toBe("table");
    expect(targetKindForMode("column-impact")).toBe("column");
    expect(targetKindForMode("atlas")).toBeNull();
  });

  it("uses the first category that actually has data", () => {
    const catalog = buildTargetCatalog(null, dbInventory());
    expect(firstAvailableTargetKind(catalog)).toBe("table");
  });
});

function codeInventory(): CodeInventory {
  return {
    project: "orders",
    routes: [{ id: "route-orders", kind: "api", name: "/api/orders", filePath: "src/routes.ts", line: 12, detail: null }],
    services: [],
    handlers: [],
    repositories: [],
    functions: [{ id: "function-load-orders", kind: "function", name: "loadOrders", filePath: "src/orders.ts", line: 23, detail: null }],
    classes: [],
    modules: [],
    unknown: [],
    files: [],
    calls: [],
    summary: { routes: 1, handlers: 0, services: 0, repositories: 0, functions: 1, classes: 0, modules: 0, files: 0, unknown: 0 },
  };
}

function dbInventory(): DbInventory {
  return {
    profileId: "main-db",
    tables: [{
      schema: "public",
      name: "orders",
      columns: [{ name: "id", dataType: "uuid", isPrimaryKey: true, isForeignKey: false }],
    }],
  };
}
