import type { SearchResultGroup } from "../types/controls";
import type { ChangeIntent, VisualEdge, VisualNode } from "../types/visual-map";
import type { RelationView } from "../visual/visualMapModel";

export const DEFAULT_CHANGE_INTENT: ChangeIntent = { kind: "rename", value: null };

export type VisualMapUiState = {
  mapMode: string;
  compositionFocusIds: string[];
  relationView: RelationView;
  changeIntent: ChangeIntent;
  searchQuery: string;
  searchPopoverOpen: boolean;
  searchSummary: string | null;
  searchGroups: SearchResultGroup[];
  selectedVisualNode: VisualNode | null;
  selectedVisualEdge: VisualEdge | null;
};

export type VisualMapUiAction =
  | { type: "set-map-mode"; value: string }
  | { type: "set-composition-focus-ids"; value: string[] }
  | { type: "set-relation-view"; value: RelationView }
  | { type: "set-change-intent"; value: ChangeIntent }
  | { type: "set-search-query"; value: string }
  | { type: "set-search-popover-open"; value: boolean }
  | { type: "set-search-summary"; value: string | null }
  | { type: "set-search-groups"; value: SearchResultGroup[] }
  | { type: "set-selected-node"; value: VisualNode | null }
  | { type: "set-selected-edge"; value: VisualEdge | null };

export function createVisualMapUiState(): VisualMapUiState {
  return {
    mapMode: "atlas",
    compositionFocusIds: [],
    relationView: "connections",
    changeIntent: { ...DEFAULT_CHANGE_INTENT },
    searchQuery: "",
    searchPopoverOpen: false,
    searchSummary: null,
    searchGroups: [],
    selectedVisualNode: null,
    selectedVisualEdge: null,
  };
}

export function visualMapUiReducer(state: VisualMapUiState, action: VisualMapUiAction): VisualMapUiState {
  switch (action.type) {
    case "set-map-mode":
      return state.mapMode === action.value ? state : { ...state, mapMode: action.value };
    case "set-composition-focus-ids":
      return { ...state, compositionFocusIds: [...action.value] };
    case "set-relation-view":
      return state.relationView === action.value ? state : { ...state, relationView: action.value };
    case "set-change-intent":
      return { ...state, changeIntent: { ...action.value } };
    case "set-search-query":
      return state.searchQuery === action.value ? state : { ...state, searchQuery: action.value };
    case "set-search-popover-open":
      return state.searchPopoverOpen === action.value ? state : { ...state, searchPopoverOpen: action.value };
    case "set-search-summary":
      return state.searchSummary === action.value ? state : { ...state, searchSummary: action.value };
    case "set-search-groups":
      return { ...state, searchGroups: [...action.value] };
    case "set-selected-node":
      return { ...state, selectedVisualNode: action.value };
    case "set-selected-edge":
      return { ...state, selectedVisualEdge: action.value };
  }
}
