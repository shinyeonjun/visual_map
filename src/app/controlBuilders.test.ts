import { describe, expect, it, vi } from "vitest";
import type { VisualEdge, VisualNode } from "../types/visual-map";
import { buildVisualMapControls } from "./controlBuilders";

type BuildVisualArgs = Parameters<typeof buildVisualMapControls>[0];

describe("buildVisualMapControls", () => {
  it("keeps canvas inspection separate from the navigation focus", () => {
    const visual = visualState();
    const code = codeState();
    const db = dbState();
    const controls = buildVisualMapControls({ visual, code, db });
    const node: VisualNode = {
      id: "code:function-1",
      kind: "function",
      title: "function-1",
      layer: "code",
      source: "code",
    };
    const edge: VisualEdge = {
      id: "edge-1",
      from: "code:function-1",
      to: "db:table:main.users",
      kind: "candidate_code_table",
      evidence: [],
    };

    controls.selectNode(node);
    expect(visual.setSelectedVisualNode).toHaveBeenCalledWith(node);
    expect(visual.setSelectedVisualNode).toHaveBeenCalledTimes(1);
    expect(visual.setSelectedVisualEdge).not.toHaveBeenCalled();

    vi.clearAllMocks();
    controls.selectEdge(edge);
    expect(visual.setSelectedVisualEdge).toHaveBeenCalledWith(edge);
    expect(visual.setSelectedVisualEdge).toHaveBeenCalledTimes(1);
    expect(visual.setSelectedVisualNode).not.toHaveBeenCalled();
    expect(code.setSelectedCodeItem).not.toHaveBeenCalled();
    expect(db.setSelectedDbTableKey).not.toHaveBeenCalled();
  });

  it("updates the navigation focus when the user chooses a new context item", () => {
    const visual = visualState();
    const code = codeState();
    const db = dbState();
    const controls = buildVisualMapControls({ visual, code, db });

    controls.showMode("table-usage", "db:table:main.users");

    expect(db.setSelectedDbTableKey).toHaveBeenCalledWith("main.users");
    expect(code.setSelectedCodeItem).toHaveBeenCalledWith(null);
    expect(visual.showMapMode).toHaveBeenCalledWith("table-usage", "db:table:main.users");
  });
});

function visualState(): BuildVisualArgs["visual"] {
  return {
    visualMap: null,
    mapMode: "atlas",
    mapFocusId: null,
    visualMapLoading: false,
    visualMapEnriching: false,
    changeIntent: { kind: "unknown", value: null },
    snapshotSavedAt: null,
    snapshotStaleReasons: [],
    snapshotSourceSummary: null,
    analysisCoverage: null,
    projectionElapsedMs: null,
    searchQuery: "",
    searchPopoverOpen: false,
    searchSummary: null,
    searchGroups: [],
    selectedVisualNode: null,
    selectedVisualEdge: null,
    setSearchQuery: vi.fn(),
    showMapMode: vi.fn(),
    setChangeIntent: vi.fn(),
    runSearch: vi.fn(),
    selectSearchResult: vi.fn(),
    openSearchPopover: vi.fn(),
    closeSearchPopover: vi.fn(),
    setSelectedVisualNode: vi.fn(),
    setSelectedVisualEdge: vi.fn(),
    clearVisualSelection: vi.fn(),
  } as unknown as BuildVisualArgs["visual"];
}

function codeState(): BuildVisualArgs["code"] {
  return {
    codeInventory: null,
    setSelectedCodeItem: vi.fn(),
  } as unknown as BuildVisualArgs["code"];
}

function dbState(): BuildVisualArgs["db"] {
  return {
    dbInventory: null,
    setSelectedDbTableKey: vi.fn(),
  } as unknown as BuildVisualArgs["db"];
}
