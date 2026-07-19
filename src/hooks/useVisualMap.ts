import { invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useRef, useState } from "react";
import { commandErrorCode, toUserError } from "../app/operationStatus";
import { collectSearchResults, groupSearchResults, searchScopeText, searchSummaryText } from "../visual/search";
import { saveMapContext, savedMapContext } from "../visual/mapContext";
import type { CodeInventory, CodeInventoryItem, DbInventory } from "../types/workspace";
import type { SearchResult, SearchResultGroup } from "../types/controls";
import type { AnalysisCoverage, ChangeIntent, InventorySnapshot, VisualEdge, VisualMap, VisualNode } from "../types/visual-map";

type SearchContext = {
  codeInventory: CodeInventory | null;
  dbInventory: DbInventory | null;
  selectCodeItem: (item: CodeInventoryItem) => void;
  selectDbTable: (tableKey: string) => void;
};

const DEFAULT_CHANGE_INTENT: ChangeIntent = { kind: "rename", value: null };

export function useVisualMap({ currentWorkspaceId }: { currentWorkspaceId: string | null }) {
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
  const currentWorkspaceIdRef = useRef<string | null>(currentWorkspaceId);
  const changeIntentRef = useRef<ChangeIntent>(DEFAULT_CHANGE_INTENT);
  const visualTargetRef = useRef<{ workspaceId: string; mode: string; focusId: string | null } | null>(null);
  const visualMapRequestRef = useRef(0);
  const evidenceGenerationRef = useRef(0);
  const enrichedMapCacheRef = useRef(new Map<string, VisualMap>());
  const enrichedMapRequestsRef = useRef(new Map<string, Promise<VisualMap>>());
  const searchContextRef = useRef<SearchContext | null>(null);

  useLayoutEffect(() => {
    currentWorkspaceIdRef.current = currentWorkspaceId;
    invalidateEnrichedMaps();
    if (!currentWorkspaceId) {
      clearVisualMapState();
      return;
    }

    const context = savedMapContext(currentWorkspaceId);
    setMapMode(context.mode);
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
    const requestId = ++visualMapRequestRef.current;
    const startedAt = performance.now();
    const requestChangeIntent = mode === "column-impact" ? { ...changeIntentRef.current } : null;
    const targetKey = mapRequestKey(workspaceId, mode, focusId, requestChangeIntent);
    visualTargetRef.current = { workspaceId, mode, focusId: focusId ?? null };
    setVisualStateWorkspaceId(workspaceId);
    setVisualTargetKey(targetKey);
    setVisualMapEnriching(false);
    const shouldEnrichCodeEvidence = Boolean(
      (mode === "table-usage" && focusId?.startsWith("db:table:")) ||
        (mode === "column-impact" && focusId?.startsWith("db:column:")),
    );
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
      setVisualMap(map);
      setVisualMapKey(targetKey);
      setProjectionElapsedMs(Math.round(performance.now() - startedAt));
      syncVisualSelection(map);
      autoSelectFocusNode(map, focusId ?? null);
      setVisualMapStatus(
        map.nodes.length > 0
          ? shouldEnrichCodeEvidence
            ? `확정 근거 ${map.nodes.length}개 표시 · 코드 후보 확인 중`
            : `캔버스 항목 ${map.nodes.length}개 표시`
          : "캔버스 항목 없음",
      );
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      if (shouldEnrichCodeEvidence) {
        void enrichVisualMap({
          workspaceId,
          focusId: focusId ?? null,
          mode,
          changeIntent: requestChangeIntent,
          requestId,
          targetKey,
        });
      }
      return map;
    } catch (error) {
      if (!isCurrentRequest()) {
        return null;
      }
      if (commandErrorCode(error) === "snapshot_missing") {
        setVisualMap(null);
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
  }: {
    workspaceId: string;
    focusId: string | null;
    mode: string;
    changeIntent: ChangeIntent | null;
    requestId: number;
    targetKey: string;
  }) {
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
        return;
      }
      setVisualMap(map);
      setVisualMapKey(targetKey);
      syncVisualSelection(map);
      autoSelectFocusNode(map, focusId);
      setVisualMapStatus(
        map.nodes.length > 0 ? `캔버스 항목 ${map.nodes.length}개 · 코드 후보 확인 완료` : "캔버스 항목 없음",
      );
    } catch (error) {
      if (!isCurrentRequest()) {
        return;
      }
      const uiError = toUserError(error, "코드 후보를 확인하지 못했습니다");
      setVisualMapStatus("DB 확정 근거 표시 · 코드 후보 확인 실패");
      setVisualMapErrorDetail(uiError.details);
    } finally {
      enrichedMapRequestsRef.current.delete(cacheKey);
      if (isCurrentRequest()) {
        setVisualMapEnriching(false);
      }
    }
  }

  async function saveInventorySnapshot(
    workspaceId: string,
    code: CodeInventory | null,
    db: DbInventory | null,
  ): Promise<boolean> {
    if (!code && !db) {
      return false;
    }

    try {
      const snapshot = await invoke<InventorySnapshot>("save_inventory_snapshot", { workspaceId, code, db });
      if (currentWorkspaceIdRef.current !== workspaceId) {
        return false;
      }
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
      const uiError = toUserError(error, "코드/DB 읽기 결과를 저장하지 못했습니다");
      setVisualMapError(uiError.message);
      setVisualMapErrorDetail(uiError.details);
      return false;
    }
  }

  function showMapMode(mode: string, focusId?: string | null) {
    setMapMode(mode);
    if (mode !== "search-focus") {
      setSearchPopoverOpen(false);
    }
    if (currentWorkspaceIdRef.current) {
      saveMapContext(currentWorkspaceIdRef.current, mode, focusId);
    }
    clearVisualSelection();
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
    const collection = collectSearchResults(query, context.codeInventory, context.dbInventory);
    if (query.length < 2) {
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      return;
    }
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
      showMapMode("search-focus", null);
      focusSearchInput();
      return;
    }

    const collection = collectSearchResults(query, codeInventory, dbInventory);
    const grouped = groupSearchResults(collection.results);
    if (query.length < 2) {
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      showMapMode("search-focus", null);
      return;
    }
    if (collection.truncated) {
      setSearchSummary(`${searchSummaryText(collection)} 그룹별 상위 결과만 보여줍니다.`);
      setSearchGroups(grouped);
      showMapMode("search-focus", null);
      return;
    }
    setSearchSummary(searchSummaryText(collection));
    setSearchGroups(grouped);
    const firstResult = grouped[0]?.results[0] ?? null;
    if (firstResult) {
      selectSearchResult(firstResult);
      return;
    }
    showMapMode("search-focus", null);
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
    setSnapshotStaleReasons(snapshot.staleReasons ?? []);
    setSnapshotSourceSummary(sourceSummary(snapshot));
    setAnalysisCoverage(coverageFromSnapshot(snapshot));
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

  function invalidateEnrichedMaps() {
    evidenceGenerationRef.current += 1;
    enrichedMapCacheRef.current.clear();
    enrichedMapRequestsRef.current.clear();
  }

  function clearVisualSelection() {
    setSelectedVisualNode(null);
    setSelectedVisualEdge(null);
  }

  function syncVisualSelection(map: VisualMap) {
    setSelectedVisualNode((node) => (node ? map.nodes.find((item) => item.id === node.id) ?? null : null));
    setSelectedVisualEdge((edge) => (edge ? map.edges.find((item) => item.id === edge.id) ?? null : null));
  }

  // 대상 중심 화면이 열리면 해당 항목을 자동 선택해 인스펙터가 바로 대상을 보여준다.
  // 명시적인 모드/검색 이동은 이전 선택보다 우선한다.
  function autoSelectFocusNode(map: VisualMap, requestedFocusId: string | null) {
    const focus = requestedFocusId ?? map.focus;
    if (!focus || focus === "overview" || focus === "narrow-focus") {
      return;
    }
    const focusNode = map.nodes.find((node) => node.id === focus);
    if (focusNode) {
      if (requestedFocusId) {
        setSelectedVisualEdge(null);
        setSelectedVisualNode(focusNode);
        return;
      }
      setSelectedVisualNode((current) => current ?? focusNode);
    }
  }

  const currentVisualMap =
    visualMapKey === visualTargetKey && visualMap?.workspaceId === currentWorkspaceId && visualMap.mode === mapMode
      ? visualMap
      : null;
  const workspaceStateMatches = visualStateWorkspaceId === currentWorkspaceId;
  const snapshotMatches = snapshotWorkspaceId === currentWorkspaceId;
  const visibleSelectedNode = selectedVisualNode
    ? currentVisualMap?.nodes.find((node) => node.id === selectedVisualNode.id) ?? null
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
    setSelectedVisualNode,
    setSelectedVisualEdge,
    clearVisualSelection,
    noteSnapshotLoaded,
    clearVisualMap: () => clearVisualMapState(),
    saveInventorySnapshot,
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
    gaps: snapshot.metadata?.gaps?.length ?? 0,
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
