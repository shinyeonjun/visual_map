import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { toUserError } from "../app/operationStatus";
import { collectSearchResults, groupSearchResults, searchScopeText, searchSummaryText } from "../visual/search";
import { saveMapContext, savedMapContext } from "../visual/mapContext";
import type { CodeInventory, CodeInventoryItem, DbInventory } from "../types/workspace";
import type { SearchResult, SearchResultGroup } from "../types/controls";
import type { InventorySnapshot, VisualEdge, VisualMap, VisualNode } from "../types/visual-map";

type SearchContext = {
  codeInventory: CodeInventory | null;
  dbInventory: DbInventory | null;
  selectCodeItem: (item: CodeInventoryItem) => void;
  selectDbTable: (tableKey: string) => void;
};

export function useVisualMap({ currentWorkspaceId }: { currentWorkspaceId: string | null }) {
  const [visualMap, setVisualMap] = useState<VisualMap | null>(null);
  const [visualMapLoading, setVisualMapLoading] = useState(false);
  const [visualMapStatus, setVisualMapStatus] = useState<string | null>(null);
  const [visualMapError, setVisualMapError] = useState<string | null>(null);
  const [visualMapErrorDetail, setVisualMapErrorDetail] = useState<string | null>(null);
  const [snapshotSavedAt, setSnapshotSavedAt] = useState<string | null>(null);
  const [projectionElapsedMs, setProjectionElapsedMs] = useState<number | null>(null);
  const [mapMode, setMapMode] = useState("atlas");
  const [searchQuery, setSearchQueryValue] = useState("");
  const [searchPopoverOpen, setSearchPopoverOpen] = useState(false);
  const [searchSummary, setSearchSummary] = useState<string | null>(null);
  const [searchGroups, setSearchGroups] = useState<SearchResultGroup[]>([]);
  const [selectedVisualNode, setSelectedVisualNode] = useState<VisualNode | null>(null);
  const [selectedVisualEdge, setSelectedVisualEdge] = useState<VisualEdge | null>(null);
  const currentWorkspaceIdRef = useRef<string | null>(currentWorkspaceId);
  const visualMapRequestRef = useRef(0);
  const codeEvidenceTimerRef = useRef<number | null>(null);
  const searchContextRef = useRef<SearchContext | null>(null);

  useEffect(() => {
    currentWorkspaceIdRef.current = currentWorkspaceId;
    if (!currentWorkspaceId) {
      clearVisualMapState();
      return;
    }

    const context = savedMapContext(currentWorkspaceId);
    setMapMode(context.mode);
    void loadVisualMap(context.focusId, context.mode, currentWorkspaceId);
  }, [currentWorkspaceId]);

  async function loadVisualMap(focusId?: string | null, mode = "atlas", workspaceId = currentWorkspaceIdRef.current) {
    if (!workspaceId) {
      clearVisualMapState();
      return;
    }
    const requestId = ++visualMapRequestRef.current;
    const startedAt = performance.now();
    cancelCodeEvidenceEnrichment();
    const isCurrentRequest = () =>
      visualMapRequestRef.current === requestId && currentWorkspaceIdRef.current === workspaceId;

    try {
      setVisualMapLoading(true);
      setVisualMapStatus("캔버스 준비 중");
      const map = await invoke<VisualMap>("get_visual_map", {
        workspaceId,
        focusId: focusId ?? null,
        mode,
        enrichCodeEvidence: false,
      });
      if (!isCurrentRequest()) {
        return;
      }
      setVisualMap(map);
      setProjectionElapsedMs(Math.round(performance.now() - startedAt));
      syncVisualSelection(map);
      autoSelectFocusNode(map, focusId ?? null);
      setVisualMapStatus(map.nodes.length > 0 ? `캔버스 항목 ${map.nodes.length}개 표시` : "캔버스 항목 없음");
      setVisualMapError(null);
      setVisualMapErrorDetail(null);
      scheduleCodeEvidenceEnrichment(focusId ?? map.focus, mode, workspaceId, isCurrentRequest);
    } catch (error) {
      if (!isCurrentRequest()) {
        return;
      }
      if (isMissingInventorySnapshot(String(error))) {
        setVisualMap(null);
        clearVisualSelection();
        setVisualMapError(null);
        setVisualMapErrorDetail(null);
        setVisualMapStatus("코드/DB 읽기 결과 필요");
        setProjectionElapsedMs(Math.round(performance.now() - startedAt));
        return;
      }

      const uiError = toUserError(error, "캔버스를 만들지 못했습니다");
      setVisualMap(null);
      setVisualMapStatus(null);
      clearVisualSelection();
      setVisualMapError(uiError.message);
      setVisualMapErrorDetail(uiError.details);
      setProjectionElapsedMs(Math.round(performance.now() - startedAt));
    } finally {
      if (isCurrentRequest()) {
        setVisualMapLoading(false);
      }
    }
  }

  function scheduleCodeEvidenceEnrichment(
    focusId: string | null,
    mode: string,
    workspaceId: string,
    isCurrentRequest: () => boolean,
  ) {
    const shouldEnrich =
      (mode === "table-usage" && focusId?.startsWith("db:table:")) ||
      (mode === "column-impact" && focusId?.startsWith("db:column:"));
    if (!shouldEnrich) {
      return;
    }

    codeEvidenceTimerRef.current = window.setTimeout(() => {
      codeEvidenceTimerRef.current = null;
      if (!isCurrentRequest()) {
        return;
      }
      const startedAt = performance.now();
      void invoke<VisualMap>("get_visual_map", {
        workspaceId,
        focusId,
        mode,
        enrichCodeEvidence: true,
      })
        .then((map) => {
          if (!isCurrentRequest()) {
            return;
          }
          setVisualMap(map);
          setProjectionElapsedMs(Math.round(performance.now() - startedAt));
          syncVisualSelection(map);
          autoSelectFocusNode(map, focusId);
          setVisualMapStatus(
            map.nodes.length > 0
              ? `캔버스 항목 ${map.nodes.length}개 · 코드 근거 보강`
              : "캔버스 항목 없음",
          );
        })
        .catch(() => {
          // 기본 map을 유지한다. 검색 실패 범위는 정상 응답의 review unknown에 기록된다.
        });
    }, 200);
  }

  function cancelCodeEvidenceEnrichment() {
    if (codeEvidenceTimerRef.current !== null) {
      window.clearTimeout(codeEvidenceTimerRef.current);
      codeEvidenceTimerRef.current = null;
    }
  }

  async function saveInventorySnapshot(workspaceId: string, code: CodeInventory | null, db: DbInventory | null) {
    if (!code && !db) {
      return;
    }

    try {
      const snapshot = await invoke<InventorySnapshot>("save_inventory_snapshot", { workspaceId, code, db });
      if (currentWorkspaceIdRef.current !== workspaceId) {
        return;
      }
      setSnapshotSavedAt(snapshot.savedAt);
      await loadVisualMap(null, mapMode, workspaceId);
    } catch (error) {
      if (currentWorkspaceIdRef.current !== workspaceId) {
        return;
      }
      const uiError = toUserError(error, "코드/DB 읽기 결과를 저장하지 못했습니다");
      setVisualMapError(uiError.message);
      setVisualMapErrorDetail(uiError.details);
    }
  }

  function showMapMode(mode: string, focusId?: string | null) {
    setMapMode(mode);
    if (currentWorkspaceIdRef.current) {
      saveMapContext(currentWorkspaceIdRef.current, mode, focusId);
    }
    clearVisualSelection();
    void loadVisualMap(focusId, mode);
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
    const results = collectSearchResults(query, context.codeInventory, context.dbInventory);
    if (query.length < 2) {
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      return;
    }
    setSearchSummary(results.length > 18 ? `${searchSummaryText(results)} 상위 결과만 보여줍니다.` : searchSummaryText(results));
    setSearchGroups(groupSearchResults(results));
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

    const results = collectSearchResults(query, codeInventory, dbInventory);
    const grouped = groupSearchResults(results);
    if (query.length < 2) {
      setSearchSummary("두 글자 이상 입력하면 더 정확합니다.");
      setSearchGroups([]);
      showMapMode("search-focus", null);
      return;
    }
    if (results.length > 18) {
      setSearchSummary(`${searchSummaryText(results)} 상위 결과만 보여줍니다.`);
      setSearchGroups(grouped);
      showMapMode("search-focus", null);
      return;
    }
    setSearchSummary(searchSummaryText(results));
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
      const input = document.querySelector<HTMLInputElement>('input[aria-label="프로젝트 항목 검색"]');
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

  function clearVisualMapState(error: string | null = null, detail: string | null = null) {
    visualMapRequestRef.current += 1;
    cancelCodeEvidenceEnrichment();
    setVisualMap(null);
    setVisualMapLoading(false);
    setVisualMapStatus(null);
    setSnapshotSavedAt(null);
    setProjectionElapsedMs(null);
    setVisualMapError(error);
    setVisualMapErrorDetail(detail);
    setSearchPopoverOpen(false);
    clearVisualSelection();
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

  return {
    visualMap,
    visualMapLoading,
    visualMapStatus,
    visualMapError,
    visualMapErrorDetail,
    snapshotSavedAt,
    projectionElapsedMs,
    mapMode,
    searchQuery,
    searchPopoverOpen,
    selectedVisualNode,
    selectedVisualEdge,
    searchSummary,
    searchGroups,
    setSearchQuery: updateSearchQuery,
    showMapMode,
    runSearch,
    selectSearchResult,
    openSearchPopover,
    closeSearchPopover,
    setSelectedVisualNode,
    setSelectedVisualEdge,
    clearVisualSelection,
    noteSnapshotLoaded: setSnapshotSavedAt,
    clearVisualMap: () => clearVisualMapState(),
    saveInventorySnapshot,
  };
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

function isMissingInventorySnapshot(value: string): boolean {
  return (
    value.includes("코드/DB 읽기 결과") ||
    value.includes("inventory snapshot") ||
    (value.includes("source") && value.includes("snapshot")) ||
    value.includes("os error 2") ||
    value.includes("os error 3")
  );
}
