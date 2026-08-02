import { describe, expect, it } from "vitest";
import type { SearchResult } from "../types/controls";
import type { InventorySnapshot, VisualMap } from "../types/visual-map";
import {
  coverageFromSnapshot,
  mapAnswersMode,
  mapRequestKey,
  rememberVisualMap,
  searchModeForResult,
  selectedSearchSummary,
  sourceSummary,
} from "./visualMapModel";

function result(id: string): SearchResult {
  return { id, title: "Users", subtitle: "src/users.ts:10", focusId: id };
}

function map(id: string): VisualMap {
  return { id, workspaceId: "workspace", mode: "atlas", focus: id, nodes: [], edges: [], warnings: [] };
}

describe("visual map model", () => {
  it("builds stable request keys and keeps the cache bounded", () => {
    expect(mapRequestKey("workspace", "api-flow", "code:route", null)).toContain("api-flow");

    const cache = new Map<string, VisualMap>();
    for (let index = 0; index < 40; index += 1) {
      rememberVisualMap(cache, `map-${index}`, map(`map-${index}`));
    }

    expect(cache.size).toBe(32);
    expect(cache.has("map-0")).toBe(false);
    expect(cache.has("map-39")).toBe(true);
  });

  it("keeps search mode labels and snapshot coverage deterministic", () => {
    expect(searchModeForResult(result("api:users"))).toBe("api-flow");
    expect(selectedSearchSummary(result("table:public.users"))).toContain("테이블 연결");

    const snapshot: InventorySnapshot = {
      workspaceId: "workspace",
      savedAt: "1",
      items: [],
      metadata: {
        code: {
          savedAt: "1",
          sourceType: "local",
          sourceRevisionLabel: "abc123",
          resultCount: 3,
        },
        gaps: [{ id: "gap", kind: "provider", message: "확인 필요" }],
      },
    };
    expect(sourceSummary(snapshot)).toBe("abc123 · 로컬 상태 확인");
    expect(coverageFromSnapshot(snapshot)).toMatchObject({
      code: { available: true, observed: 3 },
      db: { available: false },
      gaps: 1,
    });
  });

  it("requires the expected result shape for focused map modes", () => {
    expect(mapAnswersMode(null, "api-flow")).toBe(false);
    expect(mapAnswersMode({ ...map("api"), apiReading: null }, "api-flow")).toBe(false);
    expect(
      mapAnswersMode(
        { ...map("api"), nodes: [{ id: "n", kind: "api", title: "n", layer: "api", source: "code" }] },
        "atlas",
      ),
    ).toBe(true);
  });
});
