import { invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useReducer, useRef } from "react";
import { commandErrorCode, toUserError } from "../app/operationStatus";
import { validateInventoryBootstrap, validateInventorySearchResult, validateVisualMap } from "../app/runtimeContracts";
import { hasTauriRuntime } from "../app/tauriRuntime";
import {
  groupSearchResults,
  searchCollectionFromInventoryResult,
  searchScopeText,
  searchSummaryText,
  type SearchCollection,
} from "../visual/search";
import { collectSearchResultsAsync } from "../visual/searchWorkerClient";
import { resetMapContext, saveMapContext, savedMapContext } from "../visual/mapContext";
import {
  compositionSearchResultIsSupported,
  coverageFromSnapshot,
  mapAnswersMode,
  mapRequestKey,
  rememberVisualMap,
  searchModeForResult,
  selectedSearchSummary,
  sourceSummary,
  visualMapCacheKey,
  type RelationView,
} from "../visual/visualMapModel";
import type { CodeInventory, CodeInventoryItem, DbInventory } from "../types/workspace";
import type { SearchResult, SearchResultGroup } from "../types/controls";
import type { ChangeIntent, InventorySnapshot, VisualEdge, VisualMap, VisualNode } from "../types/visual-map";
import { createVisualMapUiState, DEFAULT_CHANGE_INTENT, visualMapUiReducer } from "./visualMapUiState";
import { useVisualMapState } from "./useVisualMapState";

type SearchContext = {
  codeInventory: CodeInventory | null;
  dbInventory: DbInventory | null;
  selectCodeItem: (item: CodeInventoryItem) => void;
  selectDbTable: (tableKey: string) => void;
};

export function useVisualMap({
  currentWorkspaceId,
  bootstrapReady = true,
  onOperation,
}: {
  currentWorkspaceId: string | null;
  bootstrapReady?: boolean;
  onOperation?: (action: string) => void;
}) {
  const {
    visualMap,
    setVisualMap,
    visualMapLoading,
    setVisualMapLoading,
    visualMapEnriching,
    setVisualMapEnriching,
    visualMapStatus,
    setVisualMapStatus,
    visualMapError,
    setVisualMapError,
    visualMapErrorDetail,
    setVisualMapErrorDetail,
    snapshotSavedAt,
    setSnapshotSavedAt,
    snapshotStaleReasons,
    setSnapshotStaleReasons,
    snapshotSourceSummary,
    setSnapshotSourceSummary,
    analysisCoverage,
    setAnalysisCoverage,
    snapshotWorkspaceId,
    setSnapshotWorkspaceId,
    projectionElapsedMs,
    setProjectionElapsedMs,
    visualStateWorkspaceId,
    setVisualStateWorkspaceId,
    visualTargetKey,
    setVisualTargetKey,
    visualMapKey,
    setVisualMapKey,
  } = useVisualMapState(currentWorkspaceId);
  const [uiState, dispatchUi] = useReducer(visualMapUiReducer, undefined, createVisualMapUiState);
  const {
    mapMode,
    compositionFocusIds,
    relationView,
    changeIntent,
    searchQuery,
    searchPopoverOpen,
    searchSummary,
    searchGroups,
    selectedVisualNode,
    selectedVisualEdge,
  } = uiState;
  const setMapMode = (value: string) => dispatchUi({ type: "set-map-mode", value });
  const setCompositionFocusIds = (value: string[]) => dispatchUi({ type: "set-composition-focus-ids", value });
  const setRelationViewState = (value: RelationView) => dispatchUi({ type: "set-relation-view", value });
  const setChangeIntentState = (value: ChangeIntent) => dispatchUi({ type: "set-change-intent", value });
  const setSearchQueryValue = (value: string) => dispatchUi({ type: "set-search-query", value });
  const setSearchPopoverOpen = (value: boolean) => dispatchUi({ type: "set-search-popover-open", value });
  const setSearchSummary = (value: string | null) => dispatchUi({ type: "set-search-summary", value });
  const setSearchGroups = (value: SearchResultGroup[]) => dispatchUi({ type: "set-search-groups", value });
  const setSelectedVisualNode = (value: VisualNode | null) => dispatchUi({ type: "set-selected-node", value });
  const setSelectedVisualEdge = (value: VisualEdge | null) => dispatchUi({ type: "set-selected-edge", value });
  const selectedVisualNodeRef = useRef<VisualNode | null>(null);
  const selectedVisualEdgeRef = useRef<VisualEdge | null>(null);
  const currentWorkspaceIdRef = useRef<string | null>(currentWorkspaceId);
  const changeIntentRef = useRef<ChangeIntent>(DEFAULT_CHANGE_INTENT);
  const compositionFocusIdsRef = useRef<string[]>([]);
  const relationViewRef = useRef<RelationView>("connections");
  const visualTargetRef = useRef<{
    workspaceId: string;
    mode: string;
    focusId: string | null;
    focusIds?: string[];
    relationView?: RelationView;
  } | null>(null);
  const visualMapRequestRef = useRef(0);
  const evidenceGenerationRef = useRef(0);
  const snapshotRevisionRef = useRef<string | null>(null);
  const visualMapCacheRef = useRef(new Map<string, VisualMap>());
  const visualMapRequestsRef = useRef(new Map<string, Promise<VisualMap>>());
  const activeVisualMapOperationRef = useRef<string | null>(null);
  const searchContextRef = useRef<SearchContext | null>(null);
  const searchRequestRef = useRef(0);

  function cancelActiveVisualMapOperation() {
    const operationId = activeVisualMapOperationRef.current;
    activeVisualMapOperationRef.current = null;
    if (!operationId || !hasTauriRuntime()) {
      return;
    }
    void invoke("cancel_visual_map", { operationId }).catch(() => {
      // The request may have completed between the new selection and cancellation.
    });
  }

  useLayoutEffect(() => {
    cancelActiveVisualMapOperation();
    currentWorkspaceIdRef.current = currentWorkspaceId;
    invalidateEnrichedMaps();
    snapshotRevisionRef.current = null;
    searchRequestRef.current += 1;
    searchContextRef.current = null;
    setSearchQueryValue("");
    setSearchPopoverOpen(false);
    setSearchSummary(null);
    setSearchGroups([]);
    compositionFocusIdsRef.current = [];
    setCompositionFocusIds([]);
    relationViewRef.current = "connections";
    setRelationViewState("connections");
    if (!currentWorkspaceId || !bootstrapReady) {
      if (!currentWorkspaceId) {
        clearVisualMapState();
      }
      return;
    }
    const context = savedMapContext(currentWorkspaceId);
    clearVisualSelection();
    setMapMode(context.mode);
    void loadVisualMap(context.focusId, context.mode, currentWorkspaceId);
    // This effect is scoped to workspace/bootstrap transitions; local helper identities change per render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bootstrapReady, currentWorkspaceId]);

  async function loadVisualMap(
    focusId?: string | null,
    mode = "atlas",
    workspaceId = currentWorkspaceIdRef.current,
    focusIds = mode === "composition" ? compositionFocusIdsRef.current : [],
    requestedRelationView = relationViewRef.current,
  ): Promise<VisualMap | null> {
    if (!workspaceId) {
      clearVisualMapState();
      return null;
    }
    onOperation?.("map-load");
    const requestId = ++visualMapRequestRef.current;
    const startedAt = performance.now();
    const requestChangeIntent = mode === "column-impact" ? { ...changeIntentRef.current } : null;
    const targetKey = mapRequestKey(workspaceId, mode, focusId, requestChangeIntent, focusIds, requestedRelationView);
    visualTargetRef.current = {
      workspaceId,
      mode,
      focusId: focusId ?? null,
      focusIds: mode === "composition" ? [...focusIds] : undefined,
      relationView: mode === "composition" ? requestedRelationView : undefined,
    };
    setVisualStateWorkspaceId(workspaceId);
    setVisualTargetKey(targetKey);
    setVisualMapEnriching(false);
    const isCurrentRequest = () =>
      visualMapRequestRef.current === requestId && currentWorkspaceIdRef.current === workspaceId;
    // Selecting a card is a snapshot projection, not a new code analysis.
    // Live evidence is intentionally opt-in so every API click stays local and
    // cannot start another provider/LSP process.
    const shouldEnrichCodeEvidence = false;
    const cacheKey = visualMapCacheKey(
      shouldEnrichCodeEvidence ? "enriched" : "base",
      targetKey,
      evidenceGenerationRef.current,
    );

    const cachedMap = visualMapCacheRef.current.get(cacheKey);
    if (cachedMap) {
      cancelActiveVisualMapOperation();
      visualMapCacheRef.current.delete(cacheKey);
      visualMapCacheRef.current.set(cacheKey, cachedMap);
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      setVisualMapLoading(false);
      setVisualMap(cachedMap);
      setVisualMapKey(targetKey);
      setProjectionElapsedMs(0);
      syncVisualSelection(cachedMap);
      setVisualMapStatus(
        cachedMap.nodes.length > 0 ? `캔버스 항목 ${cachedMap.nodes.length}개 표시` : "캔버스 항목 없음",
      );
      return cachedMap;
    }

    const sharesInFlightRequest = visualMapRequestsRef.current.has(cacheKey);
    if (!sharesInFlightRequest) {
      cancelActiveVisualMapOperation();
    }
    const operationId =
      sharesInFlightRequest && activeVisualMapOperationRef.current
        ? activeVisualMapOperationRef.current
        : "visual-map-" + Date.now() + "-" + requestId;
    if (!sharesInFlightRequest) {
      activeVisualMapOperationRef.current = operationId;
    }

    try {
      setVisualMapLoading(true);
      setVisualMapStatus("캔버스 준비 중");
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      if (shouldEnrichCodeEvidence) {
        setVisualMapStatus("정적 SQL과 코드/DB 근거 확인 중");
        const enriched = await enrichVisualMap({
          workspaceId,
          focusId: focusId ?? null,
          mode,
          changeIntent: requestChangeIntent,
          requestId,
          targetKey,
          focusIds,
          relationView: requestedRelationView,
          operationId,
        });
        if (isCurrentRequest()) {
          setProjectionElapsedMs(Math.round(performance.now() - startedAt));
        }
        return enriched;
      }
      let request = visualMapRequestsRef.current.get(cacheKey);
      if (!request) {
        request = invoke<unknown>("get_visual_map", {
          workspaceId,
          focusId: focusId ?? null,
          mode,
          changeIntent: requestChangeIntent,
          enrichCodeEvidence: false,
          composition: mode === "composition" ? { focusIds, relationView: requestedRelationView } : null,
          operationId,
        }).then(validateVisualMap);
        visualMapRequestsRef.current.set(cacheKey, request);
      }
      const map = await request.finally(() => {
        if (visualMapRequestsRef.current.get(cacheKey) === request) {
          visualMapRequestsRef.current.delete(cacheKey);
        }
      });
      rememberVisualMap(visualMapCacheRef.current, cacheKey, map);
      if (!isCurrentRequest()) {
        return null;
      }
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      setVisualMap(map);
      setVisualMapKey(targetKey);
      setProjectionElapsedMs(Math.round(performance.now() - startedAt));
      syncVisualSelection(map);
      setVisualMapStatus(map.nodes.length > 0 ? `캔버스 항목 ${map.nodes.length}개 표시` : "캔버스 항목 없음");
      return map;
    } catch (error) {
      if (!isCurrentRequest()) {
        return null;
      }
      if (["snapshot_missing", "snapshot_stale"].includes(commandErrorCode(error) ?? "")) {
        setVisualMap(null);
        setVisualMapKey(null);
        clearVisualSelection();
        setVisualMapError(null);
        setVisualMapErrorDetail(null);
        setVisualMapStatus("코드/DB 읽기 결과 필요");
        setProjectionElapsedMs(Math.round(performance.now() - startedAt));
        return null;
      }

      const uiError = toUserError(error, "캔버스를 만들지 못했습니다");
      setVisualMap(null);
      setVisualMapKey(null);
      setVisualMapStatus(null);
      clearVisualSelection();
      setVisualMapError(uiError.message);
      setVisualMapErrorDetail(uiError.details);
      setProjectionElapsedMs(Math.round(performance.now() - startedAt));
      return null;
    } finally {
      if (isCurrentRequest()) {
        setVisualMapLoading(false);
        if (activeVisualMapOperationRef.current === operationId) {
          activeVisualMapOperationRef.current = null;
        }
      }
    }
  }

  async function enrichVisualMap({
    workspaceId,
    focusId,
    mode,
    changeIntent,
    requestId,
    targetKey,
    focusIds,
    relationView,
    operationId,
  }: {
    workspaceId: string;
    focusId: string | null;
    mode: string;
    changeIntent: ChangeIntent | null;
    requestId: number;
    targetKey: string;
    focusIds: string[];
    relationView: RelationView;
    operationId: string;
  }): Promise<VisualMap | null> {
    const generation = evidenceGenerationRef.current;
    const cacheKey = visualMapCacheKey("enriched", targetKey, generation);
    const isCurrentRequest = () =>
      evidenceGenerationRef.current === generation &&
      visualMapRequestRef.current === requestId &&
      currentWorkspaceIdRef.current === workspaceId;
    if (isCurrentRequest()) {
      setVisualMapEnriching(true);
    }

    let request: Promise<VisualMap> | undefined;
    try {
      let map = visualMapCacheRef.current.get(cacheKey);
      if (!map) {
        request = visualMapRequestsRef.current.get(cacheKey);
        if (!request) {
          request = invoke<unknown>("get_visual_map", {
            workspaceId,
            focusId,
            mode,
            changeIntent,
            enrichCodeEvidence: true,
            composition: mode === "composition" ? { focusIds, relationView } : null,
            operationId,
          }).then(validateVisualMap);
          visualMapRequestsRef.current.set(cacheKey, request);
        }
        map = await request;
        if (evidenceGenerationRef.current === generation) {
          rememberVisualMap(visualMapCacheRef.current, cacheKey, map);
        }
      }
      if (!isCurrentRequest()) {
        return null;
      }
      setVisualMap(map);
      setVisualMapKey(targetKey);
      syncVisualSelection(map);
      setVisualMapStatus(
        map.nodes.length > 0 ? `캔버스 항목 ${map.nodes.length}개 · 코드 후보 확인 완료` : "캔버스 항목 없음",
      );
      return map;
    } finally {
      if (request && visualMapRequestsRef.current.get(cacheKey) === request) {
        visualMapRequestsRef.current.delete(cacheKey);
      }
      if (isCurrentRequest()) {
        setVisualMapEnriching(false);
        if (activeVisualMapOperationRef.current === operationId) {
          activeVisualMapOperationRef.current = null;
        }
      }
    }
  }

  async function refreshInventorySnapshot(workspaceId: string): Promise<boolean> {
    try {
      const bootstrap = validateInventoryBootstrap(await invoke<unknown>("load_inventory_bootstrap", { workspaceId }));
      if (!bootstrap) {
        return false;
      }
      if (currentWorkspaceIdRef.current !== workspaceId) {
        return false;
      }
      const { snapshot } = bootstrap;
      invalidateEnrichedMaps();
      noteSnapshotLoaded(snapshot, false);
      const context = savedMapContext(workspaceId);
      setMapMode(context.mode);
      const map = await loadVisualMap(context.focusId, context.mode, workspaceId);
      if (!mapAnswersMode(map, context.mode)) {
        setMapMode("atlas");
        saveMapContext(workspaceId, "atlas", null);
        const fallbackMap = await loadVisualMap(null, "atlas", workspaceId);
        return fallbackMap !== null;
      }
      return map !== null;
    } catch (error) {
      if (currentWorkspaceIdRef.current !== workspaceId) {
        return false;
      }
      const uiError = toUserError(error, "코드/DB 읽기 결과를 불러오지 못했습니다");
      setVisualMapError(uiError.message);
      setVisualMapErrorDetail(uiError.details);
      return false;
    }
  }

  function showMapMode(mode: string, focusId?: string | null, preserveSearch = false) {
    setMapMode(mode);
    clearVisualSelection();
    if (!preserveSearch) {
      searchRequestRef.current += 1;
      setSearchQueryValue("");
      setSearchPopoverOpen(false);
      setSearchSummary(null);
      setSearchGroups([]);
    } else if (mode !== "search-focus") {
      setSearchPopoverOpen(false);
    }
    if (currentWorkspaceIdRef.current && mode !== "composition") {
      saveMapContext(currentWorkspaceIdRef.current, mode, focusId);
    }
    if (mode === "composition" && compositionFocusIdsRef.current.length < 2) {
      prepareCompositionSelection();
      return;
    }
    if (mode === "composition") {
      void loadVisualMap(
        focusId,
        mode,
        currentWorkspaceIdRef.current,
        compositionFocusIdsRef.current,
        relationViewRef.current,
      );
      return;
    }
    void loadVisualMap(focusId, mode, currentWorkspaceIdRef.current);
  }

  function toggleCompositionFocus(focusId: string) {
    const current = compositionFocusIdsRef.current;
    const next = current.includes(focusId)
      ? current.filter((id) => id !== focusId)
      : current.length < 8
        ? [...current, focusId]
        : current;
    if (next === current) {
      setVisualMapStatus("관계 분석 대상은 최대 8개까지 선택할 수 있습니다");
      return;
    }
    compositionFocusIdsRef.current = next;
    setCompositionFocusIds(next);
    setMapMode("composition");
    clearVisualSelection();
    if (next.length < 2) {
      prepareCompositionSelection();
      return;
    }
    void loadVisualMap(null, "composition", currentWorkspaceIdRef.current, next, relationViewRef.current);
  }

  function clearCompositionFocus() {
    compositionFocusIdsRef.current = [];
    setCompositionFocusIds([]);
    setMapMode("composition");
    clearVisualSelection();
    prepareCompositionSelection();
  }

  function updateRelationView(view: RelationView) {
    relationViewRef.current = view;
    setRelationViewState(view);
    if (compositionFocusIdsRef.current.length >= 2) {
      void loadVisualMap(null, "composition", currentWorkspaceIdRef.current, compositionFocusIdsRef.current, view);
    }
  }

  function prepareCompositionSelection() {
    visualMapRequestRef.current += 1;
    const workspaceId = currentWorkspaceIdRef.current;
    const targetKey = workspaceId
      ? mapRequestKey(workspaceId, "composition", null, null, compositionFocusIdsRef.current, relationViewRef.current)
      : null;
    visualTargetRef.current = workspaceId
      ? {
          workspaceId,
          mode: "composition",
          focusId: null,
          focusIds: [...compositionFocusIdsRef.current],
          relationView: relationViewRef.current,
        }
      : null;
    setVisualStateWorkspaceId(workspaceId);
    setVisualTargetKey(targetKey);
    setVisualMap(null);
    setVisualMapKey(null);
    setVisualMapLoading(false);
    setVisualMapEnriching(false);
    setVisualMapError(null);
    setVisualMapErrorDetail(null);
    setVisualMapStatus(
      compositionFocusIdsRef.current.length === 0 ? "관계를 볼 대상 2~8개를 선택하세요" : "대상을 1개 더 선택하세요",
    );
  }

  function updateChangeIntent(intent: ChangeIntent) {
    const next = { kind: intent.kind, value: intent.value?.trim() || null };
    changeIntentRef.current = next;
    setChangeIntentState(next);
    const target = visualTargetRef.current;
    if (target?.mode === "column-impact" && target.workspaceId === currentWorkspaceIdRef.current) {
      void loadVisualMap(target.focusId, target.mode, target.workspaceId);
    }
  }

  function updateSearchQuery(value: string, context?: SearchContext) {
    if (context) {
      searchContextRef.current = context;
    }
    setSearchQueryValue(value);
    const query = value.trim().toLowerCase();
    setSearchPopoverOpen(Boolean(query));
    if (!query) {
      searchRequestRef.current += 1;
      setSearchSummary(null);
      setSearchGroups([]);
      return;
    }
    const activeContext = context ?? searchContextRef.current;
    refreshSearchResults(query, activeContext);
  }

  function refreshSearchResults(
    query: string,
    context: SearchContext | null,
    onResolved?: (collection: SearchCollection) => void,
  ) {
    const requestId = ++searchRequestRef.current;
    if (!context) {
      setSearchSummary(null);
      setSearchGroups([]);
      return;
    }
    if (query.length < 2) {
      searchRequestRef.current += 1;
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      return;
    }
    void collectSearchResultsAsync(query, context.codeInventory, context.dbInventory).then((collection) => {
      if (searchRequestRef.current !== requestId) {
        return;
      }
      presentSearchCollection(collection);
      if (
        (!context.codeInventory?.partial && !context.dbInventory?.partial) ||
        !currentWorkspaceIdRef.current ||
        !hasTauriRuntime()
      ) {
        onResolved?.(collection);
        return;
      }

      const workspaceId = currentWorkspaceIdRef.current;
      void invoke<unknown>("search_inventory", { workspaceId, query })
        .then(validateInventorySearchResult)
        .then((result) => {
          if (searchRequestRef.current === requestId && currentWorkspaceIdRef.current === workspaceId) {
            const resolved = searchCollectionFromInventoryResult(result);
            presentSearchCollection(resolved);
            onResolved?.(resolved);
          }
        })
        .catch(() => {
          // The bounded local index remains usable when a background full search fails.
          if (searchRequestRef.current === requestId && currentWorkspaceIdRef.current === workspaceId) {
            onResolved?.(collection);
          }
        });
    });
  }

  function presentSearchCollection(collection: SearchCollection) {
    setSearchSummary(
      collection.truncated
        ? `${searchSummaryText(collection)} 그룹별 상위 결과만 보여줍니다.`
        : searchSummaryText(collection),
    );
    setSearchGroups(groupSearchResults(collection.results));
  }

  function runSearch(
    { codeInventory, dbInventory, selectCodeItem, selectDbTable }: SearchContext,
    submittedValue = searchQuery,
  ) {
    searchContextRef.current = { codeInventory, dbInventory, selectCodeItem, selectDbTable };
    setSearchQueryValue(submittedValue);
    setSearchPopoverOpen(true);
    const query = submittedValue.trim().toLowerCase();
    if (!query) {
      setSearchSummary(`검색어를 입력하면 ${searchScopeText(codeInventory, dbInventory)}을 함께 찾습니다.`);
      setSearchGroups([]);
      if (mapMode !== "composition") {
        showMapMode("search-focus", null, true);
      }
      focusSearchInput();
      return;
    }

    if ((codeInventory?.partial || dbInventory?.partial) && currentWorkspaceIdRef.current && hasTauriRuntime()) {
      refreshSearchResults(query, searchContextRef.current, (collection) => {
        const firstResult = groupSearchResults(collection.results)[0]?.results[0] ?? null;
        if (firstResult) {
          selectSearchResult(firstResult);
        }
      });
      if (mapMode !== "composition") {
        showMapMode("search-focus", null, true);
      }
      return;
    }

    if (query.length < 2) {
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      if (mapMode !== "composition") {
        showMapMode("search-focus", null, true);
      }
      return;
    }
    refreshSearchResults(query, searchContextRef.current, (collection) => {
      const grouped = groupSearchResults(collection.results);
      if (collection.truncated) {
        setSearchSummary(`${searchSummaryText(collection)} 그룹별 상위 결과만 보여줍니다.`);
        setSearchGroups(grouped);
        return;
      }
      const firstResult = grouped[0]?.results[0] ?? null;
      if (firstResult) {
        selectSearchResult(firstResult);
      }
    });
    if (mapMode !== "composition") {
      showMapMode("search-focus", null, true);
    }
  }

  function focusSearchInput() {
    window.requestAnimationFrame(() => {
      const input = document.querySelector<HTMLInputElement>("#global-inventory-search");
      input?.focus();
      input?.select();
    });
  }

  function selectSearchResult(result: SearchResult) {
    const input = document.querySelector<HTMLInputElement>("#global-inventory-search");
    if (input) input.value = "";
    if (mapMode === "composition" && compositionSearchResultIsSupported(result)) {
      setSearchQueryValue("");
      setSearchPopoverOpen(false);
      setSearchGroups([]);
      if (compositionFocusIdsRef.current.includes(result.focusId)) {
        setSearchSummary(`이미 선택한 관계 대상 · ${result.title}`);
        setVisualMapStatus("이미 관계 분석 대상으로 선택되어 있습니다");
        return;
      }
      setSearchSummary(`관계 대상에 추가 · ${result.title}`);
      toggleCompositionFocus(result.focusId);
      return;
    }
    const context = searchContextRef.current;
    if (result.codeItem && context) {
      context.selectCodeItem(result.codeItem);
    } else if (result.tableKey && context) {
      context.selectDbTable(result.tableKey);
    }
    setSearchSummary(selectedSearchSummary(result));
    setSearchQueryValue("");
    setSearchPopoverOpen(false);
    setSearchGroups([]);
    showMapMode(searchModeForResult(result), result.focusId);
  }

  function openSearchPopover() {
    const query = searchQuery.trim().toLowerCase();
    if (!query) {
      setSearchSummary(
        `검색어를 입력하면 ${searchScopeText(searchContextRef.current?.codeInventory ?? null, searchContextRef.current?.dbInventory ?? null)}을 함께 찾습니다.`,
      );
      setSearchGroups([]);
      setSearchPopoverOpen(true);
      return;
    }
    refreshSearchResults(query, searchContextRef.current);
    setSearchPopoverOpen(true);
  }

  function closeSearchPopover() {
    searchRequestRef.current += 1;
    setSearchPopoverOpen(false);
  }

  function noteSnapshotLoaded(snapshot: InventorySnapshot, reloadMigratedFocus = true) {
    if (snapshotRevisionRef.current !== snapshot.savedAt) {
      snapshotRevisionRef.current = snapshot.savedAt;
      invalidateEnrichedMaps();
    }
    setSnapshotWorkspaceId(snapshot.workspaceId);
    setSnapshotSavedAt(snapshot.savedAt);
    noteSnapshotFreshness(snapshot.staleReasons ?? []);
    setSnapshotSourceSummary(sourceSummary(snapshot));
    setAnalysisCoverage(coverageFromSnapshot(snapshot));

    const target = visualTargetRef.current;
    if (
      target?.workspaceId !== snapshot.workspaceId ||
      target.mode !== "api-flow" ||
      !target.focusId ||
      !snapshot.items.some((item) => item.id.startsWith(`${target.focusId}#handler=`))
    ) {
      return;
    }

    // Older snapshots stored one route focus even when several handlers shared the same path.
    // The binding split cannot infer which handler the user meant, so return to a neutral answer.
    saveMapContext(snapshot.workspaceId, "api-flow", null);
    setMapMode("api-flow");
    clearVisualSelection();
    setVisualMap(null);
    setVisualMapKey(null);
    if (reloadMigratedFocus) {
      void loadVisualMap(null, "api-flow", snapshot.workspaceId);
    }
  }

  function noteSnapshotFreshness(staleReasons: string[]) {
    setSnapshotStaleReasons(staleReasons);
    if (staleReasons.length > 0) {
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
    }
  }

  function clearVisualMapState(error: string | null = null, detail: string | null = null) {
    visualMapRequestRef.current += 1;
    invalidateEnrichedMaps();
    setVisualMap(null);
    setVisualMapLoading(false);
    setVisualMapEnriching(false);
    setVisualMapStatus(null);
    setSnapshotSavedAt(null);
    setSnapshotStaleReasons([]);
    setSnapshotSourceSummary(null);
    setAnalysisCoverage(null);
    setSnapshotWorkspaceId(null);
    snapshotRevisionRef.current = null;
    setProjectionElapsedMs(null);
    setVisualStateWorkspaceId(null);
    visualTargetRef.current = null;
    setVisualTargetKey(null);
    setVisualMapKey(null);
    setVisualMapError(error);
    setVisualMapErrorDetail(detail);
    setSearchPopoverOpen(false);
    clearVisualSelection();
  }

  function resetVisualMap() {
    const workspaceId = currentWorkspaceIdRef.current;
    if (workspaceId) {
      resetMapContext(workspaceId);
    }
    setMapMode("atlas");
    clearVisualMapState();
  }

  function invalidateEnrichedMaps() {
    evidenceGenerationRef.current += 1;
    visualMapCacheRef.current.clear();
    visualMapRequestsRef.current.clear();
  }

  function clearVisualSelection() {
    selectedVisualNodeRef.current = null;
    selectedVisualEdgeRef.current = null;
    setSelectedVisualNode(null);
    setSelectedVisualEdge(null);
  }

  function syncVisualSelection(map: VisualMap) {
    const edge = selectedVisualEdgeRef.current
      ? (map.edges.find((item) => item.id === selectedVisualEdgeRef.current?.id) ?? null)
      : null;
    const node = edge
      ? null
      : selectedVisualNodeRef.current
        ? (map.nodes.find((item) => item.id === selectedVisualNodeRef.current?.id) ?? null)
        : null;
    selectedVisualNodeRef.current = node;
    selectedVisualEdgeRef.current = edge;
    setSelectedVisualNode(node);
    setSelectedVisualEdge(edge);
  }

  function selectVisualNode(node: VisualNode | null) {
    selectedVisualEdgeRef.current = null;
    selectedVisualNodeRef.current = node;
    setSelectedVisualEdge(null);
    setSelectedVisualNode(node);
  }

  function selectVisualEdge(edge: VisualEdge | null) {
    selectedVisualNodeRef.current = null;
    selectedVisualEdgeRef.current = edge;
    setSelectedVisualNode(null);
    setSelectedVisualEdge(edge);
  }

  const currentVisualMap =
    visualMap?.workspaceId === currentWorkspaceId &&
    (visualMapLoading || (visualMap.mode === mapMode && visualMapKey === visualTargetKey))
      ? visualMap
      : null;
  const currentFocusId =
    visualTargetRef.current?.workspaceId === currentWorkspaceId ? visualTargetRef.current.focusId : null;
  const workspaceStateMatches = visualStateWorkspaceId === currentWorkspaceId;
  const snapshotMatches = snapshotWorkspaceId === currentWorkspaceId;
  const visibleSelectedNode = selectedVisualNode
    ? (currentVisualMap?.nodes.find((node) => node.id === selectedVisualNode.id) ??
      (currentVisualMap?.focus === selectedVisualNode.id ? selectedVisualNode : null))
    : null;
  const visibleSelectedEdge = selectedVisualEdge
    ? (currentVisualMap?.edges.find((edge) => edge.id === selectedVisualEdge.id) ?? null)
    : null;
  const transitioning = Boolean(currentWorkspaceId && visualMapLoading);

  return {
    visualMap: currentVisualMap,
    visualMapLoading: transitioning,
    visualMapEnriching: workspaceStateMatches ? visualMapEnriching : false,
    visualMapStatus: workspaceStateMatches ? visualMapStatus : null,
    visualMapError: workspaceStateMatches ? visualMapError : null,
    visualMapErrorDetail: workspaceStateMatches ? visualMapErrorDetail : null,
    snapshotSavedAt: snapshotMatches ? snapshotSavedAt : null,
    snapshotStaleReasons: snapshotMatches ? snapshotStaleReasons : [],
    snapshotSourceSummary: snapshotMatches ? snapshotSourceSummary : null,
    analysisCoverage: snapshotMatches ? analysisCoverage : null,
    projectionElapsedMs: workspaceStateMatches ? projectionElapsedMs : null,
    mapMode,
    mapFocusId: currentFocusId,
    compositionFocusIds,
    relationView,
    changeIntent,
    searchQuery,
    searchPopoverOpen,
    selectedVisualNode: visibleSelectedNode,
    selectedVisualEdge: visibleSelectedEdge,
    searchSummary,
    searchGroups,
    setSearchQuery: updateSearchQuery,
    showMapMode,
    toggleCompositionFocus,
    clearCompositionFocus,
    setRelationView: updateRelationView,
    setChangeIntent: updateChangeIntent,
    runSearch,
    selectSearchResult,
    openSearchPopover,
    closeSearchPopover,
    setSelectedVisualNode: selectVisualNode,
    setSelectedVisualEdge: selectVisualEdge,
    clearVisualSelection,
    noteSnapshotLoaded,
    noteSnapshotFreshness,
    clearVisualMap: resetVisualMap,
    refreshInventorySnapshot,
  };
}
