import { afterEach, describe, expect, it, vi } from "vitest";
import type { CodeInventory } from "../types/workspace";
import { SEARCH_WORKER_THRESHOLD, searchInventorySize } from "./search";
import { collectSearchResultsAsync } from "./searchWorkerClient";

describe("search worker client", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("falls back to the trusted local search when a large inventory has no Worker runtime", async () => {
    vi.stubGlobal("Worker", undefined);
    const inventory = largeCodeInventory();

    expect(searchInventorySize(inventory, null)).toBeGreaterThanOrEqual(SEARCH_WORKER_THRESHOLD);
    await expect(collectSearchResultsAsync("target", inventory, null)).resolves.toMatchObject({
      total: 1,
      results: [{ title: "target" }],
    });
  });
});

function largeCodeInventory(): CodeInventory {
  const functions = Array.from({ length: SEARCH_WORKER_THRESHOLD }, (_, index) => ({
    id: `function-${index}`,
    kind: "function",
    name: index === 37 ? "target" : `function-${index}`,
    filePath: `src/function-${index}.ts`,
    detail: null,
  }));
  return {
    project: "search-worker-test",
    routes: [],
    services: [],
    files: [],
    handlers: [],
    repositories: [],
    functions,
    classes: [],
    modules: [],
    unknown: [],
    summary: {
      routes: 0,
      handlers: 0,
      services: 0,
      repositories: 0,
      functions: functions.length,
      classes: 0,
      modules: 0,
      files: 0,
      unknown: 0,
    },
    calls: [],
  };
}
