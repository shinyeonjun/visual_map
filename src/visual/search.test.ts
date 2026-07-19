import { describe, expect, it } from "vitest";
import type { CodeInventory, CodeInventoryItem } from "../types/workspace";
import { collectSearchResults, searchSummaryText } from "./search";

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
