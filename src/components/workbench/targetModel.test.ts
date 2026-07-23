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

  it("puts backend roles ahead of modules and files", () => {
    const inventory = codeInventory();
    inventory.services = [codeItem("service", "OrderService")];
    inventory.handlers = [codeItem("handler", "OrderHandler")];
    inventory.repositories = [codeItem("repository", "OrderRepository")];
    inventory.classes = [codeItem("class", "OrderModel")];
    inventory.modules = [codeItem("module", "orders.module")];
    inventory.files = [codeItem("file", "orders.ts")];

    expect(buildTargetCatalog(inventory, null).code.map((item) => [item.badge, item.group]))
      .toEqual([
        ["HNDL", "핸들러"],
        ["SVC", "서비스"],
        ["REPO", "리포지토리"],
        ["FUNC", "함수"],
        ["CLASS", "클래스"],
        ["MOD", "모듈"],
        ["FILE", "파일"],
      ]);
  });

  it("keeps engine-only builtins out of user-selectable code targets", () => {
    const inventory = codeInventory();
    inventory.functions.push({
      ...codeItem("function", "len"),
      filePath: "<python-builtins>",
    });

    expect(buildTargetCatalog(inventory, null).code.map((item) => item.title)).toEqual(["loadOrders"]);
    expect(inventory.functions.map((item) => item.name)).toContain("len");
  });

  it("keeps the source root visible for duplicate routes in different trees", () => {
    const inventory = codeInventory();
    inventory.routes = [
      { ...inventory.routes[0], id: "legacy-route", filePath: "legacy/backend/app/api/routes/events.py", line: 198 },
      { ...inventory.routes[0], id: "current-route", filePath: "server/app/api/routes/events/query.py", line: 116 },
    ];

    expect(buildTargetCatalog(inventory, null).api.map((item) => item.meta)).toEqual([
      "legacy/…/routes/events.py:198",
      "server/…/events/query.py:116",
    ]);
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

function codeItem(kind: string, name: string) {
  return { id: `${kind}-${name}`, kind, name, filePath: `src/${name}`, line: 1, detail: null };
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
