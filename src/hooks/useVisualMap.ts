import { invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useRef, useState } from "react";
import { commandErrorCode, toUserError } from "../app/operationStatus";
import { hasTauriRuntime } from "../app/tauriRuntime";
import {
  collectSearchResults,
  groupSearchResults,
  searchCollectionFromInventoryResult,
  searchScopeText,
  searchSummaryText,
  type SearchCollection,
} from "../visual/search";
import { resetMapContext, saveMapContext, savedMapContext } from "../visual/mapContext";
import type { CodeInventory, CodeInventoryItem, DbInventory } from "../types/workspace";
import type { SearchResult, SearchResultGroup } from "../types/controls";
import type {
  AnalysisCoverage,
  ChangeIntent,
  InventoryBootstrap,
  InventorySearchResult,
  InventorySnapshot,
  VisualEdge,
  VisualMap,
  VisualNode,
} from "../types/visual-map";

type SearchContext = {
  codeInventory: CodeInventory | null;
  dbInventory: DbInventory | null;
  selectCodeItem: (item: CodeInventoryItem) => void;
  selectDbTable: (tableKey: string) => void;
};

const DEFAULT_CHANGE_INTENT: ChangeIntent = { kind: "rename", value: null };

export function useVisualMap({
  currentWorkspaceId,
  onOperation,
}: {
  currentWorkspaceId: string | null;
  onOperation?: (action: string) => void;
}) {
  const [visualMap, setVisualMap] = useState<VisualMap | null>(null);
  const [visualMapLoading, setVisualMapLoading] = useState(false);
  const [visualMapEnriching, setVisualMapEnriching] = useState(false);
  const [visualMapStatus, setVisualMapStatus] = useState<string | null>(null);
  const [visualMapError, setVisualMapError] = useState<string | null>(null);
  const [visualMapErrorDetail, setVisualMapErrorDetail] = useState<string | null>(null);
  const [snapshotSavedAt, setSnapshotSavedAt] = useState<string | null>(null);
  const [snapshotStaleReasons, setSnapshotStaleReasons] = useState<string[]>([]);
  const [snapshotSourceSummary, setSnapshotSourceSummary] = useState<string | null>(null);
  const [analysisCoverage, setAnalysisCoverage] = useState<AnalysisCoverage | null>(null);
  const [snapshotWorkspaceId, setSnapshotWorkspaceId] = useState<string | null>(null);
  const [projectionElapsedMs, setProjectionElapsedMs] = useState<number | null>(null);
  const [visualStateWorkspaceId, setVisualStateWorkspaceId] = useState<string | null>(currentWorkspaceId);
  const [visualTargetKey, setVisualTargetKey] = useState<string | null>(null);
  const [visualMapKey, setVisualMapKey] = useState<string | null>(null);
  const [mapMode, setMapMode] = useState("atlas");
  const [changeIntent, setChangeIntentState] = useState<ChangeIntent>(DEFAULT_CHANGE_INTENT);
  const [searchQuery, setSearchQueryValue] = useState("");
  const [searchPopoverOpen, setSearchPopoverOpen] = useState(false);
  const [searchSummary, setSearchSummary] = useState<string | null>(null);
  const [searchGroups, setSearchGroups] = useState<SearchResultGroup[]>([]);
  const [selectedVisualNode, setSelectedVisualNode] = useState<VisualNode | null>(null);
  const [selectedVisualEdge, setSelectedVisualEdge] = useState<VisualEdge | null>(null);
  const selectedVisualNodeRef = useRef<VisualNode | null>(null);
  const selectedVisualEdgeRef = useRef<VisualEdge | null>(null);
  const autoSelectFocusRef = useRef(false);
  const currentWorkspaceIdRef = useRef<string | null>(currentWorkspaceId);
  const changeIntentRef = useRef<ChangeIntent>(DEFAULT_CHANGE_INTENT);
  const visualTargetRef = useRef<{ workspaceId: string; mode: string; focusId: string | null } | null>(null);
  const visualMapRequestRef = useRef(0);
  const evidenceGenerationRef = useRef(0);
  const enrichedMapCacheRef = useRef(new Map<string, VisualMap>());
  const enrichedMapRequestsRef = useRef(new Map<string, Promise<VisualMap>>());
  const searchContextRef = useRef<SearchContext | null>(null);
  const searchRequestRef = useRef(0);

  useLayoutEffect(() => {
    currentWorkspaceIdRef.current = currentWorkspaceId;
    invalidateEnrichedMaps();
    if (!currentWorkspaceId) {
      clearVisualMapState();
      return;
    }

    const context = savedMapContext(currentWorkspaceId);
    clearVisualSelection();
    setMapMode(context.mode);
    autoSelectFocusRef.current = Boolean(context.focusId);
    void loadVisualMap(context.focusId, context.mode, currentWorkspaceId);
  }, [currentWorkspaceId]);

  async function loadVisualMap(
    focusId?: string | null,
    mode = "atlas",
    workspaceId = currentWorkspaceIdRef.current,
  ): Promise<VisualMap | null> {
    if (!workspaceId) {
      clearVisualMapState();
      return null;
    }
    onOperation?.("map-load");
    const requestId = ++visualMapRequestRef.current;
    const startedAt = performance.now();
    const requestChangeIntent = mode === "column-impact" ? { ...changeIntentRef.current } : null;
    const targetKey = mapRequestKey(workspaceId, mode, focusId, requestChangeIntent);
    visualTargetRef.current = { workspaceId, mode, focusId: focusId ?? null };
    setVisualStateWorkspaceId(workspaceId);
    setVisualTargetKey(targetKey);
    setVisualMapEnriching(false);
    const isCurrentRequest = () =>
      visualMapRequestRef.current === requestId && currentWorkspaceIdRef.current === workspaceId;

    try {
      setVisualMapLoading(true);
      setVisualMapStatus("캔버스 준비 중");
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      const map = await invoke<VisualMap>("get_visual_map", {
        workspaceId,
        focusId: focusId ?? null,
        mode,
        changeIntent: requestChangeIntent,
        enrichCodeEvidence: false,
      });
      if (!isCurrentRequest()) {
        return null;
      }
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      const shouldEnrichCodeEvidence = Boolean(
        (mode === "api-flow" && map.apiReading?.dbCandidates.length) ||
          (mode === "table-usage" && focusId?.startsWith("db:table:")) ||
          (mode === "column-impact" && focusId?.startsWith("db:column:")),
      );
      if (shouldEnrichCodeEvidence) {
        setVisualMapStatus(`확정 근거 ${map.nodes.length}개 확인 · 코드 후보 확인 중`);
        const enriched = await enrichVisualMap({
          workspaceId,
          focusId: focusId ?? null,
          mode,
          changeIntent: requestChangeIntent,
          requestId,
          targetKey,
          fallbackMap: map,
        });
        if (isCurrentRequest()) {
          setProjectionElapsedMs(Math.round(performance.now() - startedAt));
        }
        return enriched;
      }
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
    fallbackMap,
  }: {
    workspaceId: string;
    focusId: string | null;
    mode: string;
    changeIntent: ChangeIntent | null;
    requestId: number;
    targetKey: string;
    fallbackMap: VisualMap;
  }): Promise<VisualMap | null> {
    const generation = evidenceGenerationRef.current;
    const cacheKey = `${generation}:${targetKey}`;
    const isCurrentRequest = () =>
      evidenceGenerationRef.current === generation &&
      visualMapRequestRef.current === requestId &&
      currentWorkspaceIdRef.current === workspaceId;
    if (isCurrentRequest()) {
      setVisualMapEnriching(true);
    }

    try {
      let map = enrichedMapCacheRef.current.get(cacheKey);
      if (!map) {
        let request = enrichedMapRequestsRef.current.get(cacheKey);
        if (!request) {
          request = invoke<VisualMap>("get_visual_map", {
            workspaceId,
            focusId,
            mode,
            changeIntent,
            enrichCodeEvidence: true,
          });
          enrichedMapRequestsRef.current.set(cacheKey, request);
        }
        map = await request;
        if (evidenceGenerationRef.current === generation) {
          enrichedMapCacheRef.current.set(cacheKey, map);
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
    } catch (error) {
      if (!isCurrentRequest()) {
        return null;
      }
      const uiError = toUserError(error, "코드 후보를 확인하지 못했습니다");
      setVisualMap(fallbackMap);
      setVisualMapKey(targetKey);
      syncVisualSelection(fallbackMap);
      setVisualMapStatus("DB 확정 근거 표시 · 코드 후보 확인 실패");
      setVisualMapErrorDetail(uiError.details);
      return fallbackMap;
    } finally {
      enrichedMapRequestsRef.current.delete(cacheKey);
      if (isCurrentRequest()) {
        setVisualMapEnriching(false);
      }
    }
  }

  async function refreshInventorySnapshot(workspaceId: string): Promise<boolean> {
    try {
      const bootstrap = await invoke<InventoryBootstrap | null>("load_inventory_bootstrap", { workspaceId });
      if (!bootstrap) {
        return false;
      }
      if (currentWorkspaceIdRef.current !== workspaceId) {
        return false;
      }
      const { snapshot } = bootstrap;
      invalidateEnrichedMaps();
      noteSnapshotLoaded(snapshot);
      const context = savedMapContext(workspaceId);
      setMapMode(context.mode);
      const map = await loadVisualMap(context.focusId, context.mode, workspaceId);
      if (!mapAnswersMode(map, context.mode)) {
        setMapMode("atlas");
        saveMapContext(workspaceId, "atlas", null);
        await loadVisualMap(null, "atlas", workspaceId);
      }
      return true;
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
    autoSelectFocusRef.current = Boolean(focusId);
    if (!preserveSearch) {
      setSearchQueryValue("");
      setSearchPopoverOpen(false);
      setSearchSummary(null);
      setSearchGroups([]);
    } else if (mode !== "search-focus") {
      setSearchPopoverOpen(false);
    }
    if (currentWorkspaceIdRef.current) {
      saveMapContext(currentWorkspaceIdRef.current, mode, focusId);
    }
    void loadVisualMap(focusId, mode);
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

  function refreshSearchResults(query: string, context: SearchContext | null) {
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
    const collection = collectSearchResults(query, context.codeInventory, context.dbInventory);
    presentSearchCollection(collection);
    if (
      (!context.codeInventory?.partial && !context.dbInventory?.partial) ||
      !currentWorkspaceIdRef.current ||
      !hasTauriRuntime()
    ) {
      return;
    }

    const requestId = ++searchRequestRef.current;
    const workspaceId = currentWorkspaceIdRef.current;
    void invoke<InventorySearchResult>("search_inventory", { workspaceId, query })
      .then((result) => {
        if (searchRequestRef.current === requestId && currentWorkspaceIdRef.current === workspaceId) {
          presentSearchCollection(searchCollectionFromInventoryResult(result));
        }
      })
      .catch(() => {
        // The bounded local index remains usable when a background full search fails.
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

  function runSearch({ codeInventory, dbInventory, selectCodeItem, selectDbTable }: SearchContext) {
    searchContextRef.current = { codeInventory, dbInventory, selectCodeItem, selectDbTable };
    setSearchPopoverOpen(true);
    const query = searchQuery.trim().toLowerCase();
    if (!query) {
      setSearchSummary(`검색어를 입력하면 ${searchScopeText(codeInventory, dbInventory)}을 함께 찾습니다.`);
      setSearchGroups([]);
      showMapMode("search-focus", null, true);
      focusSearchInput();
      return;
    }

    if ((codeInventory?.partial || dbInventory?.partial) && currentWorkspaceIdRef.current && hasTauriRuntime()) {
      refreshSearchResults(query, searchContextRef.current);
      showMapMode("search-focus", null, true);
      return;
    }

    const collection = collectSearchResults(query, codeInventory, dbInventory);
    const grouped = groupSearchResults(collection.results);
    if (query.length < 2) {
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      showMapMode("search-focus", null, true);
      return;
    }
    if (collection.truncated) {
      setSearchSummary(`${searchSummaryText(collection)} 그룹별 상위 결과만 보여줍니다.`);
      setSearchGroups(grouped);
      showMapMode("search-focus", null, true);
      return;
    }
    setSearchSummary(searchSummaryText(collection));
    setSearchGroups(grouped);
    const firstResult = grouped[0]?.results[0] ?? null;
    if (firstResult) {
      selectSearchResult(firstResult);
      return;
    }
    showMapMode("search-focus", null, true);
  }

  function focusSearchInput() {
    window.requestAnimationFrame(() => {
      const input = document.querySelector<HTMLInputElement>("#global-inventory-search");
      input?.focus();
      input?.select();
    });
  }

  function selectSearchResult(result: SearchResult) {
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
      setSearchSummary(`검색어를 입력하면 ${searchScopeText(searchContextRef.current?.codeInventory ?? null, searchContextRef.current?.dbInventory ?? null)}을 함께 찾습니다.`);
      setSearchGroups([]);
      setSearchPopoverOpen(true);
      return;
    }
    refreshSearchResults(query, searchContextRef.current);
    setSearchPopoverOpen(true);
  }

  function closeSearchPopover() {
    setSearchPopoverOpen(false);
  }

  function noteSnapshotLoaded(snapshot: InventorySnapshot) {
    setSnapshotWorkspaceId(snapshot.workspaceId);
    setSnapshotSavedAt(snapshot.savedAt);
    noteSnapshotFreshness(snapshot.staleReasons ?? []);
    setSnapshotSourceSummary(sourceSummary(snapshot));
    setAnalysisCoverage(coverageFromSnapshot(snapshot));
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
    enrichedMapCacheRef.current.clear();
    enrichedMapRequestsRef.current.clear();
  }

  function clearVisualSelection() {
    autoSelectFocusRef.current = false;
    selectedVisualNodeRef.current = null;
    selectedVisualEdgeRef.current = null;
    setSelectedVisualNode(null);
    setSelectedVisualEdge(null);
  }

  function syncVisualSelection(map: VisualMap) {
    const edge = selectedVisualEdgeRef.current
      ? map.edges.find((item) => item.id === selectedVisualEdgeRef.current?.id) ?? null
      : null;
    const node = edge
      ? null
      : selectedVisualNodeRef.current
        ? map.nodes.find((item) => item.id === selectedVisualNodeRef.current?.id) ?? null
        : autoSelectFocusRef.current
          ? map.nodes.find((item) => item.id === map.focus) ?? null
          : null;
    selectedVisualNodeRef.current = node;
    selectedVisualEdgeRef.current = edge;
    autoSelectFocusRef.current = false;
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
    visualTargetRef.current?.workspaceId === currentWorkspaceId
      ? visualTargetRef.current.focusId
      : null;
  const workspaceStateMatches = visualStateWorkspaceId === currentWorkspaceId;
  const snapshotMatches = snapshotWorkspaceId === currentWorkspaceId;
  const visibleSelectedNode = selectedVisualNode
    ? currentVisualMap?.nodes.find((node) => node.id === selectedVisualNode.id) ??
      (currentVisualMap?.focus === selectedVisualNode.id ? selectedVisualNode : null)
    : null;
  const visibleSelectedEdge = selectedVisualEdge
    ? currentVisualMap?.edges.find((edge) => edge.id === selectedVisualEdge.id) ?? null
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
    changeIntent,
    searchQuery,
    searchPopoverOpen,
    selectedVisualNode: visibleSelectedNode,
    selectedVisualEdge: visibleSelectedEdge,
    searchSummary,
    searchGroups,
    setSearchQuery: updateSearchQuery,
    showMapMode,
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

function sourceSummary(snapshot: InventorySnapshot): string | null {
  const code = snapshot.metadata?.code;
  const codeLabel = code?.sourceRevisionLabel
    ? code.sourceType === "github-clone"
      ? `${code.sourceRevisionLabel} · 원격 최신 여부 미확인`
      : `${code.sourceRevisionLabel} · 로컬 상태 확인`
    : null;
  const labels = [codeLabel, snapshot.metadata?.db?.sourceRevisionLabel].filter(
    (label): label is string => Boolean(label),
  );
  return labels.length > 0 ? labels.join(" · ") : null;
}

function coverageFromSnapshot(snapshot: InventorySnapshot): AnalysisCoverage {
  const code = snapshot.metadata?.code;
  const db = snapshot.metadata?.db;
  const gaps = snapshot.metadata?.gaps ?? [];
  return {
    code: {
      available: Boolean(code),
      observed: code?.resultCount ?? null,
      total: null,
      limit: code?.limitApplied ?? null,
      truncated: Boolean(code?.truncated || code?.limitClamped),
    },
    db: {
      available: Boolean(db),
      observed: db?.resultCount ?? null,
      total: db?.totalTables ?? null,
      limit: db?.limitApplied ?? null,
      truncated: Boolean(db?.truncated || db?.limitClamped),
    },
    gaps: gaps.filter((gap) => gap.kind !== "db-capability").length,
    capabilities: gaps.filter((gap) => gap.kind === "db-capability").length,
    reindexRequired: Boolean(snapshot.metadata?.migration?.reindexRequired),
  };
}

function mapRequestKey(
  workspaceId: string,
  mode: string,
  focusId?: string | null,
  changeIntent?: ChangeIntent | null,
): string {
  return `${workspaceId}\u0000${mode}\u0000${focusId ?? ""}\u0000${changeIntent?.kind ?? ""}\u0000${changeIntent?.value ?? ""}`;
}

function mapAnswersMode(map: VisualMap | null, mode: string): boolean {
  if (!map) {
    return false;
  }
  if (mode === "api-flow") {
    return Boolean(map.apiReading);
  }
  if (mode === "table-usage" || mode === "column-impact") {
    return Boolean(map.reviewBoard);
  }
  return map.nodes.length > 0;
}

function selectedSearchSummary(result: SearchResult): string {
  const target = result.subtitle ? `${result.title} · ${result.subtitle}` : result.title;
  return `${searchModeLabelForResult(result)} · ${target}`;
}

function searchModeForResult(result: SearchResult): string {
  if (result.id.startsWith("api:")) return "api-flow";
  if (result.id.startsWith("table:")) return "table-usage";
  if (result.id.startsWith("column:")) return "column-impact";
  return "search-focus";
}

function searchModeLabelForResult(result: SearchResult): string {
  if (result.id.startsWith("api:")) return "API가 닿는 코드";
  if (result.id.startsWith("table:")) return "테이블 연결";
  if (result.id.startsWith("column:")) return "컬럼 변경 범위";
  return "대상 주변 근거";
}
