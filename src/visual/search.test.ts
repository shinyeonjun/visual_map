import { describe, expect, it } from "vitest";
import { codeInventoryFromSnapshot, dbInventoryFromSnapshot } from "../inventory/snapshotRestore";
import type { InventorySearchResult, InventorySummary } from "../types/visual-map";
import type { CodeInventory, CodeInventoryItem, DbInventory } from "../types/workspace";
import { collectSearchResults, searchCollectionFromInventoryResult, searchSummaryText } from "./search";

describe("developer search", () => {
  it("disambiguates duplicate route paths with method and source location", () => {
    const inventory = codeInventory([
      route("sessions-get__route__GET__", "server/routes/sessions.ts", 18),
      route("sessions-post__route__POST__", "server/routes/admin.ts", 44),
    ]);

    const collection = collectSearchResults("sessions", inventory, null);

    expect(collection.total).toBe(2);
    expect(collection.results.map((result) => result.title)).toEqual([
      "GET /api/v1/sessions",
      "POST /api/v1/sessions",
    ]);
    expect(collection.results.map((result) => result.subtitle)).toEqual([
      "server/routes/sessions.ts:L18",
      "server/routes/admin.ts:L44",
    ]);
    expect(searchSummaryText(collection)).toBe("찾은 대상 2개 · API 2");
  });

  it("keeps unverified route-like strings searchable without presenting them as APIs", () => {
    const item: CodeInventoryItem = {
      id: "docs.route-string",
      kind: "unknown",
      name: "/api/v1/sessions",
      engineLabel: "Route",
      detail: {},
    };
    const inventory = codeInventory([]);
    inventory.unknown = [item];
    inventory.summary.unknown = 1;

    const collection = collectSearchResults("sessions", inventory, null);

    expect(collection.total).toBe(1);
    expect(collection.results[0]).toMatchObject({
      id: "code:docs.route-string",
      title: "/api/v1/sessions",
      subtitle: "근거 미확인",
    });
    expect(searchSummaryText(collection)).toBe("찾은 대상 1개 · 코드 1");
  });

  it("keeps virtual engine symbols out of developer search", () => {
    const inventory = codeInventory([]);
    inventory.functions = [
      {
        id: "project-length",
        kind: "function",
        name: "length",
        filePath: "src/strings.py",
        detail: null,
      },
      {
        id: "builtin-len",
        kind: "function",
        name: "len",
        filePath: "<python-builtins>",
        detail: null,
      },
    ];
    inventory.summary.functions = inventory.functions.length;

    const collection = collectSearchResults("len", inventory, null);

    expect(collection.total).toBe(1);
    expect(collection.results[0]).toMatchObject({
      id: "code:project-length",
      title: "length",
    });
  });

  it("maps a full-snapshot search hit that was omitted from the bounded bootstrap", () => {
    const result: InventorySearchResult = {
      hits: [
        {
          group: "api",
          item: {
            id: "code:sessions__route__GET__",
            kind: "api",
            name: "/api/v1/sessions",
            layer: "api",
            source: "code",
            qualifiedName: "sessions__route__GET__",
            location: { path: "server/routes/sessions.ts", line: 18 },
          },
        },
      ],
      total: 1,
      counts: { api: 1 },
      truncated: false,
    };

    expect(searchCollectionFromInventoryResult(result).results[0]).toMatchObject({
      title: "GET /api/v1/sessions",
      subtitle: "server/routes/sessions.ts:L18",
      focusId: "code:sessions__route__GET__",
    });
  });

  it("maps database object hits to their surrounding evidence", () => {
    const itemId = "db:view:sqlite%3Ashop%3Amain%3Apublic%3Aview%3Aactive_orders";
    const result: InventorySearchResult = {
      hits: [
        {
          group: "db-object",
          item: {
            id: itemId,
            kind: "view",
            name: "active_orders",
            layer: "data",
            source: "db",
            qualifiedName: "sqlite:shop:main:public:view:active_orders",
          },
        },
      ],
      total: 1,
      counts: { "db-object": 1 },
      truncated: false,
    };

    const collection = searchCollectionFromInventoryResult(result);

    expect(collection.results[0]).toMatchObject({
      id: `db-object:${itemId}`,
      title: "active_orders",
      subtitle: "뷰 · public.active_orders",
      focusId: itemId,
    });
    expect(searchSummaryText(collection)).toBe("찾은 대상 1개 · DB 객체 1");
  });

  it("restores exact totals while keeping a bounded set of code items", () => {
    const summary: InventorySummary = {
      workspaceId: "workspace-1",
      savedAt: "1",
      totalItems: 151,
      totalLinks: 0,
      sources: {
        code: { total: 151, groups: { routes: 1, functions: 150 } },
      },
    };
    const inventory = codeInventoryFromSnapshot(
      {
        workspaceId: "workspace-1",
        savedAt: "1",
        items: [
          {
            id: "code:function:visible",
            kind: "function",
            name: "visible",
            layer: "code",
            source: "code",
          },
        ],
      },
      "test",
      summary,
    );

    expect(inventory.summary).toMatchObject({ routes: 1, functions: 150 });
    expect(inventory.functions).toHaveLength(1);
    expect(inventory.partial).toBe(true);
  });

  it("marks a bounded DB bootstrap partial while preserving the exact table total", () => {
    const summary: InventorySummary = {
      workspaceId: "workspace-1",
      savedAt: "1",
      totalItems: 300,
      totalLinks: 0,
      sources: {
        db: { total: 300, groups: { table: 150, column: 150 } },
      },
    };
    const inventory = dbInventoryFromSnapshot(
      {
        workspaceId: "workspace-1",
        savedAt: "1",
        metadata: { db: { savedAt: "1", totalTables: 150, sourceType: "sqlite" } },
        items: [
          {
            id: "db:table:public.users",
            kind: "table",
            name: "users",
            layer: "database",
            source: "db",
            path: "public",
          },
        ],
      },
      "profile-1",
      summary,
    );

    expect(inventory.tables).toHaveLength(1);
    expect(inventory.totalTables).toBe(150);
    expect(inventory.partial).toBe(true);
  });

  it("marks DB inventory partial when dependent objects were bounded", () => {
    const summary: InventorySummary = {
      workspaceId: "workspace-1",
      savedAt: "1",
      totalItems: 2,
      totalLinks: 1,
      sources: {
        db: { total: 2, groups: { table: 1, view: 1 } },
      },
    };
    const inventory = dbInventoryFromSnapshot(
      {
        workspaceId: "workspace-1",
        savedAt: "1",
        items: [
          {
            id: "db:table:public.orders",
            kind: "table",
            name: "orders",
            layer: "database",
            source: "db",
            path: "public",
          },
        ],
      },
      "profile-1",
      summary,
    );

    expect(inventory.tables).toHaveLength(1);
    expect(inventory.partial).toBe(true);
  });

  it("keeps delimiter-heavy database names selectable and human-readable", () => {
    const inventory: DbInventory = {
      profileId: "profile-1",
      tables: [
        {
          schema: "audit.2026",
          name: "order:events",
          columns: [
            {
              name: "value:raw%text",
              isPrimaryKey: false,
              isForeignKey: false,
            },
          ],
        },
      ],
    };

    const collection = collectSearchResults("raw", null, inventory);

    expect(collection.results[0]).toMatchObject({
      title: "order:events.value:raw%text",
      subtitle: "audit.2026.order:events",
      focusId: "db:column:audit%2E2026.order%3Aevents:value%3Araw%25text",
      tableKey: "audit%2E2026.order%3Aevents",
    });
  });

  it("finds each dependent database object once across related tables", () => {
    const dependent = {
      key: "sqlite:shop:main:public:view:active_orders",
      kind: "view",
      name: "active_orders",
      relation: "view_depends_on",
      columnKeys: [],
    };
    const inventory: DbInventory = {
      profileId: "profile-1",
      tables: [
        { schema: "public", name: "orders", columns: [], dependents: [dependent] },
        { schema: "public", name: "customers", columns: [], dependents: [dependent] },
      ],
    };

    const collection = collectSearchResults("active", null, inventory);

    expect(collection.total).toBe(1);
    expect(collection.results[0]).toMatchObject({
      id: "db-object:sqlite:shop:main:public:view:active_orders",
      title: "active_orders",
      subtitle: "뷰 · public.active_orders",
      focusId: "db:view:sqlite%3Ashop%3Amain%3Apublic%3Aview%3Aactive_orders",
    });
    expect(searchSummaryText(collection)).toBe("찾은 대상 1개 · DB 객체 1");
  });
});

function route(id: string, filePath: string, line: number): CodeInventoryItem {
  return {
    id,
    kind: "route",
    name: "/api/v1/sessions",
    filePath,
    line,
    qualifiedName: id,
    detail: {},
  };
}

function codeInventory(routes: CodeInventoryItem[]): CodeInventory {
  return {
    project: "test",
    routes,
    services: [],
    files: [],
    handlers: [],
    repositories: [],
    functions: [],
    classes: [],
    modules: [],
    unknown: [],
    calls: [],
    handles: [],
    summary: {
      routes: routes.length,
      handlers: 0,
      services: 0,
      repositories: 0,
      functions: 0,
      classes: 0,
      modules: 0,
      files: 0,
      unknown: 0,
    },
  };
}
