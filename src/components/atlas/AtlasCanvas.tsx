import { Cog, FileText, LoaderCircle, Maximize2, Minus, MousePointer2, Plus, Table2, Unlink, X } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, PointerEvent, ReactNode, WheelEvent } from "react";
import { codeInventoryCodeItems, codeKindChip, dbInventoryTableKey } from "../../types/workspace";
import type { CodeInventory, CodeInventoryItem } from "../../types/workspace";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  columnLabelFromNodeId,
  columnRefFromNodeId,
  dbColumnNodeId,
  dbTableIdentityLabel,
  dbTableNodeId,
  tableKeyFromDbNodeId as tableKeyFromNodeId,
} from "../../visual/nodeIds";
import {
  visualEdgeKindLabel as edgeKindLabel,
  visualNodeKindLabel as nodeKindLabel,
} from "../../visual/labels";
import { focusDbProfileSetup } from "../common/focusSourceSetup";
import { ApiReadingHeader, ApiReadingPath } from "./ApiReadingPath";
import type { ApiReadingView } from "./ApiReadingPath";
import { ArchitectureMap, RelationBadge } from "./ArchitectureMap";
import { ImpactReviewBoard } from "./ImpactReviewBoard";
import { SetupChecklist } from "./SetupChecklist";
import {
  atlasCanvasFacts,
  atlasCanvasGuide,
  atlasModePurpose,
  atlasModeTitle,
  atlasReadOrder,
} from "./atlasGuidance";
import {
  AT_GUTTER_WIDTH,
  AT_LANE_GAP,
  AT_LANE_PAD_X,
  AT_LANE_WIDTH,
  atlasCodeKindRank,
  buildRelationBeams,
  buildRelationCounts,
  clamp,
  codeIdsFromNodeIds,
  columnMeta,
  columnNamesForTableFromNodeIds,
  compactPath,
  edgeTouchesNode,
  edgeTouchesNodeId,
  edgeTouchesTable,
  filterCodeItemsByMap,
  filterTablesByMap,
  idsInItems,
  nodeLabel,
  nodeTouchesTable,
  nodesShareTableOrId,
  rankNodeItems,
  relationFocusIdFromMapFocus,
  relationLedgerRows,
  relationLedgerScopedEdges,
  tableKeyFromFocusedTable,
  tableKeysFromNodeIds,
  takeWithPinned,
  type RelationBeam,
  type RelationLedgerRow,
  type RelationTone,
} from "./atlasRelations";

type FocusStripState = {
  label: string;
  title: string;
  meta: string;
  hint: string;
  tone: "code" | "db" | "edge" | "neutral";
};

type CanvasViewState = {
  zoom: number;
  left: number;
  top: number;
};

const RELATION_ACTION_LABEL: Record<RelationTone, string> = {
  confirmed: "1차 근거",
  typed: "구조 근거",
  candidate: "검증 필요",
  inferred: "이름 단서",
};

export function AtlasCanvas({
  openSourceManager,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  openSourceManager: () => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const mode = visualMapControls.currentMap?.mode ?? visualMapControls.mode;
  const architectureMode = mode === "atlas" || mode === "explore";
  const compositionMode = mode === "composition";
  const architectureMap =
    architectureMode && visualMapControls.currentMap && ["atlas", "explore"].includes(visualMapControls.currentMap.mode)
      ? visualMapControls.currentMap
      : null;
  const impactBoard =
    !architectureMode && ["table-usage", "column-impact"].includes(mode)
      ? visualMapControls.currentMap?.reviewBoard ?? null
      : null;
  const apiReading = mode === "api-flow" ? visualMapControls.currentMap?.apiReading ?? null : null;
  const needsTarget = visualMapControls.currentMap?.focus === "narrow-focus" && !visualMapControls.focusId;
  const projectionOnlyMode = architectureMode || Boolean(impactBoard) || Boolean(apiReading);
  const architectureDetail = Boolean(architectureMap?.focus.startsWith("group:"));
  const stageRef = useRef<HTMLDivElement | null>(null);
  const panRef = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const zoomRef = useRef(1);
  const viewStatesRef = useRef(new Map<string, CanvasViewState>());
  const [atlasZoom, setAtlasZoom] = useState(1);
  const [apiReadingView, setApiReadingView] = useState<ApiReadingView>("connections");
  const pendingFocus = visualMapControls.loading && visualMapControls.currentMap
    ? transitionFocusState(
        mode,
        visualMapControls.focusId,
        workspaceControls.codeInventory,
        dbProfileControls,
      )
    : null;

  useLayoutEffect(() => {
    const saved = viewStatesRef.current.get(mode) ?? { zoom: 1, left: 0, top: 0 };
    zoomRef.current = saved.zoom;
    setAtlasZoom(saved.zoom);
    window.requestAnimationFrame(() => {
      if (!stageRef.current) return;
      stageRef.current.scrollLeft = saved.left;
      stageRef.current.scrollTop = saved.top;
    });
  }, [mode]);

  if (visualMapControls.loading && !visualMapControls.currentMap) {
    return (
      <CanvasTransitionState
        mode={mode}
        focusId={visualMapControls.focusId}
        codeInventory={workspaceControls.codeInventory}
        dbProfileControls={dbProfileControls}
      />
    );
  }
  const routes = projectionOnlyMode ? [] : workspaceControls.codeInventory?.routes ?? [];
  const codeItems = projectionOnlyMode ? [] : codeInventoryCodeItems(workspaceControls.codeInventory);
  const fileItems = projectionOnlyMode ? [] : workspaceControls.codeInventory?.files ?? [];
  const allTables = projectionOnlyMode ? [] : dbProfileControls.inventory?.tables ?? [];
  // 선택된 테이블은 항상 보이게 앞으로 정렬한다.
  const tables = [...allTables].sort((a, b) => {
    const aSel = dbInventoryTableKey(a) === dbProfileControls.selectedTableKey ? 0 : 1;
    const bSel = dbInventoryTableKey(b) === dbProfileControls.selectedTableKey ? 0 : 1;
    return aSel - bSel;
  });
  const dbColumnCount = allTables.reduce((sum, table) => sum + table.columns.length, 0);
  const inventoryCounts = {
    routes: routes.length,
    code: codeItems.length,
    files: fileItems.length,
    tables: allTables.length,
    columns: dbColumnCount,
  };
  const hasInventoryData = routes.length > 0 || codeItems.length > 0 || fileItems.length > 0 || allTables.length > 0;
  const hasData = compositionMode
    ? Boolean(visualMapControls.currentMap?.nodes.length)
    : architectureMode
      ? Boolean(architectureMap?.nodes.length)
      : impactBoard || apiReading
        ? true
        : hasInventoryData;
  const activeMode = hasData
    ? compositionMode
      ? "관계 분석"
      : architectureMode
      ? architectureDetail
        ? "구조 영역 상세"
        : "전체 구조"
      : impactBoard
        ? impactBoard.scope === "column" ? "컬럼 변경 영향" : "테이블 사용처"
        : apiReading
          ? "API 읽기 경로"
        : atlasModeTitle(mode, inventoryCounts)
    : compositionMode
      ? "관계 분석"
      : workspaceControls.currentWorkspace
      ? "코드/DB 연결"
      : "프로젝트 연결";
  const readOrder = hasData
    ? compositionMode
      ? "선택 대상 → 근거 경로 → 관계 상세"
      : architectureMode
      ? "구조 영역 → API → 코드 → DB"
      : impactBoard
        ? "직접 영향 → 코드 후보 → 확인 필요 → 권장 확인"
        : apiReading
          ? "Route → Handler → Service/Function → Repository/Query → DB 후보"
        : atlasReadOrder(mode, inventoryCounts)
    : compositionMode ? "대상 2~8개 선택" : "프로젝트 → 코드 → DB";
  const modePurpose = hasData
    ? compositionMode
      ? "선택한 대상 사이의 근거 경로만 표시합니다"
      : architectureMode
      ? architectureDetail
        ? "선택한 구조 영역의 실제 항목만 펼쳤습니다"
        : "엔진 패키지와 DB 스키마 경계를 먼저 읽습니다"
      : impactBoard
        ? "수정 전에 확정 사실과 검증할 후보를 순서대로 읽습니다"
        : apiReading
          ? "확정 HANDLES/CALLS만 읽기 경로로 사용합니다"
        : atlasModePurpose(mode, inventoryCounts)
    : compositionMode
      ? "왼쪽 항목에서 함께 볼 대상을 선택하세요"
      : workspaceControls.currentWorkspace
      ? "코드/DB 목록을 불러오면 캔버스가 채워집니다"
      : "프로젝트를 열면 캔버스가 채워집니다";
  const analysisFocusId = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.focus
    : visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? "";
  const focusedCodeItem = codeInventoryItemFromNodeId(workspaceControls.codeInventory, analysisFocusId);
  const focusedNodeIds = new Set(visualMapControls.currentMap?.nodes.map((node) => node.id) ?? []);
  const shouldFocusCards = !impactBoard && mode !== "atlas" && focusedNodeIds.size > 0;
  const filteredRoutes = shouldFocusCards ? filterCodeItemsByMap(routes, focusedNodeIds) : routes;
  const focusedRoute = focusedCodeItem ? routes.find((item) => item.id === focusedCodeItem.id) ?? null : null;
  const visibleRoutes = includeFocusedCodeItem(filteredRoutes, focusedRoute);
  const codeBandItems = shouldFocusCards || codeItems.length === 0 ? [...codeItems, ...fileItems] : codeItems;
  const filteredCodeItems = shouldFocusCards ? filterCodeItemsByMap(codeBandItems, focusedNodeIds) : codeBandItems;
  const focusedCodeBandItem = focusedCodeItem
    ? codeBandItems.find((item) => item.id === focusedCodeItem.id) ?? null
    : null;
  const visibleCodeItems = includeFocusedCodeItem(filteredCodeItems, focusedCodeBandItem);
  const orderedCodeItems = [...visibleCodeItems].sort((a, b) => atlasCodeKindRank(a.kind) - atlasCodeKindRank(b.kind));
  const visibleTables = shouldFocusCards ? filterTablesByMap(tables, focusedNodeIds) : tables;
  const relationCounts = buildRelationCounts(visualMapControls.currentMap);
  const selectedCodeNodeId = visualMapControls.selectedNode?.source === "code"
    ? visualMapControls.selectedNode.id
    : focusedCodeItem
      ? `code:${focusedCodeItem.id}`
      : null;
  const selectedTableNodeId = dbProfileControls.selectedTableKey ? `db:table:${dbProfileControls.selectedTableKey}` : null;
  const selectedRelationFocusId = visualMapControls.selectedNode?.id
    ?? (compositionMode ? null : relationFocusIdFromMapFocus(analysisFocusId));
  const pinnedNodeIds = [
    visualMapControls.selectedEdge?.from,
    visualMapControls.selectedEdge?.to,
    selectedRelationFocusId,
    selectedCodeNodeId,
    selectedTableNodeId,
  ];
  const pinnedCodeIds = codeIdsFromNodeIds(pinnedNodeIds);
  const pinnedTableKeys = tableKeysFromNodeIds(pinnedNodeIds);
  const pinnedRouteIds = idsInItems(pinnedCodeIds, visibleRoutes, (item) => item.id);
  const pinnedCodeBandIds = idsInItems(pinnedCodeIds, orderedCodeItems, (item) => item.id);
  const pinnedVisibleTableKeys = idsInItems(pinnedTableKeys, visibleTables, dbInventoryTableKey);
  // 모드에 따라 관련 밴드만 보여주되, 선택 관계의 양끝 밴드는 숨기지 않는다.
  const showApiBand = (mode !== "table-usage" && mode !== "column-impact" && routes.length > 0) || pinnedRouteIds.size > 0;
  const showCodeBand = codeItems.length > 0 || fileItems.length > 0 || pinnedCodeBandIds.size > 0;
  const showDbBand = (mode !== "api-flow" && allTables.length > 0) || pinnedVisibleTableKeys.size > 0;
  const cardLimit = 12;
  const rankedRoutes = rankNodeItems(visibleRoutes, relationCounts, (item) => `code:${item.id}`, selectedCodeNodeId);
  const rankedCodeItems = rankNodeItems(orderedCodeItems, relationCounts, (item) => `code:${item.id}`, selectedCodeNodeId);
  const rankedTables = rankNodeItems(
    visibleTables,
    relationCounts,
    (table) => `db:table:${dbInventoryTableKey(table)}`,
    selectedTableNodeId,
  );
  const routeCards = takeWithPinned(rankedRoutes, pinnedRouteIds, (item) => item.id, cardLimit);
  const codeCards = takeWithPinned(rankedCodeItems, pinnedCodeBandIds, (item) => item.id, cardLimit);
  const tableCards = takeWithPinned(rankedTables, pinnedVisibleTableKeys, dbInventoryTableKey, cardLimit);
  const displayedRouteCards = showApiBand ? routeCards : [];
  const displayedCodeCards = showCodeBand ? codeCards : [];
  const codeBandLabel = codeBandItems.every((item) => item.kind.trim().toLowerCase() === "file")
    ? "파일"
    : mode === "column-impact"
      ? "후보 코드"
      : fileItems.length > 0 && codeItems.length > 0 && shouldFocusCards
        ? "코드/파일"
        : "코드";
  const displayedTableCards = showDbBand ? tableCards : [];
  const laneCount = Math.max(4, displayedRouteCards.length, displayedCodeCards.length, displayedTableCards.length);
  const mapStyle = {
    "--at-lanes": laneCount,
    "--at-gutter-width": `${AT_GUTTER_WIDTH}px`,
    "--at-lane-width": `${AT_LANE_WIDTH}px`,
    "--at-lane-gap": `${AT_LANE_GAP}px`,
    "--at-lane-pad-x": `${AT_LANE_PAD_X}px`,
    minWidth: `${AT_GUTTER_WIDTH + laneCount * AT_LANE_WIDTH + Math.max(0, laneCount - 1) * AT_LANE_GAP + AT_LANE_PAD_X * 2}px`,
    zoom: atlasZoom,
  } as CSSProperties;
  const bands: Array<"api" | "code" | "db"> = [
    ...(showApiBand ? ["api" as const] : []),
    ...(showCodeBand ? ["code" as const] : []),
    ...(showDbBand ? ["db" as const] : []),
  ];
  const bandNumber = (band: "api" | "code" | "db") => String(bands.indexOf(band) + 1);
  const emptyTitle = workspaceControls.currentWorkspace
    ? "코드/DB 목록을 불러오면 답이 열립니다"
    : workspaceControls.canCreateWorkspace
      ? workspaceControls.repoSourceMode === "github"
        ? "저장소 복제 준비"
        : "프로젝트 열기 준비"
      : workspaceControls.repoSourceMode === "github"
        ? "GitHub URL을 입력하세요"
        : "프로젝트 폴더를 지정하세요";
  const workspaceRequiredText = workspaceControls.repoSourceMode === "github" ? "GitHub URL 필요" : "로컬 폴더 필요";
  const canvasFacts = hasData
      ? compositionMode
      ? `선택 ${visualMapControls.compositionFocusIds.length}개 · 관계 ${visualMapControls.currentMap?.edges.length ?? 0}개`
      : architectureMode
      ? architectureCanvasFacts(architectureMap)
      : apiReading
        ? `읽기 ${apiReading.steps.length}단계 · DB 연결 ${apiReading.dbRelations?.length ?? 0}개 · 후보 ${apiReading.dbCandidates.length}개${
            apiReading.hiddenBranches > 0
              ? apiReading.hiddenBranchesIsLowerBound
                ? ` · 최소 +${apiReading.hiddenBranches} 경계 관계 · 하위 미탐색`
                : ` · +${apiReading.hiddenBranches}개 접힘`
              : ""
          }`
        : atlasCanvasFacts({
        mode,
        mapNodes: visualMapControls.currentMap?.nodes.length ?? 0,
        mapEdges: visualMapControls.currentMap?.edges.length ?? 0,
        mapWarnings: visualMapControls.currentMap?.warnings.length ?? 0,
        routes: routes.length,
        code: codeItems.length,
        files: fileItems.length,
        tables: tables.length,
        columns: dbColumnCount,
        searchSummary: visualMapControls.searchSummary,
        })
    : workspaceControls.currentWorkspace
      ? "코드 또는 DB 연결"
      : workspaceControls.canCreateWorkspace
        ? workspaceControls.repoSourceMode === "github"
          ? "저장소 복제 준비"
          : "프로젝트 열기 준비"
        : workspaceRequiredText;
  const focus = architectureMode
    ? architectureFocusState(visualMapControls)
    : impactBoard
      ? {
          label: impactBoard.scope === "column" ? "컬럼" : "테이블",
          title: impactBoard.subject,
          meta: `검토 항목 ${impactBoard.lanes.reduce((total, lane) => total + lane.total, 0)}개`,
          hint: "01 직접 영향부터 04 권장 확인까지 순서대로 검토합니다.",
          tone: "db" as const,
        }
      : apiReading
        ? {
            label: "API",
            title: apiReading.subject,
            meta: `확정 읽기 ${Math.max(0, apiReading.steps.length - 1)}개 · DB 연결 ${apiReading.dbRelations?.length ?? 0}개 · 후보 ${apiReading.dbCandidates.length}개`,
            hint: apiReading.unknowns[0]?.detail ?? "번호 순서대로 파일을 읽습니다.",
            tone: "code" as const,
          }
      : atlasFocusState(focusedCodeItem, dbProfileControls, visualMapControls, tables);
  const relationRows = relationLedgerRows(
    visualMapControls.currentMap,
    visualMapControls.selectedEdge,
    visualMapControls.selectedNode,
    selectedRelationFocusId,
  );
  const relationScopedTotal = relationLedgerScopedEdges(
    visualMapControls.currentMap,
    visualMapControls.selectedEdge,
    visualMapControls.selectedNode,
    selectedRelationFocusId,
  ).length;
  const relationTargetCodeItem = visualMapControls.selectedNode?.source === "code"
    ? codeInventoryItemFromNodeId(workspaceControls.codeInventory, visualMapControls.selectedNode.id)
    : focusedCodeItem;
  const showDisconnectedCodeFocus = Boolean(
    mode === "search-focus" &&
      relationTargetCodeItem &&
      selectedRelationFocusId &&
      relationScopedTotal === 0,
  );
  const focusedColumnLabel = columnLabelFromNodeId(analysisFocusId);
  const focusedTableKey = tableKeyFromFocusedTable(analysisFocusId);
  const selectedNodeTableKey = visualMapControls.selectedNode ? tableKeyFromNodeId(visualMapControls.selectedNode.id) : null;
  const selectedTableKey = selectedNodeTableKey ?? focusedTableKey ?? dbProfileControls.selectedTableKey;
  const selectedTableNeedsColumns = Boolean(
    selectedTableKey &&
      allTables.find((table) => dbInventoryTableKey(table) === selectedTableKey)?.columns.length === 0,
  );
  const relationEmptyReason = architectureMode
    ? visualMapControls.selectedNode
      ? `${visualMapControls.selectedNode.title}와 연결된 관계가 없습니다.`
      : undefined
    : selectedTableNeedsColumns
      ? "컬럼을 읽으면 이 테이블의 관계가 열립니다."
      : focusedColumnLabel
        ? `${focusedColumnLabel} 컬럼과 연결된 관계가 없습니다.`
        : focusedCodeItem
          ? `${focusedCodeItem.name}와 연결된 관계가 없습니다.`
          : focusedTableKey
            ? `${focusedTableKey} 테이블과 연결된 관계가 없습니다.`
            : undefined;
  const hasSelectedRelationTarget = architectureMode
    ? Boolean(visualMapControls.selectedNode)
    : Boolean(visualMapControls.selectedNode || focusedCodeItem || focusedColumnLabel || focusedTableKey);
  const guide = hasData
    ? compositionMode
      ? {
          question: "선택한 대상은 어떤 근거로 이어지나",
          action: "연결을 선택해 근거 확인",
          basis: "확정 관계 우선 · 후보 관계 분리",
        }
      : architectureMode
      ? {
          question: architectureDetail ? "이 구조 영역은 무엇으로 구성됐나" : "먼저 읽을 구조 영역은?",
          action: architectureDetail ? "API → 코드 → DB 순서로 선택" : "구조 영역 선택",
          basis: architectureDetail ? "projection 상세 항목" : "확정 연결도 · API · DB 중요도",
        }
      : impactBoard
        ? {
            question: "무엇을 먼저 확인해야 하나",
            action: "01 → 04 순서로 검토",
            basis: "DB 직접 사실 · 코드 후보 · snapshot gap",
          }
        : apiReading
          ? {
              question: "이 API 요청은 어디까지 이어지나",
              action: "번호 순서대로 파일 읽기",
              basis: "확정 HANDLES · CALLS · 정적 SQL READS/WRITES · 후보 분리",
            }
      : atlasCanvasGuide({
        mode,
        counts: inventoryCounts,
        readOrder,
        relationTotal: visualMapControls.currentMap?.edges.length ?? 0,
        selectedEdge: Boolean(visualMapControls.selectedEdge),
        selectedNode: Boolean(visualMapControls.selectedNode),
        selectedTableNeedsColumns,
        })
    : null;
  const guideAction = guide?.action.includes("보강")
    ? {
        label:
          !dbProfileControls.activeProfile && dbProfileControls.canSaveProfile
            ? "DB 연결 저장"
            : dbProfileControls.canIndexProfile
              ? "다시 읽기"
              : "DB 정보 입력",
        run:
          !dbProfileControls.activeProfile && dbProfileControls.canSaveProfile
            ? dbProfileControls.saveProfile
            : dbProfileControls.canIndexProfile
              ? dbProfileControls.indexProfile
              : () => showWorkbenchDbSetup(openSourceManager, dbProfileControls),
        disabled: dbProfileControls.busy,
      }
    : null;
  const relationBeams = buildRelationBeams({
    map: architectureMode || apiReading || mode === "search-focus" ? null : visualMapControls.currentMap,
    routeCards: displayedRouteCards,
    codeCards: displayedCodeCards,
    tableCards: displayedTableCards,
    bands,
    selectedEdge: visualMapControls.selectedEdge,
    selectedNode: visualMapControls.selectedNode,
    selectedFocusId: selectedRelationFocusId,
  });
  const hasRelationFocus = Boolean(visualMapControls.selectedEdge || visualMapControls.selectedNode || selectedRelationFocusId);
  const showRelationLedger = Boolean(
    hasData &&
      visualMapControls.currentMap &&
      !apiReading &&
      (!architectureMode || architectureDetail || visualMapControls.selectedNode || visualMapControls.selectedEdge),
  );

  return (
    <main className={`canvas at-canvas ${visualMapControls.loading ? "is-refreshing" : ""}`} aria-busy={visualMapControls.loading}>
      {visualMapControls.loading ? (
        <div className="at-update-indicator" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={13} />
          {pendingFocus ? `${pendingFocus.title} 분석 중 · 이전 결과 표시` : "새 보기 준비 중"}
        </div>
      ) : null}
      <div className={`at-canvas-head${apiReading ? " api-reading-head" : ""}`}>
        {apiReading && visualMapControls.currentMap ? (
          <ApiReadingHeader
            answer={apiReading}
            map={visualMapControls.currentMap}
            view={apiReadingView}
            onViewChange={setApiReadingView}
          />
        ) : (
          <>
            <div className="at-title-block">
              <strong>{activeMode}</strong>
              <span>{canvasFacts}</span>
            </div>
            {guide ? (
              <div className="at-guide" aria-label="현재 화면에서 답 찾는 순서">
                <span>
                  <b>찾는 답</b>
                  <i title={guide.question}>{guide.question}</i>
                </span>
                <strong>
                  <b>다음 행동</b>
                  {guideAction ? (
                    <button
                      className="at-guide-action"
                      type="button"
                      onClick={guideAction.run}
                      disabled={guideAction.disabled}
                      aria-label={`${guide.action}: ${guideAction.label}`}
                    >
                      {guideAction.label}
                    </button>
                  ) : (
                    <i title={guide.action}>{guide.action}</i>
                  )}
                </strong>
                <em>
                  <b>근거 범위</b>
                  <i title={guide.basis}>{guide.basis}</i>
                </em>
              </div>
            ) : (
              <>
                <span className="at-legend-item primary">
                  <i className="line blue" /> {readOrder}
                </span>
                <span className="at-legend-item">{modePurpose}</span>
              </>
            )}
          </>
        )}
        {hasData && !apiReading && (
          <div className="at-canvas-controls">
            <button type="button" className="tool" title="화면 원점으로" aria-label="캔버스 화면 원점으로" onClick={resetAtlasView}>
              <Maximize2 size={14} />
            </button>
            <button type="button" className="tool wide" title="배율 초기화" aria-label="캔버스 배율 초기화" onClick={resetAtlasZoom}>
              {Math.round(atlasZoom * 100)}%
            </button>
            <button type="button" className="tool" title="확대" aria-label="캔버스 확대" onClick={() => zoomAtlas(0.12)}>
              <Plus size={14} />
            </button>
            <button type="button" className="tool" title="축소" aria-label="캔버스 축소" onClick={() => zoomAtlas(-0.12)}>
              <Minus size={14} />
            </button>
          </div>
        )}
      </div>
      {compositionMode ? (
        <CompositionToolbar
          visualMapControls={visualMapControls}
          codeInventory={workspaceControls.codeInventory}
        />
      ) : hasData && !needsTarget && !apiReading ? (
        <FocusStrip
          focus={focus}
          onClear={visualMapControls.selectedEdge || visualMapControls.selectedNode ? visualMapControls.clearSelection : null}
        />
      ) : null}

      <div
        className="at-stage"
        ref={stageRef}
        tabIndex={0}
        aria-label="근거 캔버스"
        onPointerDown={startPan}
        onPointerMove={movePan}
        onPointerUp={stopPan}
        onPointerCancel={stopPan}
        onKeyDown={handleStageKeyDown}
        onWheel={handleWheel}
        onScroll={rememberCanvasView}
      >
        {compositionMode && visualMapControls.compositionFocusIds.length < 2 ? (
          <div className="map-empty composition-selection-empty">
            <MousePointer2 size={22} aria-hidden="true" />
            <strong>{visualMapControls.compositionFocusIds.length === 0 ? "분석 대상 2개 필요" : "대상 1개 더 필요"}</strong>
            <span>왼쪽 항목에서 API·코드·테이블·컬럼을 선택하세요.</span>
          </div>
        ) : needsTarget ? (
          <div className="map-empty target-selection-empty">
            <MousePointer2 size={22} aria-hidden="true" />
            <strong>{targetSelectionPrompt(mode).title}</strong>
            <span>{targetSelectionPrompt(mode).description}</span>
          </div>
        ) : !hasData ? (
          <SetupChecklist
            title={emptyTitle}
            openSourceManager={openSourceManager}
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
          />
        ) : (
          <>
            <div
              className={`at-map-surface ${architectureMode ? "at-architecture-surface" : ""} ${impactBoard ? "at-impact-surface" : ""} ${apiReading ? "at-api-reading-surface" : ""} ${hasRelationFocus ? "has-relation-focus" : ""}`}
              style={architectureMode || impactBoard || apiReading || showDisconnectedCodeFocus ? ({ zoom: atlasZoom } as CSSProperties) : mapStyle}
            >
              {impactBoard && visualMapControls.currentMap ? (
                <ImpactReviewBoard
                  board={impactBoard}
                  map={visualMapControls.currentMap}
                  onSelectNode={visualMapControls.selectNode}
                  changeIntent={visualMapControls.changeIntent}
                  onChangeIntent={visualMapControls.setChangeIntent}
                />
              ) : architectureMode && architectureMap ? (
                <ArchitectureMap
                  map={architectureMap}
                  relationCounts={relationCounts}
                  selectedNodeId={visualMapControls.selectedNode?.id ?? null}
                  selectedEdgeId={visualMapControls.selectedEdge?.id ?? null}
                  onBack={() => visualMapControls.showMode("atlas", null)}
                  onOpenGroup={(node) => visualMapControls.showMode("atlas", node.id)}
                  onOpenMember={openArchitectureMember}
                  onSelectEdge={visualMapControls.selectEdge}
                />
              ) : apiReading && visualMapControls.currentMap ? (
                <ApiReadingPath
                  answer={apiReading}
                  map={visualMapControls.currentMap}
                  view={apiReadingView}
                  selectedNodeId={visualMapControls.selectedNode?.id ?? null}
                  selectedEdgeId={visualMapControls.selectedEdge?.id ?? null}
                  dbTables={dbProfileControls.inventory?.tables ?? []}
                  onSelectNode={visualMapControls.selectNode}
                  onSelectEdge={visualMapControls.selectEdge}
                />
              ) : showDisconnectedCodeFocus && relationTargetCodeItem ? (
                <DisconnectedCodeFocus
                  item={relationTargetCodeItem}
                  hiddenNearbyCount={Math.max(0, (visualMapControls.currentMap?.nodes.length ?? 1) - 1)}
                />
              ) : (
              <>
              {relationBeams.length > 0 && (
                <RelationBeams beams={relationBeams} onSelect={visualMapControls.selectEdge} />
              )}
              {showApiBand && (
              <Band num={bandNumber("api")} label="API 라우트" total={routes.length} shown={routeCards.length}>
                {routeCards.map((route) => (
                  <button
                    className={`at-card route ${isSelectedCodeCard(route.id) ? "selected" : ""}${isSelectedEdgeEndpoint(`code:${route.id}`) ? " edge-endpoint" : ""}${isFocusRelatedNode(`code:${route.id}`) ? " focus-related" : ""}`}
                    aria-label={`${route.name} API 선택. 오른쪽에 근거 표시`}
                    aria-pressed={isSelectedCodeCard(route.id)}
                    data-edge-role={edgeEndpointRole(`code:${route.id}`) ?? undefined}
                    key={route.id}
                    type="button"
                    title={`${route.name} · 대상 근거 표시`}
                    onClick={() => selectMappedNode(`code:${route.id}`)}
                  >
                    <div className="at-card-head">
                      <span className="method get">{codeKindChip(route.kind)}</span>
                      <code>{route.name}</code>
                      <RelationBadge summary={relationCounts.get(`code:${route.id}`)} />
                    </div>
                    <div className="at-card-meta">
                      {route.line && <span>L{route.line}</span>}
                      <small title={route.filePath ?? undefined}>{compactPath(route.filePath) ?? "코드 목록"}</small>
                    </div>
                  </button>
                ))}
              </Band>
              )}

              {showCodeBand && (
              <Band num={bandNumber("code")} label={codeBandLabel} total={codeBandItems.length} shown={codeCards.length}>
                {codeCards.map((item) => (
                  <button
                    className={`at-card code ${item.kind.trim().toLowerCase() === "file" ? "file" : ""} ${isSelectedCodeCard(item.id) ? "selected" : ""}${isSelectedEdgeEndpoint(`code:${item.id}`) ? " edge-endpoint" : ""}${isFocusRelatedNode(`code:${item.id}`) ? " focus-related" : ""}`}
                    aria-label={`${item.name} ${codeKindChip(item.kind)} 선택. 오른쪽에 근거 표시`}
                    aria-pressed={isSelectedCodeCard(item.id)}
                    data-edge-role={edgeEndpointRole(`code:${item.id}`) ?? undefined}
                    key={item.id}
                    type="button"
                    title={`${item.name} · 주변 근거`}
                    onClick={() => selectMappedNode(`code:${item.id}`)}
                  >
                    <div className="at-card-head">
                      {item.kind.trim().toLowerCase() === "file" ? <FileText size={14} /> : <Cog size={14} />}
                      <strong>{item.name}</strong>
                      <RelationBadge summary={relationCounts.get(`code:${item.id}`)} />
                    </div>
                    <div className="at-card-meta">
                      <span>{codeKindChip(item.kind)}</span>
                      {item.line && <span>L{item.line}</span>}
                    </div>
                    {item.filePath && <small title={item.filePath}>{compactPath(item.filePath)}</small>}
                  </button>
                ))}
              </Band>
              )}

              {showDbBand && (
              <Band num={bandNumber("db")} label="DB 스키마" total={tables.length} shown={tableCards.length} last>
                {tableCards.map((table) => {
                  const tableKey = dbInventoryTableKey(table);
                  const tableLabel = dbTableIdentityLabel(tableKey);
                  const pinnedColumnNames = columnNamesForTableFromNodeIds(pinnedNodeIds, tableKey);
                  const visibleColumns = takeWithPinned(table.columns, pinnedColumnNames, (column) => column.name, 2);
                  const hiddenColumnCount = Math.max(0, table.columns.length - visibleColumns.length);
                  const needsColumns = table.columns.length === 0;
                  return (
                    <div className="at-table-slot" key={tableKey}>
                      <div
                        className={`at-card table ${needsColumns ? "needs-columns" : ""} ${isSelectedTableCard(tableKey) ? "selected" : ""}${isSelectedEdgeTableEndpoint(tableKey) ? " edge-endpoint" : ""}${isFocusRelatedTable(tableKey) ? " focus-related" : ""}`}
                        data-edge-role={edgeEndpointTableRole(tableKey) ?? undefined}
                      >
                        <button
                          className="at-card-head table-head-button"
                          aria-label={
                            needsColumns
                              ? `${tableLabel} 테이블 선택. 컬럼 대기`
                              : `${tableLabel} 테이블 선택. 오른쪽에 근거 표시`
                          }
                          aria-pressed={isSelectedTableCard(tableKey)}
                          type="button"
                          title={needsColumns ? `${tableLabel} 컬럼을 읽으면 관계가 열립니다` : `${tableLabel} · 대상 근거 표시`}
                          onClick={() => selectMappedNode(dbTableNodeId(tableKey))}
                        >
                          <Table2 size={13} />
                          <strong>{tableLabel}</strong>
                          <RelationBadge summary={relationCounts.get(dbTableNodeId(tableKey))} />
                          <span className="at-table-open-label">테이블</span>
                          <span className={`at-count${needsColumns ? " warn" : ""}`}>
                            {needsColumns ? "대기" : table.columns.length}
                          </span>
                        </button>
                        <div className="at-column-list">
                          {needsColumns && <span className="at-column-empty">컬럼 대기</span>}
                          {visibleColumns.map((column) => (
                            <button
                              className={`at-column-row ${isActiveColumn(tableKey, column.name) ? "active" : ""}`}
                              aria-label={`${tableLabel}.${column.name} 컬럼 선택. 오른쪽에 근거 표시`}
                              aria-pressed={isActiveColumn(tableKey, column.name)}
                              key={column.name}
                              type="button"
                              title={`${tableLabel}.${column.name} · 대상 근거 표시`}
                              onClick={() =>
                                selectMappedNode(dbColumnNodeId(tableKey, column.name))
                              }
                            >
                              <code>{column.name}</code>
                              <em>{columnMeta(column)}</em>
                            </button>
                          ))}
                          {hiddenColumnCount > 0 && (
                            <button
                              className="at-column-more"
                              aria-label={`${tableLabel} 테이블 선택. 숨겨진 컬럼 ${hiddenColumnCount}개 포함`}
                              type="button"
                              title={`${tableLabel} 테이블 선택 · 숨겨진 컬럼 ${hiddenColumnCount}개 포함`}
                              onClick={() => dbProfileControls.openTable(tableKey)}
                            >
                              +{hiddenColumnCount}개 더
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })}
              </Band>
              )}
              </>
              )}
            </div>
            {apiReading ? (
              <div className="api-map-floating-controls" aria-label="연결 지도 배율">
                <button type="button" title="화면 원점으로" aria-label="캔버스 화면 원점으로" onClick={resetAtlasView}>
                  <Maximize2 size={14} />
                </button>
                <button type="button" title="축소" aria-label="캔버스 축소" onClick={() => zoomAtlas(-0.12)}>
                  <Minus size={14} />
                </button>
                <button className="wide" type="button" title="배율 초기화" aria-label="캔버스 배율 초기화" onClick={resetAtlasZoom}>
                  {Math.round(atlasZoom * 100)}%
                </button>
                <button type="button" title="확대" aria-label="캔버스 확대" onClick={() => zoomAtlas(0.12)}>
                  <Plus size={14} />
                </button>
              </div>
            ) : null}

          </>
        )}
      </div>
      {showRelationLedger && visualMapControls.currentMap && (
        <RelationLedger
          rows={relationRows}
          selectedEdgeId={visualMapControls.selectedEdge?.id ?? null}
          selectedNode={visualMapControls.selectedNode}
          hasSelectedTarget={hasSelectedRelationTarget}
          emptyReason={relationEmptyReason}
          total={relationScopedTotal}
          onSelect={visualMapControls.selectEdge}
        />
      )}
    </main>
  );

  function startPan(event: PointerEvent<HTMLDivElement>) {
    const target = event.target instanceof Element ? event.target : null;
    if (event.button !== 0 || target?.closest("button, [role='button']")) {
      return;
    }
    if (visualMapControls.selectedEdge || visualMapControls.selectedNode) {
      visualMapControls.clearSelection();
    }
    const stage = stageRef.current;
    if (!stage) {
      return;
    }
    panRef.current = { x: event.clientX, y: event.clientY, left: stage.scrollLeft, top: stage.scrollTop };
    stage.setPointerCapture(event.pointerId);
    stage.classList.add("panning");
  }

  function movePan(event: PointerEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    const pan = panRef.current;
    if (!stage || !pan) {
      return;
    }
    stage.scrollLeft = pan.left - (event.clientX - pan.x);
    stage.scrollTop = pan.top - (event.clientY - pan.y);
  }

  function stopPan(event: PointerEvent<HTMLDivElement>) {
    if (!panRef.current) {
      return;
    }
    panRef.current = null;
    stageRef.current?.classList.remove("panning");
    try {
      stageRef.current?.releasePointerCapture(event.pointerId);
    } catch {
      // Pointer capture may already be released by the WebView.
    }
  }

  function handleWheel(event: WheelEvent<HTMLDivElement>) {
    const stage = stageRef.current;
    if (!stage) {
      return;
    }
    if (!event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      stage.scrollLeft += event.deltaX;
      stage.scrollTop += event.deltaY;
      return;
    }
    event.preventDefault();
    zoomAtlas(event.deltaY < 0 ? 0.08 : -0.08, { clientX: event.clientX, clientY: event.clientY });
  }

  function handleStageKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "Escape" || (!visualMapControls.selectedEdge && !visualMapControls.selectedNode)) {
      return;
    }
    event.preventDefault();
    visualMapControls.clearSelection();
  }

  function zoomAtlas(delta: number, origin?: { clientX: number; clientY: number }) {
    setAtlasZoom((current) => {
      const next = clamp(current + delta, 0.55, 1.65);
      zoomRef.current = next;
      const stage = stageRef.current;
      if (origin && stage && next !== current) {
        const rect = stage.getBoundingClientRect();
        const x = origin.clientX - rect.left;
        const y = origin.clientY - rect.top;
        const ratio = next / current;
        window.requestAnimationFrame(() => {
          stage.scrollLeft = (stage.scrollLeft + x) * ratio - x;
          stage.scrollTop = (stage.scrollTop + y) * ratio - y;
          rememberCanvasView();
        });
      }
      viewStatesRef.current.set(mode, {
        zoom: next,
        left: stage?.scrollLeft ?? 0,
        top: stage?.scrollTop ?? 0,
      });
      return next;
    });
  }

  function resetAtlasView() {
    zoomRef.current = 1;
    setAtlasZoom(1);
    if (stageRef.current) {
      stageRef.current.scrollLeft = 0;
      stageRef.current.scrollTop = 0;
    }
    viewStatesRef.current.set(mode, { zoom: 1, left: 0, top: 0 });
  }

  function resetAtlasZoom() {
    zoomRef.current = 1;
    setAtlasZoom(1);
    const stage = stageRef.current;
    viewStatesRef.current.set(mode, {
      zoom: 1,
      left: stage?.scrollLeft ?? 0,
      top: stage?.scrollTop ?? 0,
    });
  }

  function rememberCanvasView() {
    const stage = stageRef.current;
    if (!stage) return;
    viewStatesRef.current.set(mode, {
      zoom: zoomRef.current,
      left: stage.scrollLeft,
      top: stage.scrollTop,
    });
  }

  function isSelectedEdgeEndpoint(nodeId: string): boolean {
    const edge = visualMapControls.selectedEdge;
    return Boolean(edge && (edge.from === nodeId || edge.to === nodeId));
  }

  function edgeEndpointRole(nodeId: string): string | null {
    const edge = visualMapControls.selectedEdge;
    if (!edge) {
      return null;
    }
    if (edge.from === nodeId) {
      return "기준";
    }
    return edge.to === nodeId ? "대상" : null;
  }

  function edgeEndpointTableRole(tableKey: string): string | null {
    const edge = visualMapControls.selectedEdge;
    if (!edge) {
      return null;
    }
    if (nodeTouchesTable(edge.from, tableKey)) {
      return "기준";
    }
    return nodeTouchesTable(edge.to, tableKey) ? "대상" : null;
  }

  function isSelectedCodeCard(codeId: string): boolean {
    const nodeId = `code:${codeId}`;
    const node = visualMapControls.selectedNode;
    if (node) {
      return node.id === nodeId;
    }
    if (visualMapControls.selectedEdge) {
      return false;
    }
    return nodeId === selectedRelationFocusId || nodeId === selectedCodeNodeId;
  }

  function isSelectedTableCard(tableKey: string): boolean {
    const node = visualMapControls.selectedNode;
    if (node) {
      return nodeTouchesTable(node.id, tableKey);
    }
    if (visualMapControls.selectedEdge || selectedCodeNodeId) {
      return false;
    }
    const useSelectedTable =
      selectedRelationFocusId?.startsWith("db:") ||
      visualMapControls.mode === "table-usage" ||
      visualMapControls.mode === "column-impact";
    return (selectedRelationFocusId ? nodeTouchesTable(selectedRelationFocusId, tableKey) : false) || (useSelectedTable && tableKey === dbProfileControls.selectedTableKey);
  }

  function isSelectedEdgeTableEndpoint(tableKey: string): boolean {
    const edge = visualMapControls.selectedEdge;
    if (!edge) {
      return false;
    }
    const tableId = dbTableNodeId(tableKey);
    const columnPrefix = `db:column:${tableKey}:`;
    return edge.from === tableId || edge.to === tableId || edge.from.startsWith(columnPrefix) || edge.to.startsWith(columnPrefix);
  }

  function isActiveColumn(tableKey: string, columnName: string): boolean {
    const columnId = dbColumnNodeId(tableKey, columnName);
    const selectedNode = visualMapControls.selectedNode;
    if (selectedNode?.id === columnId) {
      return true;
    }
    const edge = visualMapControls.selectedEdge;
    return selectedRelationFocusId === columnId || Boolean(edge && (edge.from === columnId || edge.to === columnId));
  }

  function isFocusRelatedNode(nodeId: string): boolean {
    const edge = visualMapControls.selectedEdge;
    if (edge) {
      return edge.from === nodeId || edge.to === nodeId;
    }
    const selectedNode = visualMapControls.selectedNode;
    if (!selectedNode) {
      return selectedRelationFocusId ? nodesShareTableOrId(nodeId, selectedRelationFocusId) || hasRelationBetween(nodeId, selectedRelationFocusId) : false;
    }
    if (selectedNode.id === nodeId) {
      return true;
    }
    return Boolean(visualMapControls.currentMap?.edges.some((item) => edgeTouchesNodeId(item, nodeId) && edgeTouchesNode(item, selectedNode)));
  }

  function isFocusRelatedTable(tableKey: string): boolean {
    if (visualMapControls.selectedEdge) {
      return isSelectedEdgeTableEndpoint(tableKey);
    }
    const selectedNode = visualMapControls.selectedNode;
    if (!selectedNode) {
      return selectedRelationFocusId
        ? nodeTouchesTable(selectedRelationFocusId, tableKey) || hasRelationBetween(`db:table:${tableKey}`, selectedRelationFocusId)
        : false;
    }
    if (nodeTouchesTable(selectedNode.id, tableKey)) {
      return true;
    }
    return Boolean(visualMapControls.currentMap?.edges.some((edge) => edgeTouchesTable(edge, tableKey) && edgeTouchesNode(edge, selectedNode)));
  }

  function hasRelationBetween(nodeId: string, focusId: string): boolean {
    return Boolean(visualMapControls.currentMap?.edges.some((edge) => edgeTouchesNodeId(edge, nodeId) && edgeTouchesNodeId(edge, focusId)));
  }

  function openArchitectureMember(node: VisualNode) {
    if (node.layer === "api") {
      visualMapControls.showMode("api-flow", node.id);
      return;
    }
    if (node.source === "db" && node.kind === "table") {
      visualMapControls.showMode("table-usage", node.id);
      return;
    }
    visualMapControls.showMode("search-focus", node.id);
  }

  function selectMappedNode(nodeId: string) {
    const node = visualMapControls.currentMap?.nodes.find((item) => item.id === nodeId) ?? null;
    if (node) {
      visualMapControls.selectNode(node);
      return;
    }
    if (nodeId === analysisFocusId && focusedCodeItem) {
      visualMapControls.selectNode({
        id: nodeId,
        kind: focusedCodeItem.kind,
        title: focusedCodeItem.name,
        subtitle: focusedCodeItem.filePath ?? null,
        layer: "code",
        source: "code",
      });
      return;
    }
    visualMapControls.showMode(mode, nodeId);
  }
}

function DisconnectedCodeFocus({
  item,
  hiddenNearbyCount,
}: {
  item: CodeInventoryItem;
  hiddenNearbyCount: number;
}) {
  const isFile = item.kind.trim().toLowerCase() === "file";
  const source = compactPath(item.filePath) ?? "소스 위치 없음";
  return (
    <section className="at-disconnected-focus" aria-label={`${item.name} 연결 없음`}>
      <div className="at-disconnected-side incoming">
        <span>들어오는 연결</span>
        <strong>0</strong>
        <small>현재 스냅샷에서 확인되지 않음</small>
      </div>
      <article className="at-disconnected-target">
        <header>
          {isFile ? <FileText size={16} /> : <Cog size={16} />}
          <span>{codeKindChip(item.kind)}</span>
        </header>
        <strong title={item.name}>{item.name}</strong>
        <small title={item.filePath ?? undefined}>{source}{item.line ? `:${item.line}` : ""}</small>
      </article>
      <div className="at-disconnected-side outgoing">
        <span>나가는 연결</span>
        <strong>0</strong>
        <small>현재 스냅샷에서 확인되지 않음</small>
      </div>
      <p>
        <Unlink size={15} aria-hidden="true" />
        <span>
          <strong>확인된 직접 관계가 없습니다</strong>
          <small>
            {hiddenNearbyCount > 0
              ? `같은 분석 범위의 ${hiddenNearbyCount.toLocaleString("ko-KR")}개 항목은 관계 근거가 없어 지도에서 분리했습니다.`
              : "오른쪽에서 소스 위치와 다음 확인 항목을 볼 수 있습니다."}
          </small>
        </span>
      </p>
    </section>
  );
}

function architectureCanvasFacts(map: VisualMap | null): string {
  if (!map) {
    return "구조 projection 대기";
  }
  if (map.focus.startsWith("group:")) {
    const members = map.nodes.filter((node) => !node.id.startsWith("group:")).length;
    return `상세 항목 ${members}개 · 관계 ${map.edges.length}개`;
  }
  const groups = map.nodes.filter((node) => node.kind === "group-domain").length;
  return `구조 영역 ${groups}개 · 영역 간 관계 ${map.edges.length}개`;
}

function showWorkbenchDbSetup(openSourceManager: () => void, dbProfileControls: DbProfileControls) {
  openSourceManager();
  window.requestAnimationFrame(() => focusDbProfileSetup(dbProfileControls));
}

function RelationBeams({ beams, onSelect }: { beams: RelationBeam[]; onSelect: (edge: VisualEdge) => void }) {
  return (
    <svg className="at-relation-beams" aria-label="관계선">
      <defs>
        <marker id="at-beam-arrow" markerHeight="6" markerWidth="6" orient="auto" refX="5" refY="3">
          <path d="M0,0 L6,3 L0,6 Z" fill="context-stroke" />
        </marker>
      </defs>
      {beams.map((beam) => (
        <line
          aria-label={beam.label}
          className={`at-beam ${beam.tone} ${beam.active ? "active" : ""}`}
          key={beam.edge.id}
          markerEnd="url(#at-beam-arrow)"
          role="button"
          tabIndex={0}
          x1={beam.x1}
          x2={beam.x2}
          y1={`${beam.y1}%`}
          y2={`${beam.y2}%`}
          onClick={() => onSelect(beam.edge)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onSelect(beam.edge);
            }
          }}
        />
      ))}
    </svg>
  );
}

function RelationLedger({
  rows,
  selectedEdgeId,
  selectedNode,
  hasSelectedTarget,
  emptyReason,
  total,
  onSelect,
}: {
  rows: RelationLedgerRow[];
  selectedEdgeId: string | null;
  selectedNode: VisualNode | null;
  hasSelectedTarget: boolean;
  emptyReason?: string;
  total: number;
  onSelect: (edge: VisualEdge) => void;
}) {
  const title = selectedEdgeId ? "선택한 관계" : hasSelectedTarget ? "먼저 볼 관계" : "관계 우선순위";
  const hint = selectedEdgeId
    ? "근거와 양끝 항목 확인"
    : hasSelectedTarget
      ? "직접/구조 우선 · 후보 검증"
      : "직접/구조 우선 · 후보/이름 단서 검증";
  const hintTitle = "직접=읽은 근거, 구조=DB/FK/호출 구조, 후보/이름 단서=검증 필요";
  const emptyText = emptyReason ?? (hasSelectedTarget ? "이 대상과 연결된 관계가 없습니다." : "아직 표시할 관계가 없습니다.");
  const emptyNext = relationEmptyNextStep(emptyReason, hasSelectedTarget);
  const hidden = Math.max(0, total - rows.length);
  const countText = hidden > 0 ? `${rows.length}개 표시 · +${hidden}` : `${rows.length}개 전체`;

  return (
    <div className="at-edge-ledger" aria-label={title}>
      <div className="at-edge-ledger-head">
        <strong>{title}</strong>
        <span title={hintTitle}>{hint}</span>
        <em title={`${rows.length}개 표시 / 전체 ${total}개`}>{countText}</em>
      </div>
      {rows.length > 0 && (
        <div className="at-edge-columns" aria-hidden="true">
          <span>관계</span>
          <span>기준</span>
          <span />
          <span>연결 대상</span>
          <span>왜 연결됐나</span>
          <span>판단</span>
        </div>
      )}
      {rows.map((row) => (
        <button
          className={`at-edge-row ${row.tone} ${row.edge.id === selectedEdgeId ? "selected" : ""}${edgeTouchesNode(row.edge, selectedNode) ? " node-related" : ""}`}
          aria-pressed={row.edge.id === selectedEdgeId}
          key={row.edge.id}
          type="button"
          aria-label={`${row.label} 관계. 기준: ${row.fromTitle}. 연결 대상: ${row.toTitle}. 판단: ${relationLedgerAction(row.tone)}. 근거: ${row.evidence}`}
          title={`${row.label} · ${row.fromTitle} → ${row.toTitle} · ${relationLedgerAction(row.tone)} · ${row.evidence}`}
          onClick={() => onSelect(row.edge)}
        >
          <span className="at-edge-tone">{row.label}</span>
          <code data-label="기준" title={row.fromTitle}>{row.from}</code>
          <i aria-hidden="true" />
          <code data-label="연결 대상" title={row.toTitle}>{row.to}</code>
          <small>{row.evidence}</small>
          <b className="at-edge-action">{row.edge.id === selectedEdgeId ? "선택됨" : relationLedgerAction(row.tone)}</b>
        </button>
      ))}
      {rows.length === 0 && (
        <span className="at-edge-empty">
          <b>{emptyText}</b>
          <small>{emptyNext}</small>
        </span>
      )}
    </div>
  );
}

function relationLedgerAction(tone: RelationTone): string {
  return RELATION_ACTION_LABEL[tone];
}

function relationEmptyNextStep(emptyReason: string | undefined, hasSelectedTarget: boolean): string {
  if (emptyReason?.includes("컬럼 구조") || emptyReason?.includes("컬럼을 읽으면")) {
    return "DB 카드에서 컬럼을 보강하면 FK와 영향 근거를 확인할 수 있습니다.";
  }
  if (hasSelectedTarget) {
    return "다른 카드나 상단 검색으로 범위를 넓혀 주변 관계를 다시 확인하세요.";
  }
  return "카드를 선택하거나 상단 검색으로 API, 코드, 테이블, 컬럼을 먼저 좁히세요.";
}

function CanvasTransitionState({
  mode,
  focusId,
  codeInventory,
  dbProfileControls,
}: {
  mode: string;
  focusId: string | null;
  codeInventory: CodeInventory | null;
  dbProfileControls: DbProfileControls;
}) {
  const descriptor = transitionDescriptor(mode);
  const focus = transitionFocusState(mode, focusId, codeInventory, dbProfileControls);
  const apiMode = mode === "api-flow";

  return (
    <main className="canvas at-canvas is-transitioning" aria-busy="true">
      <div className={`at-canvas-head${apiMode ? " api-reading-head" : ""}`}>
        <div className="at-title-block">
          <strong>{descriptor.title}</strong>
          <span>{descriptor.purpose}</span>
        </div>
        <div className="at-transition-progress" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={13} />
          새 근거 구성 중
        </div>
        {!apiMode ? (
          <div className="at-canvas-controls" aria-hidden="true">
            <button className="tool" type="button" disabled><Maximize2 size={14} /></button>
            <button className="tool wide" type="button" disabled>100%</button>
            <button className="tool" type="button" disabled><Plus size={14} /></button>
            <button className="tool" type="button" disabled><Minus size={14} /></button>
          </div>
        ) : null}
      </div>
      {!apiMode ? <FocusStrip focus={focus} onClear={null} /> : null}
      <div className="at-stage">
        <div className={`at-transition-map mode-${mode}`} aria-label={`${descriptor.title} 로딩 상태`}>
          {descriptor.lanes.map((lane, index) => (
            <section className="at-transition-lane" key={lane}>
              <header>
                <span>{String(index + 1).padStart(2, "0")}</span>
                <strong>{lane}</strong>
              </header>
              <div className="at-transition-card" aria-hidden="true">
                <i />
                <b />
                <small />
              </div>
              {index < descriptor.detailLanes ? (
                <div className="at-transition-card compact" aria-hidden="true">
                  <i />
                  <b />
                  <small />
                </div>
              ) : null}
            </section>
          ))}
        </div>
      </div>
    </main>
  );
}

function transitionDescriptor(mode: string): {
  title: string;
  purpose: string;
  lanes: string[];
  detailLanes: number;
} {
  if (mode === "api-flow") {
    return {
      title: "API 읽기 경로",
      purpose: "선택한 라우트의 확정 연결만 다시 구성합니다",
      lanes: ["Route", "Handler", "Service / Function", "Repository / Query", "DB 후보"],
      detailLanes: 3,
    };
  }
  if (mode === "table-usage") {
    return {
      title: "테이블 사용처",
      purpose: "테이블과 연결된 확정 근거와 후보를 분리합니다",
      lanes: ["직접 사용", "코드 후보", "확인 필요", "권장 확인"],
      detailLanes: 2,
    };
  }
  if (mode === "column-impact") {
    return {
      title: "컬럼 변경 영향",
      purpose: "변경 전 확인할 범위를 근거 수준별로 다시 계산합니다",
      lanes: ["직접 영향", "간접 후보", "확인 필요", "권장 확인"],
      detailLanes: 2,
    };
  }
  if (mode === "search-focus") {
    return {
      title: "코드 연결",
      purpose: "선택한 코드의 주변 호출과 데이터 후보를 좁힙니다",
      lanes: ["현재 코드", "직접 연결", "데이터 후보"],
      detailLanes: 2,
    };
  }
  return {
    title: "전체 구조",
    purpose: "프로젝트의 경계와 확정 연결을 같은 기준으로 정렬합니다",
    lanes: ["API 경계", "코드 영역", "DB 스키마"],
    detailLanes: 3,
  };
}

function targetSelectionPrompt(mode: string): { title: string; description: string } {
  if (mode === "api-flow") {
    return { title: "확인할 API 라우트를 선택하세요", description: "왼쪽 API 라우트 목록에서 요청 경로를 선택하면 연결 근거를 표시합니다." };
  }
  if (mode === "table-usage") {
    return { title: "사용처를 확인할 테이블을 선택하세요", description: "왼쪽 DB 테이블 목록에서 대상을 선택하면 직접 사용과 후보를 나눠 표시합니다." };
  }
  if (mode === "column-impact") {
    return { title: "영향을 확인할 컬럼을 선택하세요", description: "왼쪽 컬럼 목록에서 변경 대상을 선택하면 확인 범위를 계산합니다." };
  }
  return { title: "확인할 코드 항목을 선택하세요", description: "왼쪽 코드 목록에서 함수, 클래스 또는 파일을 선택하면 주변 근거를 표시합니다." };
}

function transitionFocusState(
  mode: string,
  focusId: string | null,
  codeInventory: CodeInventory | null,
  dbProfileControls: DbProfileControls,
): FocusStripState {
  const codeItem = focusId ? codeInventoryItemFromNodeId(codeInventory, focusId) : null;
  if (codeItem) {
    return {
      label: mode === "api-flow" ? "API 기준" : "코드 기준",
      title: codeItem.name,
      meta: compactPath(codeItem.filePath) ?? codeItem.kind,
      hint: "새 화면에서도 이 대상을 기준으로 관계를 표시합니다.",
      tone: "code",
    };
  }

  const column = focusId ? columnRefFromNodeId(focusId) : null;
  if (column) {
    return {
      label: "변경 기준",
      title: `${dbTableIdentityLabel(column.tableKey)}.${column.columnName}`,
      meta: "컬럼 영향",
      hint: "직접 영향과 검증할 후보를 분리해 표시합니다.",
      tone: "db",
    };
  }

  const tableKey = focusId ? tableKeyFromNodeId(focusId) : null;
  if (tableKey) {
    const table = dbProfileControls.inventory?.tables.find((item) => dbInventoryTableKey(item) === tableKey) ?? null;
    return {
      label: "DB 기준",
      title: dbTableIdentityLabel(tableKey),
      meta: table ? `컬럼 ${table.columns.length.toLocaleString("ko-KR")}개` : "테이블 사용처",
      hint: "이 테이블과 연결된 근거만 다시 구성합니다.",
      tone: "db",
    };
  }

  if (focusId?.startsWith("group:")) {
    return {
      label: "구조 기준",
      title: "선택한 구조 영역",
      meta: "영역 상세",
      hint: "영역 안의 API, 코드, DB 항목을 펼칩니다.",
      tone: "neutral",
    };
  }

  return {
    label: "현재 기준",
    title: mode === "atlas" ? "전체 프로젝트" : "선택한 대상",
    meta: "근거 재구성",
    hint: "새 화면과 일치하는 정보만 표시합니다.",
    tone: "neutral",
  };
}


function CompositionToolbar({
  visualMapControls,
  codeInventory,
}: {
  visualMapControls: VisualMapControls;
  codeInventory: CodeInventory | null;
}) {
  const views = [
    ["connections", "전체 연결"],
    ["calls", "호출"],
    ["data", "데이터"],
    ["impact", "영향"],
  ] as const;

  return (
    <section className="composition-toolbar" aria-label="관계 분석 범위">
      <header>
        <strong>대상</strong>
        <span>{visualMapControls.compositionFocusIds.length}/8</span>
      </header>
      <div className="composition-targets">
        {visualMapControls.compositionFocusIds.length > 0 ? (
          visualMapControls.compositionFocusIds.map((id) => (
            <button
              type="button"
              title={`${compositionSelectionLabel(id, codeInventory, visualMapControls.currentMap)} 선택 해제`}
              aria-label={`${compositionSelectionLabel(id, codeInventory, visualMapControls.currentMap)} 선택 해제`}
              onClick={() => visualMapControls.toggleCompositionFocus(id)}
              key={id}
            >
              <span>{compositionSelectionLabel(id, codeInventory, visualMapControls.currentMap)}</span>
              <X size={12} />
            </button>
          ))
        ) : (
          <span className="composition-target-placeholder">선택 대기</span>
        )}
      </div>
      <div className="composition-view-switch" role="group" aria-label="관계 보기 방식">
        {views.map(([id, label]) => (
          <button
            className={visualMapControls.relationView === id ? "active" : ""}
            type="button"
            aria-pressed={visualMapControls.relationView === id}
            onClick={() => visualMapControls.setRelationView(id)}
            key={id}
          >
            {label}
          </button>
        ))}
      </div>
      <button
        className="composition-clear"
        type="button"
        title="분석 대상 전체 해제"
        aria-label="분석 대상 전체 해제"
        disabled={visualMapControls.compositionFocusIds.length === 0}
        onClick={visualMapControls.clearCompositionFocus}
      >
        <X size={14} />
      </button>
    </section>
  );
}

function compositionSelectionLabel(
  nodeId: string,
  codeInventory: CodeInventory | null,
  map: VisualMap | null,
): string {
  const column = columnRefFromNodeId(nodeId);
  if (column) return `${dbTableIdentityLabel(column.tableKey)}.${column.columnName}`;
  const tableKey = tableKeyFromNodeId(nodeId);
  if (tableKey) return dbTableIdentityLabel(tableKey);
  const codeItem = codeInventoryItemFromNodeId(codeInventory, nodeId);
  if (codeItem) return codeItem.name;
  const mapNode = map?.nodes.find((node) => node.id === nodeId);
  if (mapNode) return mapNode.title;
  const parts = nodeId.split(":");
  return parts[parts.length - 1] || nodeId;
}

function FocusStrip({ focus, onClear }: { focus: FocusStripState; onClear: (() => void) | null }) {
  return (
    <div className={`at-focus-strip ${focus.tone}`}>
      <span>{focus.label}</span>
      <strong title={focus.title}>{focus.title}</strong>
      <em>{focus.meta}</em>
      <small>
        <b>다음 행동</b>
        <i>{focus.hint}</i>
      </small>
      {onClear && (
        <button className="at-focus-clear" type="button" title="선택 해제" aria-label="선택 해제" onClick={onClear}>
          <X size={13} />
        </button>
      )}
    </div>
  );
}

function architectureFocusState(visualMapControls: VisualMapControls): FocusStripState {
  if (visualMapControls.selectedEdge) {
    return focusFromEdge(visualMapControls.selectedEdge, visualMapControls.currentMap);
  }
  if (visualMapControls.selectedNode) {
    return focusFromNode(visualMapControls.selectedNode, visualMapControls.currentMap);
  }
  return {
    label: "구조",
    title: "구조 영역 선택",
    meta: "중요도와 확정 연결도 순",
    hint: "카드를 열어 API → 코드 → DB 순서로 읽습니다.",
    tone: "neutral",
  };
}

function atlasFocusState(
  focusedCodeItem: CodeInventoryItem | null,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
  tables: NonNullable<DbProfileControls["inventory"]>["tables"],
): FocusStripState {
  if (visualMapControls.selectedEdge) {
    return focusFromEdge(visualMapControls.selectedEdge, visualMapControls.currentMap);
  }
  if (visualMapControls.selectedNode) {
    const tableKey = tableKeyFromNodeId(visualMapControls.selectedNode.id);
    const selectedTable = tableKey ? tables.find((item) => dbInventoryTableKey(item) === tableKey) : null;
    if (visualMapControls.selectedNode.kind === "table" && selectedTable?.columns.length === 0) {
      return {
        label: "테이블",
        title: nodeLabel(visualMapControls.selectedNode.id, visualMapControls.currentMap),
        meta: "컬럼 대기",
        hint: "컬럼을 읽으면 제약과 관계가 열립니다.",
        tone: "db",
      };
    }
    return focusFromNode(visualMapControls.selectedNode, visualMapControls.currentMap);
  }
  if (focusedCodeItem) {
    const item = focusedCodeItem;
    return {
      label: codeKindChip(item.kind),
      title: item.name,
      meta: item.line ? `라인 ${item.line}` : item.filePath ?? "코드 목록",
      hint: "오른쪽 대상 근거에서 호출과 후보를 확인합니다.",
      tone: "code",
    };
  }
  const focusedColumn = columnRefFromNodeId(visualMapControls.currentMap?.focus ?? "");
  if (focusedColumn) {
    const table = tables.find((item) => dbInventoryTableKey(item) === focusedColumn.tableKey) ?? null;
    const column = table?.columns.find((item) => item.name === focusedColumn.columnName) ?? null;
    return {
      label: "컬럼",
      title: `${dbTableIdentityLabel(focusedColumn.tableKey)}.${focusedColumn.columnName}`,
      meta: column ? columnMeta(column) || column.dataType || "컬럼" : "컬럼",
      hint: "오른쪽 대상 근거에서 타입과 키를 확인합니다.",
      tone: "db",
    };
  }
  const mapFocus = visualMapControls.currentMap?.focus ?? "";
  const useSelectedTableFocus =
    mapFocus.startsWith("db:") || visualMapControls.mode === "table-usage" || visualMapControls.mode === "column-impact";
  const table = useSelectedTableFocus
    ? tables.find((item) => dbInventoryTableKey(item) === dbProfileControls.selectedTableKey) ?? null
    : null;
  if (table) {
    if (table.columns.length === 0) {
      return {
        label: "DB",
        title: table.schema ? `${table.schema}.${table.name}` : table.name,
        meta: "컬럼 대기",
        hint: "컬럼을 읽으면 제약과 관계가 열립니다.",
        tone: "db",
      };
    }
    const fkCount = table.columns.filter((column) => column.isForeignKey).length;
    return {
      label: "DB",
      title: table.schema ? `${table.schema}.${table.name}` : table.name,
      meta: `컬럼 ${table.columns.length}개 · FK ${fkCount}개`,
      hint: "오른쪽 대상 근거에서 제약과 관계를 확인합니다.",
      tone: "db",
    };
  }
  return {
    label: "대상",
    title: "대상 선택",
    meta: "카드 또는 검색 결과를 선택하세요",
    hint: "선택하면 오른쪽에 근거 요약이 열립니다.",
    tone: "neutral",
  };
}

function codeInventoryItemFromNodeId(inventory: CodeInventory | null, nodeId: string): CodeInventoryItem | null {
  if (!inventory || !nodeId.startsWith("code:")) {
    return null;
  }
  const id = nodeId.slice("code:".length);
  return (
    inventory.routes.find((item) => item.id === id) ??
    codeInventoryCodeItems(inventory).find((item) => item.id === id) ??
    inventory.files.find((item) => item.id === id) ??
    null
  );
}

function includeFocusedCodeItem(
  items: CodeInventoryItem[],
  focusedItem: CodeInventoryItem | null,
): CodeInventoryItem[] {
  if (!focusedItem || items.some((item) => item.id === focusedItem.id)) {
    return items;
  }
  return [focusedItem, ...items];
}

function focusFromEdge(edge: VisualEdge, map: VisualMap | null): FocusStripState {
  const hasEvidence = edge.evidence.length > 0;
  return {
    label: edgeKindLabel(edge),
    title: `${nodeLabel(edge.from, map)} → ${nodeLabel(edge.to, map)}`,
    meta: edge.evidence[0]?.text ?? edgeKindLabel(edge),
    hint: edge.kind.startsWith("candidate")
      ? hasEvidence
        ? "후보는 근거를 먼저 확인하세요."
        : "후보 구조와 양끝 항목을 확인하세요."
      : hasEvidence
        ? "직접 근거를 확인하세요."
        : "관계 구조와 양끝 항목을 확인하세요.",
    tone: "edge",
  };
}

function focusFromNode(node: VisualNode, map: VisualMap | null): FocusStripState {
  const edges = map?.edges.filter((edge) => edgeTouchesNode(edge, node)) ?? [];
  const candidates = edges.filter((edge) => edge.kind.startsWith("candidate")).length;
  return {
    label: nodeKindLabel(node.kind, node.source),
    title: nodeLabel(node.id, map),
    meta: `관계 ${edges.length}개 · 후보 ${candidates}개`,
    hint: node.kind === "column" ? "컬럼 근거는 직접/후보로 나눠 봅니다." : "오른쪽 대상 근거에서 관계를 확인합니다.",
    tone: node.source === "db" ? "db" : node.source === "code" ? "code" : "neutral",
  };
}




function Band({
  num,
  label,
  total,
  shown,
  last,
  children,
}: {
  num: string;
  label: string;
  total: number;
  shown: number;
  last?: boolean;
  children: ReactNode;
}) {
  const hidden = Math.max(0, total - shown);
  const countText = hidden > 0 ? `${shown}개 표시 · +${hidden}` : `${shown}개 전체`;
  return (
    <section className={`at-band ${last ? "last" : ""}`}>
      <div className="at-gutter">
        <span className="at-num">{num}</span>
        <h3>{label}</h3>
        <small title={`${shown}개 표시 / 전체 ${total}개`}>{countText}</small>
      </div>
      <div className="at-cards">{children}</div>
    </section>
  );
}
