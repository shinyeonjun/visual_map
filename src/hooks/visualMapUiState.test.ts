import { describe, expect, it } from "vitest";
import type { VisualEdge, VisualNode } from "../types/visual-map";
import { createVisualMapUiState, visualMapUiReducer } from "./visualMapUiState";

describe("visualMapUiReducer", () => {
  it("updates map navigation state without sharing mutable arrays", () => {
    const state = createVisualMapUiState();
    const focusIds = ["code:a", "db:b"];
    const next = visualMapUiReducer(state, { type: "set-composition-focus-ids", value: focusIds });

    expect(next.compositionFocusIds).toEqual(focusIds);
    expect(next.compositionFocusIds).not.toBe(focusIds);
    expect(state.mapMode).toBe("atlas");
  });

  it("keeps search and selection transitions isolated", () => {
    const node: VisualNode = { id: "code:a", kind: "function", title: "a", layer: "code", source: "code" };
    const edge: VisualEdge = { id: "edge:a", from: node.id, to: "db:table:users", kind: "USES_TABLE", evidence: [] };
    let state = createVisualMapUiState();

    state = visualMapUiReducer(state, { type: "set-search-query", value: "users" });
    state = visualMapUiReducer(state, { type: "set-search-popover-open", value: true });
    state = visualMapUiReducer(state, { type: "set-selected-node", value: node });
    state = visualMapUiReducer(state, { type: "set-selected-edge", value: edge });

    expect(state.searchQuery).toBe("users");
    expect(state.searchPopoverOpen).toBe(true);
    expect(state.selectedVisualNode).toBe(node);
    expect(state.selectedVisualEdge).toBe(edge);
  });

  it("copies change intent so later caller mutation cannot change UI state", () => {
    const state = createVisualMapUiState();
    const intent = { kind: "rename" as const, value: "email" };
    const next = visualMapUiReducer(state, { type: "set-change-intent", value: intent });

    intent.value = "changed";
    expect(next.changeIntent).toEqual({ kind: "rename", value: "email" });
  });
});
