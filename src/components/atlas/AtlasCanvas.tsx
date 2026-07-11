import { CheckCircle2, ClipboardCopy, Cog, Database, FileText, FolderOpen, Layers3, Maximize2, Minus, Plus, Table2, X } from "lucide-react";
import { useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, PointerEvent, ReactNode, WheelEvent } from "react";
import { tauriUnavailableMessage } from "../../app/tauriRuntime";
import { codeInventoryCodeItems, codeInventoryItemCount, codeKindChip, dbInventoryTableKey } from "../../types/workspace";
import { dbProfileWorkStarted, type DbProfileControls, type VisualMapControls, type WorkspaceControls } from "../../types/controls";
import type { CodeInventoryItem, DbInventoryTable } from "../../types/workspace";
import type { ApiReadingAnswer, ApiReadingStep, ImpactReviewBoard as ImpactReviewBoardModel, ImpactReviewItem, VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { copyValue } from "../common/copyValue";
import { focusDbProfileSetup, focusSourceSetup, focusWorkspaceSetup } from "../common/focusSourceSetup";
import type { View } from "../common/ViewSwitch";

type FocusStripState = {
  label: string;
  title: string;
  meta: string;
  hint: string;
  tone: "code" | "db" | "edge" | "neutral";
};

type RelationSummary = {
  confirmed: number;
  typed: number;
  inferred: number;
  candidate: number;
};

type RelationTone = "confirmed" | "typed" | "candidate" | "inferred";

const RELATION_ACTION_LABEL: Record<RelationTone, string> = {
  confirmed: "1차 근거",
  typed: "구조 근거",
  candidate: "검증 필요",
  inferred: "이름 단서",
};

type RelationLedgerRow = {
  edge: VisualEdge;
  from: string;
  fromTitle: string;
  to: string;
  toTitle: string;
  label: string;
  tone: RelationTone;
  evidence: string;
};

type RelationBeam = {
  edge: VisualEdge;
  x1: number;
  x2: number;
  y1: number;
  y2: number;
  tone: RelationTone;
  active: boolean;
  label: string;
};

type CanvasGuide = {
  question: string;
  action: string;
  basis: string;
};

type AtlasInventoryCounts = {
  routes: number;
  code: number;
  files: number;
  tables: number;
  columns: number;
};

const AT_GUTTER_WIDTH = 88;
const AT_LANE_WIDTH = 144;
const AT_LANE_GAP = 8;
const AT_LANE_PAD_X = 6;

export function AtlasCanvas({
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const mode = visualMapControls.mode;
  const architectureMode = mode === "atlas" || mode === "explore";
  const architectureMap =
    architectureMode && visualMapControls.currentMap && ["atlas", "explore"].includes(visualMapControls.currentMap.mode)
      ? visualMapControls.currentMap
      : null;
  const impactBoard =
    !architectureMode && ["table-usage", "column-impact"].includes(mode)
      ? visualMapControls.currentMap?.reviewBoard ?? null
      : null;
  const apiReading = mode === "api-flow" ? visualMapControls.currentMap?.apiReading ?? null : null;
  const projectionOnlyMode = architectureMode || Boolean(impactBoard) || Boolean(apiReading);
  const architectureDetail = Boolean(architectureMap?.focus.startsWith("group:"));
  const stageRef = useRef<HTMLDivElement | null>(null);
  const panRef = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const [atlasZoom, setAtlasZoom] = useState(1);
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
  const hasData = architectureMode ? Boolean(architectureMap?.nodes.length) : impactBoard || apiReading ? true : hasInventoryData;
  const activeMode = hasData
    ? architectureMode
      ? architectureDetail
        ? "도메인 상세"
        : "전체 구조"
      : impactBoard
        ? impactBoard.scope === "column" ? "컬럼 변경 영향" : "테이블 변경 영향"
        : apiReading
          ? "API 읽기 경로"
        : atlasModeTitle(mode, inventoryCounts)
    : workspaceControls.currentWorkspace
      ? "코드/DB 연결"
      : "프로젝트 연결";
  const readOrder = hasData
    ? architectureMode
      ? "도메인 → API → 코드 → DB"
      : impactBoard
        ? "직접 영향 → 코드 후보 → 확인 필요 → 권장 확인"
        : apiReading
          ? "Route → Handler → Service/Function → Repository/Query → DB 후보"
        : atlasReadOrder(mode, inventoryCounts)
    : "프로젝트 → 코드 → DB";
  const modePurpose = hasData
    ? architectureMode
      ? architectureDetail
        ? "선택한 도메인의 실제 항목만 펼쳤습니다"
        : "연결도와 중요도가 높은 도메인부터 읽습니다"
      : impactBoard
        ? "수정 전에 확정 사실과 검증할 후보를 순서대로 읽습니다"
        : apiReading
          ? "확정 HANDLES/CALLS만 읽기 경로로 사용합니다"
        : atlasModePurpose(mode, inventoryCounts)
    : workspaceControls.currentWorkspace
      ? "코드/DB 목록을 불러오면 캔버스가 채워집니다"
      : "프로젝트를 열면 캔버스가 채워집니다";
  const focusedNodeIds = new Set(visualMapControls.currentMap?.nodes.map((node) => node.id) ?? []);
  const shouldFocusCards = !impactBoard && mode !== "atlas" && focusedNodeIds.size > 0;
  const visibleRoutes = shouldFocusCards ? filterCodeItemsByMap(routes, focusedNodeIds) : routes;
  const codeBandItems = shouldFocusCards || codeItems.length === 0 ? [...codeItems, ...fileItems] : codeItems;
  const visibleCodeItems = shouldFocusCards ? filterCodeItemsByMap(codeBandItems, focusedNodeIds) : codeBandItems;
  const orderedCodeItems = [...visibleCodeItems].sort((a, b) => atlasCodeKindRank(a.kind) - atlasCodeKindRank(b.kind));
  const visibleTables = shouldFocusCards ? filterTablesByMap(tables, focusedNodeIds) : tables;
  const relationCounts = buildRelationCounts(visualMapControls.currentMap);
  const selectedCodeNodeId = workspaceControls.selectedCodeItem ? `code:${workspaceControls.selectedCodeItem.id}` : null;
  const selectedTableNodeId = dbProfileControls.selectedTableKey ? `db:table:${dbProfileControls.selectedTableKey}` : null;
  const currentMapFocus = visualMapControls.currentMap?.focus ?? "";
  const selectedRelationFocusId = visualMapControls.selectedNode?.id ?? relationFocusIdFromMapFocus(currentMapFocus);
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
      ? architectureMode
      ? architectureCanvasFacts(architectureMap)
      : apiReading
        ? `읽기 ${apiReading.steps.length}단계 · DB 후보 ${apiReading.dbCandidates.length}개${
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
            meta: `확정 읽기 ${Math.max(0, apiReading.steps.length - 1)}개 · DB 후보 ${apiReading.dbCandidates.length}개`,
            hint: apiReading.unknowns[0]?.detail ?? "번호 순서대로 파일을 읽습니다.",
            tone: "code" as const,
          }
      : atlasFocusState(workspaceControls, dbProfileControls, visualMapControls, tables);
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
  const focusedColumnLabel = columnLabelFromNodeId(currentMapFocus);
  const focusedTableKey = tableKeyFromFocusedTable(currentMapFocus);
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
        : workspaceControls.selectedCodeItem
          ? `${workspaceControls.selectedCodeItem.name}와 연결된 관계가 없습니다.`
          : focusedTableKey
            ? `${focusedTableKey} 테이블과 연결된 관계가 없습니다.`
            : undefined;
  const hasSelectedRelationTarget = architectureMode
    ? Boolean(visualMapControls.selectedNode)
    : Boolean(visualMapControls.selectedNode || workspaceControls.selectedCodeItem || focusedColumnLabel || focusedTableKey);
  const guide = hasData
    ? architectureMode
      ? {
          question: architectureDetail ? "이 도메인은 무엇으로 구성됐나" : "먼저 읽을 도메인은?",
          action: architectureDetail ? "API → 코드 → DB 순서로 선택" : "도메인 카드 선택",
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
              basis: "확정 HANDLES · 확정 CALLS · 분리된 DB 후보",
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
              : () => showWorkbenchDbSetup(setView, dbProfileControls),
        disabled: dbProfileControls.busy,
      }
    : null;
  const relationBeams = buildRelationBeams({
    map: architectureMode || apiReading ? null : visualMapControls.currentMap,
    routeCards: displayedRouteCards,
    codeCards: displayedCodeCards,
    tableCards: displayedTableCards,
    bands,
    selectedEdge: visualMapControls.selectedEdge,
    selectedNode: visualMapControls.selectedNode,
    selectedFocusId: selectedRelationFocusId,
  });
  const hasRelationFocus = Boolean(visualMapControls.selectedEdge || visualMapControls.selectedNode || selectedRelationFocusId);

  return (
    <main className="canvas at-canvas">
      <div className="at-canvas-head">
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
        {hasData && (
          <div className={`at-canvas-controls${apiReading ? " api-reading-controls" : ""}`}>
            <button type="button" className="tool" title="화면 원점으로" aria-label="캔버스 화면 원점으로" onClick={resetAtlasView}>
              <Maximize2 size={14} />
            </button>
            <button type="button" className="tool wide" title="배율 초기화" aria-label="캔버스 배율 초기화" onClick={() => setAtlasZoom(1)}>
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
      {hasData && (
        <FocusStrip
          focus={focus}
          onClear={visualMapControls.selectedEdge || visualMapControls.selectedNode ? visualMapControls.clearSelection : null}
        />
      )}

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
      >
        {!hasData ? (
          <SetupChecklist
            title={emptyTitle}
            setView={setView}
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
          />
        ) : (
          <>
            <div
              className={`at-map-surface ${architectureMode ? "at-architecture-surface" : ""} ${impactBoard ? "at-impact-surface" : ""} ${apiReading ? "at-api-reading-surface" : ""} ${hasRelationFocus ? "has-relation-focus" : ""}`}
              style={architectureMode || impactBoard || apiReading ? ({ zoom: atlasZoom } as CSSProperties) : mapStyle}
            >
              {impactBoard && visualMapControls.currentMap ? (
                <ImpactReviewBoard
                  board={impactBoard}
                  map={visualMapControls.currentMap}
                  onSelectNode={visualMapControls.selectNode}
                />
              ) : architectureMode && architectureMap ? (
                <ArchitectureMap
                  map={architectureMap}
                  relationCounts={relationCounts}
                  selectedNodeId={visualMapControls.selectedNode?.id ?? null}
                  onBack={() => visualMapControls.showMode("atlas", null)}
                  onOpenGroup={(node) => visualMapControls.showMode("atlas", node.id)}
                  onOpenMember={openArchitectureMember}
                />
              ) : apiReading && visualMapControls.currentMap ? (
                <ApiReadingPath
                  answer={apiReading}
                  map={visualMapControls.currentMap}
                  onSelectNode={visualMapControls.selectNode}
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
                    aria-label={`${route.name} API 선택. 닿는 코드 열기`}
                    aria-pressed={isSelectedCodeCard(route.id)}
                    data-edge-role={edgeEndpointRole(`code:${route.id}`) ?? undefined}
                    key={route.id}
                    type="button"
                    title={`${route.name} · API가 닿는 코드 열기`}
                    onClick={() => selectCodeCard(route)}
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
                    aria-label={`${item.name} ${codeKindChip(item.kind)} 선택. 주변 근거`}
                    aria-pressed={isSelectedCodeCard(item.id)}
                    data-edge-role={edgeEndpointRole(`code:${item.id}`) ?? undefined}
                    key={item.id}
                    type="button"
                    title={`${item.name} · 주변 근거`}
                    onClick={() => selectCodeCard(item)}
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
                  const tableLabel = tableKey;
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
                              : `${tableLabel} 테이블 선택. 테이블 연결 열기`
                          }
                          aria-pressed={isSelectedTableCard(tableKey)}
                          type="button"
                          title={needsColumns ? `${tableLabel} 컬럼을 읽으면 관계가 열립니다` : `${tableLabel} 테이블 선택 · 테이블 연결 열기`}
                          onClick={() => selectTableCard(tableKey)}
                        >
                          <Table2 size={13} />
                          <strong>{tableLabel}</strong>
                          <RelationBadge summary={relationCounts.get(`db:table:${tableKey}`)} />
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
                              aria-label={`${tableLabel}.${column.name} 컬럼 선택. 변경 범위 열기`}
                              aria-pressed={isActiveColumn(tableKey, column.name)}
                              key={column.name}
                              type="button"
                              title={`${tableLabel}.${column.name} 컬럼 선택 · 변경 범위 열기`}
                              onClick={() => selectColumnCard(tableKey, column.name)}
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
                              onClick={() => selectTableCard(tableKey)}
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

          </>
        )}
      </div>
      {hasData && visualMapControls.currentMap && (
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
      const stage = stageRef.current;
      if (origin && stage && next !== current) {
        const rect = stage.getBoundingClientRect();
        const x = origin.clientX - rect.left;
        const y = origin.clientY - rect.top;
        const ratio = next / current;
        window.requestAnimationFrame(() => {
          stage.scrollLeft = (stage.scrollLeft + x) * ratio - x;
          stage.scrollTop = (stage.scrollTop + y) * ratio - y;
        });
      }
      return next;
    });
  }

  function resetAtlasView() {
    setAtlasZoom(1);
    if (stageRef.current) {
      stageRef.current.scrollLeft = 0;
      stageRef.current.scrollTop = 0;
    }
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
    return nodeId === selectedRelationFocusId || codeId === workspaceControls.selectedCodeItem?.id;
  }

  function isSelectedTableCard(tableKey: string): boolean {
    const node = visualMapControls.selectedNode;
    if (node) {
      return nodeTouchesTable(node.id, tableKey);
    }
    if (visualMapControls.selectedEdge || workspaceControls.selectedCodeItem) {
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
    const tableId = `db:table:${tableKey}`;
    const columnPrefix = `db:column:${tableKey}:`;
    return edge.from === tableId || edge.to === tableId || edge.from.startsWith(columnPrefix) || edge.to.startsWith(columnPrefix);
  }

  function isActiveColumn(tableKey: string, columnName: string): boolean {
    const columnId = `db:column:${tableKey}:${columnName}`;
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

  function selectCodeCard(item: CodeInventoryItem) {
    workspaceControls.selectCodeItem(item);
    visualMapControls.showMode(isApiItem(item) ? "api-flow" : "search-focus", `code:${item.id}`);
  }

  function selectTableCard(tableKey: string) {
    dbProfileControls.selectTable(tableKey);
    visualMapControls.showMode("table-usage", `db:table:${tableKey}`);
  }

  function selectColumnCard(tableKey: string, columnName: string) {
    dbProfileControls.selectColumn(tableKey, columnName);
    visualMapControls.showMode("column-impact", `db:column:${tableKey}:${columnName}`);
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
}

function ApiReadingPath({
  answer,
  map,
  onSelectNode,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  onSelectNode: (node: VisualNode) => void;
}) {
  const lanes: Array<{
    id: string;
    number: string;
    title: string;
    description: string;
    empty: string;
    tone: "structural" | "confirmed" | "candidate";
    items: Array<ApiReadingStep | ImpactReviewItem>;
  }> = [
    { id: "route", number: "01", title: "Route", description: "선택한 API 진입점", empty: "선택한 Route를 읽지 못했습니다.", tone: "structural", items: answer.steps.filter((step) => step.lane === "route") },
    { id: "handler", number: "02", title: "Handler", description: "확정 HANDLES 대상", empty: "확정 HANDLES 대상을 찾지 못했습니다.", tone: "confirmed", items: answer.steps.filter((step) => step.lane === "handler") },
    { id: "service-function", number: "03", title: "Service / Function", description: "확정 CALLS 경로", empty: "확정 Service/Function 경로를 찾지 못했습니다.", tone: "confirmed", items: answer.steps.filter((step) => step.lane === "service-function") },
    { id: "repository-query", number: "04", title: "Repository / Query", description: "데이터 접근 코드", empty: "확정 Repository/Query 경로를 찾지 못했습니다.", tone: "confirmed", items: answer.steps.filter((step) => step.lane === "repository-query") },
    { id: "db-candidate", number: "05", title: "DB 후보", description: "확정 경로 뒤의 검증 후보", empty: "찾은 DB 후보가 없습니다. DB 미사용이 확정된 것은 아닙니다.", tone: "candidate", items: answer.dbCandidates },
  ];

  return (
    <section className="at-impact-board at-api-reading" aria-label={`${answer.subject} API 읽기 경로`}>
      <header className="at-impact-board-head">
        <div>
          <span>API READING PATH</span>
          <strong>{answer.subject}</strong>
          <small>확정 HANDLES/CALLS만 경로로 읽고 DB 연결은 후보로 분리했습니다.</small>
        </div>
        {answer.truncated ? (
          <em className="at-api-truncated">
            {answer.hiddenBranches > 0
              ? answer.hiddenBranchesIsLowerBound
                ? `최소 +${answer.hiddenBranches} 경계 관계 · 하위 미탐색`
                : `+${answer.hiddenBranches}개 접힘`
              : "표시 한도에서 경로가 잘렸습니다"}
          </em>
        ) : null}
      </header>
      <div className="at-impact-lanes">
        {lanes.map((lane) => (
          <section className={`at-impact-lane ${lane.tone}`} key={lane.id} aria-labelledby={`api-lane-${lane.id}`}>
            <header>
              <span>{lane.number}</span>
              <div>
                <strong id={`api-lane-${lane.id}`}>{lane.title}</strong>
                <small>{lane.description}</small>
              </div>
              <em>{lane.items.length}</em>
            </header>
            <div className="at-impact-items">
              {lane.items.length === 0 ? <p className="at-impact-empty">{lane.empty}</p> : null}
              {lane.items.map((item) => {
                const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
                return <ImpactReviewEntry item={item} key={item.id} onSelect={node ? () => onSelectNode(node) : null} />;
              })}
            </div>
          </section>
        ))}
      </div>
      <div className="at-api-followups">
        <ApiReadingFollowup title="확인 안 된 구간" tone="unknown" items={answer.unknowns} map={map} onSelectNode={onSelectNode} empty="현재 표시 범위에서 확인 안 된 구간이 없습니다." />
        <ApiReadingFollowup title="권장 확인" tone="action" items={answer.recommendedChecks} map={map} onSelectNode={onSelectNode} empty="추가 권장 확인이 없습니다." />
      </div>
      {answer.truncationReason ? <p className="at-api-cap-note">표시 한도 · {answer.truncationReason}</p> : null}
    </section>
  );
}

function ApiReadingFollowup({
  title,
  tone,
  items,
  map,
  onSelectNode,
  empty,
}: {
  title: string;
  tone: "unknown" | "action";
  items: ImpactReviewItem[];
  map: VisualMap;
  onSelectNode: (node: VisualNode) => void;
  empty: string;
}) {
  return (
    <section className={`at-impact-lane ${tone}`}>
      <header>
        <span>{tone === "unknown" ? "?" : "✓"}</span>
        <div><strong>{title}</strong><small>{tone === "unknown" ? "근거가 없거나 접힌 영역" : "다음에 열어볼 대상"}</small></div>
        <em>{items.length}</em>
      </header>
      <div className="at-impact-items">
        {items.length === 0 ? <p className="at-impact-empty">{empty}</p> : null}
        {items.map((item) => {
          const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
          return <ImpactReviewEntry item={item} key={item.id} onSelect={node ? () => onSelectNode(node) : null} />;
        })}
      </div>
    </section>
  );
}

function ImpactReviewBoard({
  board,
  map,
  onSelectNode,
}: {
  board: ImpactReviewBoardModel;
  map: VisualMap;
  onSelectNode: (node: VisualNode) => void;
}) {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");

  async function copySummary() {
    const copied = await copyValue(board.markdownSummary);
    setCopyState(copied ? "copied" : "failed");
    window.setTimeout(() => setCopyState("idle"), 1800);
  }

  return (
    <section className="at-impact-board" aria-label={`${board.subject} 변경 영향 리뷰 보드`}>
      <header className="at-impact-board-head">
        <div>
          <span>{board.scope === "column" ? "COLUMN CHANGE REVIEW" : "TABLE CHANGE REVIEW"}</span>
          <strong>{board.subject}</strong>
          <small>직접 사실과 후보를 섞지 않고 수정 전 확인 순서로 정리했습니다.</small>
        </div>
        <button type="button" onClick={() => void copySummary()} aria-label="변경 영향 Markdown 요약 복사">
          {copyState === "copied" ? <CheckCircle2 size={14} /> : <ClipboardCopy size={14} />}
          {copyState === "copied" ? "복사됨" : copyState === "failed" ? "복사 실패" : "Markdown 복사"}
        </button>
      </header>

      <div className="at-impact-lanes">
        {board.lanes.map((lane) => (
          <section className={`at-impact-lane ${lane.tone}`} key={lane.id} aria-labelledby={`impact-lane-${lane.id}`}>
            <header>
              <span>{String(lane.order).padStart(2, "0")}</span>
              <div>
                <strong id={`impact-lane-${lane.id}`}>{lane.title}</strong>
                <small>{lane.description}</small>
              </div>
              <em>{lane.total}</em>
            </header>
            <div className="at-impact-items">
              {lane.items.length === 0 ? <p className="at-impact-empty">{lane.emptyMessage}</p> : null}
              {lane.items.map((item) => {
                const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
                return (
                  <ImpactReviewEntry
                    item={item}
                    key={item.id}
                    onSelect={node ? () => onSelectNode(node) : null}
                  />
                );
              })}
            </div>
            {lane.hidden > 0 ? <footer>+{lane.hidden}개 접힘</footer> : null}
          </section>
        ))}
      </div>
      <span className="sr-only" aria-live="polite">
        {copyState === "copied" ? "Markdown 요약을 복사했습니다." : copyState === "failed" ? "Markdown 요약을 복사하지 못했습니다." : ""}
      </span>
    </section>
  );
}

function ImpactReviewEntry({ item, onSelect }: { item: ImpactReviewItem; onSelect: (() => void) | null }) {
  const content = (
    <>
      <div className="at-impact-item-head">
        <span>#{item.rank}</span>
        <strong>{item.title}</strong>
      </div>
      <div className="at-impact-item-badges">
        <span className={item.truthClass}>{reviewTruthLabel(item.truthClass)}</span>
        {item.confidence ? <span className="confidence">{confidenceLabel(item.confidence)}</span> : null}
        <code>{item.kind}</code>
      </div>
      <p>{item.detail}</p>
      {item.location ? (
        <small className="at-impact-location" title={item.location.path}>
          {item.location.path}
          {item.location.line ? `:L${item.location.line}` : ""}
        </small>
      ) : null}
      {item.evidence.length > 0 ? (
        <small className="at-impact-evidence" title={item.evidence.map((evidence) => evidence.text).join("\n")}>
          근거 · {impactEvidenceLabel(item.evidence[0])}
          {item.evidence.length > 1 ? ` · +${item.evidence.length - 1}` : ""}
        </small>
      ) : null}
    </>
  );

  return onSelect ? (
    <button className="at-impact-item selectable" type="button" onClick={onSelect}>
      {content}
    </button>
  ) : (
    <article className="at-impact-item">{content}</article>
  );
}

function impactEvidenceLabel(evidence: ImpactReviewItem["evidence"][number]): string {
  if (evidence.kind.startsWith("db-") || evidence.kind === "engine-edge") return "DB 메타데이터";
  if (evidence.kind === "column-name-match") return "컬럼명 일치";
  if (evidence.kind.startsWith("code-search")) return "코드 검색";
  if (evidence.kind === "handles") return "HANDLES 관계";
  if (evidence.kind === "calls") return "CALLS 관계";
  return evidence.text;
}

function reviewTruthLabel(value: string): string {
  if (value === "confirmed") return "확정";
  if (value === "structural") return "구조";
  if (value === "candidate") return "후보";
  if (value === "action") return "행동";
  return "알 수 없음";
}

type DomainCardSummary = {
  api: number;
  code: number;
  db: number;
  topApi: string;
  topCode: string;
  topDb: string;
};

function ArchitectureMap({
  map,
  relationCounts,
  selectedNodeId,
  onBack,
  onOpenGroup,
  onOpenMember,
}: {
  map: VisualMap;
  relationCounts: Map<string, RelationSummary>;
  selectedNodeId: string | null;
  onBack: () => void;
  onOpenGroup: (node: VisualNode) => void;
  onOpenMember: (node: VisualNode) => void;
}) {
  const groupNodes = map.nodes.filter((node) => node.kind === "group-domain");
  const detailGroup = map.focus.startsWith("group:") ? groupNodes.find((node) => node.id === map.focus) ?? null : null;

  if (!detailGroup) {
    return (
      <section className="at-architecture" aria-label="도메인별 전체 구조">
        <div className="at-architecture-notes" aria-label="전체 구조 표시 범위">
          {map.warnings.map((warning) => (
            <span key={warning}>{warning}</span>
          ))}
        </div>
        <div className="at-domain-grid">
          {groupNodes.map((node, index) => {
            const summary = parseDomainCardSummary(node.subtitle);
            return (
              <button
                className="at-domain-card"
                type="button"
                key={node.id}
                aria-label={`${node.title} 도메인 열기. ${summary ? `API ${summary.api}, 코드 ${summary.code}, DB ${summary.db}` : node.subtitle ?? "요약 없음"}`}
                title={`${node.title} 도메인 상세 열기`}
                onClick={() => onOpenGroup(node)}
              >
                <div className="at-domain-head">
                  <span aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                  <Layers3 size={15} />
                  <strong>{node.title}</strong>
                  <RelationBadge summary={relationCounts.get(node.id)} />
                </div>
                {summary ? (
                  <>
                    <div className="at-domain-counts" aria-label="도메인 항목 수">
                      <span><b>API</b><strong>{summary.api}</strong></span>
                      <span><b>코드</b><strong>{summary.code}</strong></span>
                      <span><b>DB</b><strong>{summary.db}</strong></span>
                    </div>
                    <div className="at-domain-facts">
                      <span><b>API</b><code title={summary.topApi || "API 없음"}>{summary.topApi || "없음"}</code></span>
                      <span><b>코드</b><code title={summary.topCode || "코드 없음"}>{summary.topCode || "없음"}</code></span>
                      <span><b>DB</b><code title={summary.topDb || "DB 없음"}>{summary.topDb || "없음"}</code></span>
                    </div>
                  </>
                ) : (
                  <small>{node.subtitle ?? "표시할 요약이 없습니다"}</small>
                )}
                <span className="at-domain-open">API → 코드 → DB 펼치기</span>
              </button>
            );
          })}
        </div>
      </section>
    );
  }

  const members = map.nodes.filter((node) => node.id !== detailGroup.id && !node.id.startsWith("group:"));
  const api = members.filter((node) => node.layer === "api");
  const code = members.filter((node) => node.source === "code" && node.layer !== "api");
  const db = members.filter((node) => node.source === "db" && node.kind === "table");

  return (
    <section className="at-architecture at-architecture-detail" aria-label={`${detailGroup.title} 도메인 상세`}>
      <div className="at-domain-detail-head">
        <button type="button" onClick={onBack} aria-label="전체 구조로 돌아가기">← 전체 구조</button>
        <span>선택 도메인</span>
        <strong>{detailGroup.title}</strong>
        <small>{detailGroup.subtitle?.split("|")[0] ?? "도메인 항목"}</small>
      </div>
      <div className="at-architecture-notes" aria-label="도메인 상세 표시 범위">
        {map.warnings.map((warning) => (
          <span key={warning}>{warning}</span>
        ))}
      </div>
      <ArchitectureMemberBand number="1" label="API" nodes={api} selectedNodeId={selectedNodeId} relationCounts={relationCounts} onOpen={onOpenMember} />
      <ArchitectureMemberBand number="2" label="코드" nodes={code} selectedNodeId={selectedNodeId} relationCounts={relationCounts} onOpen={onOpenMember} />
      <ArchitectureMemberBand number="3" label="DB" nodes={db} selectedNodeId={selectedNodeId} relationCounts={relationCounts} onOpen={onOpenMember} />
    </section>
  );
}

function ArchitectureMemberBand({
  number,
  label,
  nodes,
  selectedNodeId,
  relationCounts,
  onOpen,
}: {
  number: string;
  label: string;
  nodes: VisualNode[];
  selectedNodeId: string | null;
  relationCounts: Map<string, RelationSummary>;
  onOpen: (node: VisualNode) => void;
}) {
  return (
    <section className="at-domain-band" data-domain-band={number} aria-label={`${label} ${nodes.length}개`}>
      <header>
        <span>{number}</span>
        <strong>{label}</strong>
        <small>{nodes.length}개</small>
      </header>
      <div>
        {nodes.map((node) => (
          <button
            type="button"
            className={`at-domain-member ${selectedNodeId === node.id ? "selected" : ""}`}
            key={node.id}
            aria-pressed={selectedNodeId === node.id}
            aria-label={`${node.title} ${nodeKindLabel(node.kind)} 선택`}
            title={`${node.title} · ${node.subtitle ?? nodeKindLabel(node.kind)}`}
            onClick={() => onOpen(node)}
          >
            {node.source === "db" ? <Table2 size={14} /> : node.kind === "file" ? <FileText size={14} /> : <Cog size={14} />}
            <strong>{node.title}</strong>
            <span>{nodeKindLabel(node.kind)}</span>
            <RelationBadge summary={relationCounts.get(node.id)} />
            {node.subtitle && <small>{compactPath(node.subtitle) ?? node.subtitle}</small>}
          </button>
        ))}
        {nodes.length === 0 && <span className="at-domain-band-empty">이 계층에 읽힌 항목이 없습니다</span>}
      </div>
    </section>
  );
}

function parseDomainCardSummary(value?: string | null): DomainCardSummary | null {
  if (!value) {
    return null;
  }
  const [counts, topApi = "", topCode = "", topDb = ""] = value.split("|");
  const match = /^API (\d+) · 코드 (\d+) · DB (\d+)$/.exec(counts);
  if (!match) {
    return null;
  }
  return {
    api: Number(match[1]),
    code: Number(match[2]),
    db: Number(match[3]),
    topApi,
    topCode,
    topDb,
  };
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
  return `도메인 카드 ${groups}개 · 그룹 간 관계 ${map.edges.length}개`;
}

function showWorkbenchDbSetup(setView: (view: View) => void, dbProfileControls: DbProfileControls) {
  setView("workbench");
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

function RelationBadge({ summary }: { summary?: RelationSummary }) {
  if (!summary) {
    return null;
  }
  const total = summary.confirmed + summary.typed + summary.inferred + summary.candidate;
  if (total === 0) {
    return null;
  }
  const dominant =
    summary.confirmed > 0
      ? { label: "직접", count: summary.confirmed }
      : summary.typed > 0
        ? { label: "구조", count: summary.typed }
        : summary.candidate > 0
          ? { label: "후보", count: summary.candidate }
          : { label: "이름 단서", count: summary.inferred };
  const badgeLabel = dominant.label === "이름 단서" ? "단서" : dominant.label;
  const label = `${badgeLabel} ${dominant.count}/${total}`;
  const tone = summary.confirmed > 0 ? "confirmed" : summary.typed > 0 ? "typed" : summary.candidate > 0 ? "candidate" : "inferred";
  const title = `카드 선택 시 답 화면 열기 · 관계 ${total}개 · 직접 ${summary.confirmed} · 구조 ${summary.typed} · 후보 ${summary.candidate} · 이름 단서 ${summary.inferred}`;
  return (
    <span className={`at-relation-badge ${tone}`} title={title} aria-label={title}>
      {label}
    </span>
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
          <span>판정</span>
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

function edgeTouchesNode(edge: VisualEdge, node: VisualNode | null): boolean {
  if (!node) {
    return false;
  }
  if (edge.from === node.id || edge.to === node.id) {
    return true;
  }
  if (node.kind !== "table" || !node.id.startsWith("db:table:")) {
    return false;
  }
  const tableKey = node.id.slice("db:table:".length);
  const columnPrefix = `db:column:${tableKey}:`;
  return edge.from.startsWith(columnPrefix) || edge.to.startsWith(columnPrefix);
}

function edgeTouchesNodeId(edge: VisualEdge, nodeId: string): boolean {
  if (edge.from === nodeId || edge.to === nodeId) {
    return true;
  }
  const tableKey = tableKeyFromNodeId(nodeId);
  return tableKey ? edgeTouchesTable(edge, tableKey) : false;
}

function edgeTouchesTable(edge: VisualEdge, tableKey: string): boolean {
  return nodeTouchesTable(edge.from, tableKey) || nodeTouchesTable(edge.to, tableKey);
}

function nodeTouchesTable(nodeId: string, tableKey: string): boolean {
  return nodeId === `db:table:${tableKey}` || nodeId.startsWith(`db:column:${tableKey}:`);
}

function nodesShareTableOrId(a: string, b: string): boolean {
  if (a === b) {
    return true;
  }
  const aTable = tableKeyFromNodeId(a);
  return Boolean(aTable && nodeTouchesTable(b, aTable));
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
    title: "도메인 선택",
    meta: "중요도와 확정 연결도 순",
    hint: "카드를 열어 API → 코드 → DB 순서로 읽습니다.",
    tone: "neutral",
  };
}

function atlasFocusState(
  workspaceControls: WorkspaceControls,
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
  if (workspaceControls.selectedCodeItem) {
    const item = workspaceControls.selectedCodeItem;
    return {
      label: codeKindChip(item.kind),
      title: item.name,
      meta: item.line ? `라인 ${item.line}` : item.filePath ?? "코드 목록",
      hint: "답 패널에서 호출/후보 근거를 확인합니다.",
      tone: "code",
    };
  }
  const focusedColumn = columnRefFromNodeId(visualMapControls.currentMap?.focus ?? "");
  if (focusedColumn) {
    const table = tables.find((item) => dbInventoryTableKey(item) === focusedColumn.tableKey) ?? null;
    const column = table?.columns.find((item) => item.name === focusedColumn.columnName) ?? null;
    return {
      label: "컬럼",
      title: `${focusedColumn.tableKey}.${focusedColumn.columnName}`,
      meta: column ? columnMeta(column) || column.dataType || "컬럼" : "컬럼",
      hint: "답 패널에서 타입과 키 속성을 확인합니다.",
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
      hint: "답 패널에서 제약과 관계를 확인합니다.",
      tone: "db",
    };
  }
  return {
    label: "대상",
    title: "대상 선택",
    meta: "카드 또는 검색 결과를 선택하세요",
    hint: "선택하면 답 패널에 요약이 열립니다.",
    tone: "neutral",
  };
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
    label: nodeKindLabel(node.kind),
    title: nodeLabel(node.id, map),
    meta: `관계 ${edges.length}개 · 후보 ${candidates}개`,
    hint: node.kind === "column" ? "컬럼 근거는 직접/후보로 나눠 봅니다." : "답 패널에서 관계와 근거를 확인합니다.",
    tone: node.source === "db" ? "db" : node.source === "code" ? "code" : "neutral",
  };
}

function nodeLabel(id: string, map: VisualMap | null): string {
  const node = map?.nodes.find((item) => item.id === id);
  if (!node) {
    return columnLabelFromNodeId(id) ?? (id.startsWith("db:table:") ? id.slice("db:table:".length) : id);
  }
  if (node?.kind === "column") {
    const tableKey = tableKeyFromNodeId(id);
    return tableKey ? `${tableKey}.${node.title}` : node.title;
  }
  return node.title;
}

function compactRelationEndpointLabel(label: string): string {
  const parts = label.split(".");
  return parts.length >= 3 ? parts.slice(-2).join(".") : label;
}

function nodeKindLabel(kind: string): string {
  if (kind === "group-domain") {
    return "도메인";
  }
  if (kind === "api") {
    return "API";
  }
  if (kind === "table") {
    return "테이블";
  }
  if (kind === "column") {
    return "컬럼";
  }
  if (kind === "file") {
    return "파일";
  }
  return "코드";
}

function SetupChecklist({
  title,
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  title: string;
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const codeCount = codeInventoryItemCount(workspaceControls.codeInventory);
  const routeCount = workspaceControls.codeInventory?.routes.length ?? 0;
  const codeSymbolCount = codeInventoryCodeItems(workspaceControls.codeInventory).length;
  const fileCount = workspaceControls.codeInventory?.files.length ?? 0;
  const dbTables = dbProfileControls.inventory?.tables ?? [];
  const dbCount = dbTables.length;
  const dbColumnCount = dbTables.reduce((sum, table) => sum + table.columns.length, 0);
  const dbMissingColumnTables = dbTables.filter((table) => table.columns.length === 0).length;
  const dbReady = dbCount > 0 && dbColumnCount > 0 && dbMissingColumnTables === 0;
  const hasCodeContext = routeCount > 0 || codeSymbolCount > 0;
  const codeIndexed = workspaceControls.codeStatus?.includes("완료") ?? false;
  const dbStarted = dbProfileWorkStarted(dbProfileControls);
  const canUseCodeStep = codeIndexed || workspaceControls.canIndexCode;
  const projectSourceLabel = workspaceControls.repoSourceMode === "github" ? "GitHub 저장소" : "프로젝트 폴더";
  const projectStepLabel = workspaceControls.repoSourceMode === "github" ? "저장소 연결" : "프로젝트 열기";
  const projectStepAction = workspaceControls.canCreateWorkspace
    ? workspaceControls.repoSourceMode === "github"
      ? "복제하고 열기"
      : "프로젝트 열기"
    : workspaceControls.repoSourceMode === "github"
      ? "URL 입력"
      : "폴더 선택";
  const codeStepAction = codeIndexed ? "목록 열기" : workspaceControls.canIndexCode ? "코드 읽기" : "코드 섹션";
  const dbStepAction = dbProfileControls.canIndexProfile
    ? "DB 읽기"
    : dbProfileControls.canLoadInventory
      ? "목록 열기"
      : "DB 섹션";
  const codeReadyText = codeInventoryReadyText({
    routes: routeCount,
    code: codeSymbolCount,
    files: fileCount,
    tables: 0,
    columns: 0,
  });
  const steps = [
    {
      icon: FolderOpen,
      label: projectStepLabel,
      text: hasWorkspace
        ? workspaceControls.currentWorkspace?.name ?? "연결됨"
        : workspaceControls.canCreateWorkspace
          ? `${workspaceControls.workspaceName || projectSourceLabel} ${workspaceControls.repoSourceMode === "github" ? "복제 준비됨" : "열기 준비됨"}`
          : workspaceControls.repoSourceMode === "github"
            ? "GitHub URL 입력"
          : "로컬 폴더를 지정하세요",
      feedback: workspaceControls.error ?? null,
      done: hasWorkspace,
      place: projectStepAction,
      run: workspaceControls.canCreateWorkspace
        ? workspaceControls.createWorkspace
        : workspaceControls.repoSourceMode === "local"
          ? workspaceControls.pickRepoPath
          : () => {
              setView("workbench");
              focusWorkspaceSetup(workspaceControls);
            },
      disabled: workspaceControls.busy,
    },
    {
      icon: Layers3,
      label: "코드 목록",
      text: codeCount > 0 ? codeReadyText : "API, 코드, 파일 읽기",
      feedback: workspaceControls.codeError ?? null,
      done: codeCount > 0,
      place: codeStepAction,
      run: codeIndexed
        ? workspaceControls.loadCodeInventory
        : workspaceControls.canIndexCode
        ? workspaceControls.indexCodeRepository
        : () => focusSourceSetup(setView, workspaceControls, dbProfileControls),
      disabled: workspaceControls.busy || !hasWorkspace,
    },
    {
      icon: Database,
      label: "DB 구조",
      text: dbReady
        ? `테이블 ${dbCount}개 · 컬럼 ${dbColumnCount}개 읽힘`
        : dbMissingColumnTables > 0 && dbColumnCount > 0
          ? `테이블 ${dbCount}개 · ${dbMissingColumnTables}개 컬럼 보강`
        : dbCount > 0
          ? `테이블 ${dbCount}개 · 컬럼 대기`
          : hasCodeContext
            ? "변경 범위 읽기"
          : "테이블/컬럼 읽기",
      feedback: dbProfileControls.error ?? null,
      done: dbReady,
      place: dbStepAction,
      run: dbProfileControls.canIndexProfile
        ? dbProfileControls.indexProfile
        : dbProfileControls.canLoadInventory
          ? dbProfileControls.loadInventory
          : () => showWorkbenchDbSetup(setView, dbProfileControls),
      disabled: dbProfileControls.busy || !hasWorkspace,
    },
  ];
  const firstCodeItem =
    workspaceControls.codeInventory?.routes[0] ??
    codeInventoryCodeItems(workspaceControls.codeInventory)[0] ??
    workspaceControls.codeInventory?.files[0] ??
    null;
  const firstTableKey = dbTables[0] ? dbInventoryTableKey(dbTables[0]) : null;
  const firstColumnFocus =
    dbTables
      .map((table) => {
        const column = table.columns.find((item) => item.isForeignKey) ?? table.columns[0] ?? null;
        return column ? `db:column:${dbInventoryTableKey(table)}:${column.name}` : null;
      })
      .find(Boolean) ?? null;
  const runCodeAnswer = () => {
    if (!hasWorkspace) {
      steps[0].run();
      return;
    }
    if (workspaceControls.codeInventory?.routes[0]) {
      visualMapControls.showMode("api-flow", `code:${workspaceControls.codeInventory.routes[0].id}`);
      return;
    }
    if (firstCodeItem) {
      visualMapControls.showMode("search-focus", `code:${firstCodeItem.id}`);
      return;
    }
    steps[1].run();
  };
  const runTableAnswer = () => {
    if (!hasWorkspace) {
      steps[0].run();
      return;
    }
    if (firstTableKey) {
      visualMapControls.showMode("table-usage", `db:table:${firstTableKey}`);
      return;
    }
    steps[2].run();
  };
  const runImpactAnswer = () => {
    if (!hasWorkspace) {
      steps[0].run();
      return;
    }
    if (firstColumnFocus) {
      visualMapControls.showMode("column-impact", firstColumnFocus);
      return;
    }
    steps[2].run();
  };
  const activeStep = !hasWorkspace
    ? 0
    : codeCount === 0 && !dbStarted && canUseCodeStep
      ? 1
      : !dbReady
        ? 2
        : -1;
  const setupSummary = !hasWorkspace
    ? workspaceControls.canCreateWorkspace
      ? workspaceControls.repoSourceMode === "github"
        ? "복제하면 API, 코드, DB 답이 열립니다."
        : "열면 API, 코드, DB 답이 열립니다."
      : workspaceControls.repoSourceMode === "github"
        ? "GitHub URL을 넣으면 코드와 DB 답을 엽니다."
        : "폴더를 연결하면 코드와 DB 답이 열립니다."
    : dbCount > 0 && !dbReady
    ? dbColumnCount > 0
      ? "일부 테이블은 컬럼을 더 읽어야 해당 제약과 변경 영향이 열립니다."
      : "테이블 목록은 읽혔고, 컬럼을 읽으면 제약과 변경 영향이 열립니다."
    : hasCodeContext
      ? routeCount > 0
        ? "코드와 DB 구조를 읽으면 API 경로, DB 제약, 후보 근거가 채워집니다."
        : "코드와 DB 구조를 읽으면 파일/심볼 구조, DB 제약, 후보 근거가 채워집니다."
      : "DB 구조는 테이블 구조와 컬럼 제약부터 보여주고, 코드 목록이 연결되면 후보 근거까지 확장됩니다.";
  const heroTitle = hasWorkspace ? title : `${projectSourceLabel} 연결`;
  const codeAnswerState = hasCodeContext || fileCount > 0 ? "ready" : "pending";
  const tableAnswerState = dbCount > 0 ? "ready" : "pending";
  const impactAnswerState = dbReady ? "ready" : dbColumnCount > 0 ? "partial" : "pending";
  const answerTitle = hasWorkspace ? "눌러서 바로 찾기" : "연결 후 찾을 답";
  const projectNeededSource = workspaceControls.repoSourceMode === "github" ? "URL 먼저" : "폴더 먼저";
  const codeNeededSource = hasWorkspace ? "코드 읽기 먼저" : projectNeededSource;
  const dbNeededSource = hasWorkspace ? "DB 연결 먼저" : projectNeededSource;
  const codeAnswerPreview =
    routeCount > 0
      ? { label: "API 흐름", question: "이 API는 어디서 처리돼?", source: "라우트 + 코드", state: codeAnswerState }
      : codeSymbolCount > 0
        ? { label: "코드 근거", question: "이 코드는 어디서 불려?", source: "심볼 + 파일", state: codeAnswerState }
        : fileCount > 0
          ? { label: "파일 근거", question: "이 파일은 어디와 묶여?", source: "파일 목록", state: codeAnswerState }
        : hasWorkspace
          ? { label: "코드 목록", question: "읽힌 코드가 뭐야?", source: codeNeededSource, state: codeAnswerState }
          : { label: "API 흐름", question: "이 API는 어디서 처리돼?", source: projectNeededSource, state: codeAnswerState };
  const answerPreviews = [
    { ...codeAnswerPreview, run: runCodeAnswer },
    dbCount > 0 && !dbReady
      ? { label: "테이블 목록", question: "읽힌 테이블 확인", source: "DB 구조", state: tableAnswerState, run: runTableAnswer }
      : dbCount === 0
        ? { label: "테이블 연결", question: "이 테이블은 누가 써?", source: dbNeededSource, state: tableAnswerState, run: runTableAnswer }
      : hasCodeContext
        ? { label: "테이블 연결", question: "이 테이블은 누가 써?", source: "코드 + DB", state: tableAnswerState, run: runTableAnswer }
          : { label: "테이블 구조", question: "PK/FK가 어떻게 묶여?", source: "DB 구조", state: tableAnswerState, run: runTableAnswer },
    dbCount > 0 && !dbReady
      ? { label: "컬럼 보강", question: "컬럼 읽기 필요", source: dbColumnCount > 0 ? "컬럼 보강 필요" : "컬럼 읽기 필요", state: impactAnswerState, run: runImpactAnswer }
      : dbCount === 0
        ? { label: "변경 범위", question: "이 컬럼 바꾸면 어디까지 닿아?", source: dbNeededSource, state: impactAnswerState, run: runImpactAnswer }
      : hasCodeContext
        ? { label: "변경 범위", question: "이 컬럼 바꾸면 어디까지 닿아?", source: "컬럼 + 근거", state: impactAnswerState, run: runImpactAnswer }
          : { label: "컬럼 제약", question: "컬럼 제약 구조", source: "컬럼 + 제약", state: impactAnswerState, run: runImpactAnswer },
  ];
  const activeStepIndex = activeStep >= 0 ? activeStep : steps.findIndex((step) => !step.done);

  return (
    <div className="map-empty setup-empty">
      <div className="setup-hero">
        <Layers3 size={28} />
        <strong>{heroTitle}</strong>
        <span>{setupSummary}</span>
      </div>
      <div className="setup-body">
        <div className="setup-start">
          <span className="setup-answer-title">연결 순서</span>
          <div className="setup-steps" aria-label="프로젝트 연결 순서">
            {steps.map((step, index) => {
              const StepIcon = step.icon;
              const stepActive = index === activeStepIndex && !step.done;
              return (
                <div
                  className={`setup-step ${step.done ? "done" : ""} ${stepActive ? "active primary" : ""}`}
                  aria-current={stepActive ? "step" : undefined}
                  key={step.label}
                >
                  <span className="setup-state">{step.done ? <CheckCircle2 size={15} /> : index + 1}</span>
                  <StepIcon size={16} />
                  <span className="setup-copy">
                    <b>{step.label}</b>
                    <small>{step.text}</small>
                    {step.feedback && (
                      <small className={`setup-feedback ${step.feedback === tauriUnavailableMessage ? "notice" : "error"}`}>
                        {step.feedback}
                      </small>
                    )}
                  </span>
                  {step.done ? (
                    <span className="setup-place">완료</span>
                  ) : stepActive ? (
                    <button
                      className="setup-place action"
                      type="button"
                      onClick={step.run}
                      onPointerDown={(event) => event.stopPropagation()}
                      disabled={step.disabled}
                    >
                      {step.place}
                    </button>
                  ) : (
                    <span className="setup-place waiting">대기</span>
                  )}
                </div>
              );
            })}
          </div>
        </div>
        <div className="setup-answers" aria-label="코드/DB 연결 후 확인할 수 있는 답">
          <span className="setup-answer-title">{answerTitle}</span>
          {answerPreviews.map((item) => (
            <button
              className={`setup-answer ${item.state}`}
              key={item.label}
              type="button"
              aria-label={`${item.label}: ${item.question} · ${item.source}`}
              title={`${item.question} · ${item.source}`}
              onClick={item.run}
              onPointerDown={(event) => event.stopPropagation()}
            >
              <small>{item.label}</small>
              <b>{item.question}</b>
              <em>{item.source}</em>
            </button>
          ))}
        </div>
      </div>
    </div>
  );

}

function atlasCanvasFacts({
  mode,
  mapNodes,
  mapEdges,
  mapWarnings,
  routes,
  code,
  files,
  tables,
  columns,
  searchSummary,
}: {
  mode: string;
  mapNodes: number;
  mapEdges: number;
  mapWarnings: number;
  routes: number;
  code: number;
  files: number;
  tables: number;
  columns: number;
  searchSummary: string | null;
}): string {
  if (mode === "search-focus" && searchSummary) {
    return searchSummary;
  }
  if (mapNodes > 0 || mapEdges > 0) {
    return `항목 ${mapNodes}개 · 관계 ${mapEdges}개${mapWarnings > 0 ? ` · 경고 ${mapWarnings}개` : ""}`;
  }
  return inventoryFactsText({ routes, code, files, tables, columns });
}

function atlasReadOrder(mode: string, counts: AtlasInventoryCounts): string {
  if (mode === "api-flow") {
    return counts.routes > 0 && counts.code > 0 ? "확인 순서: API → 코드" : inventoryReadOrder(counts);
  }
  if (mode === "table-usage") {
    return counts.code > 0 && counts.tables > 0 ? "확인 순서: 테이블 → 코드 후보" : inventoryReadOrder(counts);
  }
  if (mode === "column-impact") {
    return counts.code > 0 && counts.tables > 0 ? "확인 순서: 컬럼 → 영향 후보" : inventoryReadOrder(counts);
  }
  if (mode === "search-focus") {
    return "확인 순서: 검색 대상 → 주변 근거";
  }
  return inventoryReadOrder(counts);
}

function atlasModePurpose(mode: string, counts: AtlasInventoryCounts): string {
  if (mode === "api-flow") {
    if (counts.routes > 0) {
      return "API가 어떤 코드까지 이어지는지 봅니다";
    }
    return counts.files > 0 && counts.code === 0 ? "API 라우트 없음 · 파일 구조부터 봅니다" : "API 라우트 없음 · 실제 코드만 봅니다";
  }
  if (mode === "table-usage") {
    if (counts.tables > 0 && counts.columns === 0) {
      return "컬럼 대기 · 테이블 목록만 봅니다";
    }
    return counts.tables > 0 && (counts.routes > 0 || counts.code > 0)
      ? "테이블 연결과 제약을 분리해서 봅니다"
      : counts.tables > 0
        ? "테이블 구조와 DB 제약만 봅니다"
        : "DB 연결 전 코드 구조를 봅니다";
  }
  if (mode === "column-impact") {
    if (counts.tables === 0) {
      return "DB를 연결하면 컬럼 답이 열립니다";
    }
    if (counts.columns === 0) {
      return "컬럼을 읽으면 변경 범위가 열립니다";
    }
    return counts.routes > 0 || counts.code > 0
      ? "컬럼 변경의 직접/후보 근거를 봅니다"
      : "컬럼 제약과 DB 내부 구조를 봅니다";
  }
  if (mode === "search-focus") {
    return "검색한 대상만 좁혀 봅니다";
  }
  if (counts.routes === 0 && counts.code === 0 && counts.files > 0 && counts.tables > 0) {
    return "코드 심볼 없음 · 파일과 DB 구조부터 봅니다";
  }
  if (counts.routes === 0 && counts.code === 0 && counts.files > 0) {
    return "코드 심볼 없음 · 파일 구조부터 봅니다";
  }
  if (counts.routes === 0 && counts.code > 0 && counts.tables > 0) {
    return "API 라우트 없음 · 코드 구조부터 봅니다";
  }
  if (counts.routes === 0 && counts.code > 0) {
    return "API 라우트 없음 · 실제 코드만 봅니다";
  }
  if (counts.routes === 0 && counts.tables > 0 && counts.columns === 0) {
    return "컬럼 연결 전 테이블 목록만 봅니다";
  }
  if (counts.routes === 0 && counts.tables > 0) {
    return "코드 연결 전 DB 구조를 봅니다";
  }
  return "API·코드·DB 전체 구조를 봅니다";
}

function atlasCanvasGuide({
  mode,
  counts,
  readOrder,
  relationTotal,
  selectedEdge,
  selectedNode,
  selectedTableNeedsColumns,
}: {
  mode: string;
  counts: AtlasInventoryCounts;
  readOrder: string;
  relationTotal: number;
  selectedEdge: boolean;
  selectedNode: boolean;
  selectedTableNeedsColumns: boolean;
}): CanvasGuide {
  if (selectedEdge) {
    return {
      question: "이 관계는 근거가 있나",
      action: "근거 보기",
      basis: "양끝 항목 확인",
    };
  }
  if (selectedNode) {
    if (selectedTableNeedsColumns) {
      return {
        question: "테이블 구조가 충분한가",
        action: "DB 컬럼 보강",
        basis: "테이블 목록만 있음",
      };
    }
    if (relationTotal === 0) {
      return {
        question: "이 대상에 연결이 있나",
        action: "요약 확인",
        basis: "관계 없음",
      };
    }
    return {
      question: "선택 대상 영향",
      action: "관계 행 선택",
      basis: relationTotal > 0 ? `관계 ${relationTotal}개` : "관계 없음",
    };
  }
  if (mode === "api-flow") {
    return {
      question: "이 API가 어디까지 닿나",
      action: "API 카드 선택",
      basis: readOrder,
    };
  }
  if (mode === "table-usage") {
    if (counts.tables > 0 && counts.columns === 0) {
      return {
        question: "테이블 구조가 충분한가",
        action: "컬럼 구조 보강",
        basis: readOrder,
      };
    }
    return {
      question: counts.code > 0 ? "이 테이블과 연결된 코드는?" : "테이블 키 구조",
      action: "테이블 카드 선택",
      basis: readOrder,
    };
  }
  if (mode === "column-impact") {
    if (counts.tables > 0 && counts.columns === 0) {
      return {
        question: "영향 근거가 있는가",
        action: "DB 컬럼 보강",
        basis: readOrder,
      };
    }
    return {
      question: counts.code > 0 ? "이 컬럼 변경 범위는?" : "컬럼 제약",
      action: "컬럼 선택",
      basis: readOrder,
    };
  }
  if (mode === "search-focus") {
    return {
      question: "이 대상 주변에 뭐가 있나",
      action: "검색 결과 선택",
      basis: readOrder,
    };
  }
  return {
    question: "먼저 볼 덩어리는?",
    action: relationTotal > 0 ? "카드 선택" : "프로젝트/DB 연결",
    basis: readOrder,
  };
}

function inventoryReadOrder(counts: AtlasInventoryCounts): string {
  const parts = [
    counts.routes > 0 ? "API" : null,
    counts.code > 0 ? "코드" : null,
    counts.files > 0 && counts.code === 0 ? "파일" : null,
    counts.tables > 0 ? "DB" : null,
  ].filter(Boolean);
  return `확인 순서: ${parts.length > 0 ? parts.join(" → ") : "코드/DB 연결"}`;
}

function inventoryFactsText(counts: AtlasInventoryCounts): string {
  const parts = [
    counts.routes > 0 ? `API ${counts.routes}개` : null,
    counts.code > 0 ? `코드 ${counts.code}개` : null,
    counts.files > 0 ? `파일 ${counts.files}개` : null,
    counts.tables > 0 ? `테이블 ${counts.tables}개` : null,
    counts.tables > 0 ? (counts.columns > 0 ? `컬럼 ${counts.columns}개` : "컬럼 대기") : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : "코드/DB 연결";
}

function codeInventoryReadyText(counts: AtlasInventoryCounts): string {
  return `${inventoryFactsText(counts)} 읽힘`;
}

function atlasModeTitle(mode: string, counts: AtlasInventoryCounts): string {
  if (mode === "api-flow") {
    return "API가 닿는 코드";
  }
  if (mode === "table-usage") {
    if (counts.tables > 0 && counts.columns === 0) {
      return "테이블 목록";
    }
    return counts.routes > 0 || counts.code > 0 ? "테이블 연결" : "테이블 구조";
  }
  if (mode === "column-impact") {
    if (counts.tables > 0 && counts.columns === 0) {
      return "컬럼 대기";
    }
    return counts.routes > 0 || counts.code > 0 ? "컬럼 변경 범위" : "컬럼 제약";
  }
  if (mode === "search-focus") {
    return "대상 주변 근거";
  }
  return "전체 구조";
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

function takeWithPinned<T>(items: T[], pinnedIds: Set<string>, key: (item: T) => string, limit: number): T[] {
  const pinned = items.filter((item) => pinnedIds.has(key(item)));
  const rest = items.filter((item) => !pinnedIds.has(key(item))).slice(0, Math.max(0, limit - pinned.length));
  return [...pinned, ...rest].slice(0, limit);
}

function idsInItems<T>(ids: Set<string>, items: T[], key: (item: T) => string): Set<string> {
  return new Set(items.map(key).filter((id) => ids.has(id)));
}

function codeIdsFromNodeIds(nodeIds: Array<string | null | undefined>): Set<string> {
  return new Set(
    nodeIds
      .filter((id): id is string => Boolean(id?.startsWith("code:")))
      .map((id) => id.slice("code:".length)),
  );
}

function tableKeysFromNodeIds(nodeIds: Array<string | null | undefined>): Set<string> {
  return new Set(
    nodeIds
      .map((id) => (id ? tableKeyFromNodeId(id) : null))
      .filter((id): id is string => Boolean(id)),
  );
}

function columnNamesForTableFromNodeIds(nodeIds: Array<string | null | undefined>, tableKey: string): Set<string> {
  return new Set(
    nodeIds
      .map((id) => (id ? columnRefFromNodeId(id) : null))
      .filter((ref): ref is { tableKey: string; columnName: string } => Boolean(ref && ref.tableKey === tableKey))
      .map((ref) => ref.columnName),
  );
}

function atlasCodeKindRank(kind: string): number {
  const key = kind.trim().toLowerCase();
  if (key === "handler" || key === "controller" || key === "function" || key === "method") {
    return 0;
  }
  if (key === "service") {
    return 1;
  }
  if (key === "repository") {
    return 2;
  }
  return key === "class" ? 3 : 4;
}

function filterCodeItemsByMap<T extends { id: string }>(items: T[], focusedNodeIds: Set<string>): T[] {
  const filtered = items.filter((item) => focusedNodeIds.has(`code:${item.id}`));
  return filtered.length > 0 ? filtered : items;
}

function filterTablesByMap(items: DbInventoryTable[], focusedNodeIds: Set<string>): DbInventoryTable[] {
  const filtered = items.filter((item) => {
    const tableKey = dbInventoryTableKey(item);
    return focusedNodeIds.has(`db:table:${tableKey}`) || Array.from(focusedNodeIds).some((id) => id.startsWith(`db:column:${tableKey}:`));
  });
  return filtered.length > 0 ? filtered : items;
}

function rankNodeItems<T>(
  items: T[],
  relationCounts: Map<string, RelationSummary>,
  nodeId: (item: T) => string,
  selectedNodeId: string | null,
): T[] {
  return [...items].sort((a, b) => {
    const aId = nodeId(a);
    const bId = nodeId(b);
    if (selectedNodeId) {
      if (aId === selectedNodeId) return -1;
      if (bId === selectedNodeId) return 1;
    }
    return relationScore(relationCounts.get(bId)) - relationScore(relationCounts.get(aId));
  });
}

function relationScore(summary?: RelationSummary): number {
  return summary ? summary.confirmed * 100 + summary.typed * 60 + summary.candidate * 30 + summary.inferred * 15 : 0;
}

function buildRelationCounts(map: VisualMap | null): Map<string, RelationSummary> {
  const counts = new Map<string, RelationSummary>();
  if (!map) {
    return counts;
  }

  for (const edge of map.edges) {
    for (const nodeId of relationCountNodeIds(edge)) {
      addRelation(counts, nodeId, edge);
    }
  }
  return counts;
}

function relationCountNodeIds(edge: VisualEdge): string[] {
  return Array.from(new Set([edge.from, edge.to, tableAggregateNodeId(edge.from), tableAggregateNodeId(edge.to)].filter(Boolean) as string[]));
}

function tableAggregateNodeId(nodeId: string): string | null {
  const tableKey = tableKeyFromNodeId(nodeId);
  const tableId = tableKey ? `db:table:${tableKey}` : null;
  return tableId && tableId !== nodeId ? tableId : null;
}

function addRelation(counts: Map<string, RelationSummary>, nodeId: string, edge: VisualEdge) {
  const summary = counts.get(nodeId) ?? { confirmed: 0, typed: 0, inferred: 0, candidate: 0 };
  const tone = relationTone(edge);
  if (tone === "candidate") {
    summary.candidate += 1;
  } else if (tone === "inferred") {
    summary.inferred += 1;
  } else if (tone === "typed") {
    summary.typed += 1;
  } else {
    summary.confirmed += 1;
  }
  counts.set(nodeId, summary);
}

function relationLedgerRows(
  map: VisualMap | null,
  selectedEdge: VisualEdge | null,
  selectedNode: VisualNode | null,
  selectedFocusId: string | null,
): RelationLedgerRow[] {
  return [...relationLedgerScopedEdges(map, selectedEdge, selectedNode, selectedFocusId)]
    .sort((a, b) => relationLedgerRank(a, selectedEdge, selectedNode, selectedFocusId) - relationLedgerRank(b, selectedEdge, selectedNode, selectedFocusId))
    .slice(0, 5)
    .map((edge) => {
      const tone = relationTone(edge);
      const from = nodeLabel(edge.from, map);
      const to = nodeLabel(edge.to, map);
      return {
        edge,
        from: compactRelationEndpointLabel(from),
        fromTitle: from,
        to: compactRelationEndpointLabel(to),
        toTitle: to,
        label: relationLabel(tone),
        tone,
        evidence: relationEvidenceText(edge, tone),
      };
    });
}

function relationLedgerScopedEdges(
  map: VisualMap | null,
  selectedEdge: VisualEdge | null,
  selectedNode: VisualNode | null,
  selectedFocusId: string | null,
): VisualEdge[] {
  if (!map) {
    return [];
  }
  if (selectedEdge) {
    return map.edges.filter((edge) => edge.id === selectedEdge.id);
  }
  if (selectedNode) {
    return map.edges.filter((edge) => edgeTouchesNode(edge, selectedNode));
  }
  if (selectedFocusId) {
    return map.edges.filter((edge) => edgeTouchesNodeId(edge, selectedFocusId));
  }
  return map.edges;
}

function relationEvidenceText(edge: VisualEdge, tone: RelationTone): string {
  const evidence = edge.evidence[0]?.text?.trim();
  if (evidence) {
    return readableRelationEvidence(evidence, edge, tone);
  }
  if (tone === "candidate") {
    return edge.confidence ? `후보 근거 · 단서 ${confidenceLabel(edge.confidence)}` : "후보 근거 · 직접 검증 대기";
  }
  if (tone === "inferred") {
    return "이름 단서 · 호출 근거 대기";
  }
  return `${edgeKindLabel(edge)} · 구조 정보 기준`;
}

function readableRelationEvidence(evidence: string, edge: VisualEdge, tone: RelationTone): string {
  if (/[가-힣]/.test(evidence)) {
    return evidence;
  }

  const lower = evidence.toLowerCase();
  if (edge.kind === "code_handle") {
    return "코드 엔진 HANDLES로 확인한 Route → Handler 근거";
  }
  if (edge.kind === "code_call" || lower.includes("calls from")) {
    if (lower.includes("route") && lower.includes("service")) {
      return "라우트에서 서비스로 이어지는 호출 근거";
    }
    if (lower.includes("service") && lower.includes("repository")) {
      return "서비스에서 저장소로 이어지는 호출 근거";
    }
    return "읽은 코드 호출 근거";
  }
  if (edge.kind === "db_fk" || lower.includes("foreign key") || lower.startsWith("fk ")) {
    return "DB FK 제약으로 확인된 구조 근거";
  }
  if (tone === "candidate" || lower.includes("name match") || lower.includes("table name match")) {
    return edge.confidence ? `이름 단서가 맞아 후보로 연결 · ${confidenceLabel(edge.confidence)}` : "이름 단서가 맞아 후보로 연결";
  }
  return evidence;
}

function relationLedgerRank(edge: VisualEdge, selectedEdge: VisualEdge | null, selectedNode: VisualNode | null, selectedFocusId: string | null): number {
  if (selectedEdge?.id === edge.id) {
    return -20;
  }
  if (edgeTouchesNode(edge, selectedNode)) {
    return -10 + relationRank(edge);
  }
  if (selectedFocusId && edgeTouchesNodeId(edge, selectedFocusId)) {
    return -10 + relationRank(edge);
  }
  return relationRank(edge);
}

function buildRelationBeams({
  map,
  routeCards,
  codeCards,
  tableCards,
  bands,
  selectedEdge,
  selectedNode,
  selectedFocusId,
}: {
  map: VisualMap | null;
  routeCards: { id: string }[];
  codeCards: { id: string }[];
  tableCards: DbInventoryTable[];
  bands: Array<"api" | "code" | "db">;
  selectedEdge: VisualEdge | null;
  selectedNode: VisualNode | null;
  selectedFocusId: string | null;
}): RelationBeam[] {
  if (!map) {
    return [];
  }
  const visibleEdges = prioritizedBeamEdges(map.edges, selectedEdge, selectedNode, selectedFocusId);
  return visibleEdges.flatMap((edge) => {
    const from = nodePosition(edge.from, routeCards, codeCards, tableCards);
    const to = nodePosition(edge.to, routeCards, codeCards, tableCards);
    if (!from || !to) {
      return [];
    }
    const tone = relationTone(edge);
    return [
      {
        edge,
        x1: laneCenterX(from.lane),
        x2: laneCenterX(to.lane),
        y1: bandCenterPercent(bands, from.band),
        y2: bandCenterPercent(bands, to.band),
        tone,
        active: selectedEdge?.id === edge.id || edgeTouchesNode(edge, selectedNode) || Boolean(selectedFocusId && edgeTouchesNodeId(edge, selectedFocusId)),
        label: `${relationLabel(tone)} 관계: ${nodeLabel(edge.from, map)} → ${nodeLabel(edge.to, map)}`,
      },
    ];
  });
}

function prioritizedBeamEdges(
  edges: VisualEdge[],
  selectedEdge: VisualEdge | null,
  selectedNode: VisualNode | null,
  selectedFocusId: string | null,
): VisualEdge[] {
  const focused = edges.filter(
    (edge) =>
      selectedEdge?.id === edge.id ||
      edgeTouchesNode(edge, selectedNode) ||
      Boolean(selectedFocusId && edgeTouchesNodeId(edge, selectedFocusId)),
  );
  return uniqueEdges([
    ...focused.sort((a, b) => beamFocusRank(a, selectedEdge) - beamFocusRank(b, selectedEdge)),
    ...[...edges].sort((a, b) => relationRank(a) - relationRank(b)),
  ]).slice(0, 12);
}

function beamFocusRank(edge: VisualEdge, selectedEdge: VisualEdge | null): number {
  return (selectedEdge?.id === edge.id ? -20 : 0) + relationRank(edge);
}

function uniqueEdges(edges: VisualEdge[]): VisualEdge[] {
  const seen = new Set<string>();
  return edges.filter((edge) => {
    if (seen.has(edge.id)) {
      return false;
    }
    seen.add(edge.id);
    return true;
  });
}

function nodePosition(
  nodeId: string,
  routeCards: { id: string }[],
  codeCards: { id: string }[],
  tableCards: DbInventoryTable[],
): { band: "api" | "code" | "db"; lane: number } | null {
  if (nodeId.startsWith("code:")) {
    const codeId = nodeId.slice("code:".length);
    const routeIndex = routeCards.findIndex((item) => item.id === codeId);
    if (routeIndex >= 0) {
      return { band: "api", lane: routeIndex };
    }
    const codeIndex = codeCards.findIndex((item) => item.id === codeId);
    return codeIndex >= 0 ? { band: "code", lane: codeIndex } : null;
  }
  const tableKey = tableKeyFromNodeId(nodeId);
  if (!tableKey) {
    return null;
  }
  const tableIndex = tableCards.findIndex((table) => dbInventoryTableKey(table) === tableKey);
  return tableIndex >= 0 ? { band: "db", lane: tableIndex } : null;
}

function tableKeyFromNodeId(nodeId: string): string | null {
  if (nodeId.startsWith("db:table:")) {
    return nodeId.slice("db:table:".length);
  }
  if (!nodeId.startsWith("db:column:")) {
    return null;
  }
  const body = nodeId.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 ? body.slice(0, splitIndex) : null;
}

function tableKeyFromFocusedTable(focusId: string): string | null {
  return focusId.startsWith("db:table:") ? focusId.slice("db:table:".length) : null;
}

function relationFocusIdFromMapFocus(focusId: string): string | null {
  return focusId.startsWith("code:") || focusId.startsWith("db:table:") || focusId.startsWith("db:column:") ? focusId : null;
}

function columnLabelFromNodeId(nodeId: string): string | null {
  if (!nodeId.startsWith("db:column:")) {
    return null;
  }
  const body = nodeId.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 ? `${body.slice(0, splitIndex)}.${body.slice(splitIndex + 1)}` : null;
}

function columnRefFromNodeId(nodeId: string): { tableKey: string; columnName: string } | null {
  if (!nodeId.startsWith("db:column:")) {
    return null;
  }
  const body = nodeId.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 && splitIndex < body.length - 1
    ? { tableKey: body.slice(0, splitIndex), columnName: body.slice(splitIndex + 1) }
    : null;
}

function laneCenterX(lane: number): number {
  // ponytail: mirrors fixed CSS lane sizes; measure DOM only if card widths become variable.
  return AT_GUTTER_WIDTH + AT_LANE_PAD_X + lane * (AT_LANE_WIDTH + AT_LANE_GAP) + AT_LANE_WIDTH / 2;
}

function bandCenterPercent(bands: Array<"api" | "code" | "db">, target: "api" | "code" | "db"): number {
  const weights = bands.map((band) => (band === "db" ? 1.6 : 1));
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  let before = 0;
  for (let index = 0; index < bands.length; index += 1) {
    if (bands[index] === target) {
      return ((before + weights[index] / 2) / total) * 100;
    }
    before += weights[index];
  }
  return 50;
}

function relationTone(edge: VisualEdge): RelationTone {
  if (edge.kind.startsWith("candidate")) {
    return "candidate";
  }
  if (edge.kind.startsWith("structural_")) {
    return "typed";
  }
  if (edge.kind === "contains" || edge.kind === "group_contains") {
    return "typed";
  }
  if (edge.kind === "code_flow") {
    return "inferred";
  }
  return edge.evidence.length > 0 ? "confirmed" : "typed";
}

function relationRank(edge: VisualEdge): number {
  const tone = relationTone(edge);
  if (tone === "confirmed") {
    return 0;
  }
  if (tone === "typed") {
    return 1;
  }
  return tone === "candidate" ? 2 : 3;
}

function relationLabel(tone: RelationTone): string {
  if (tone === "confirmed") {
    return "직접";
  }
  if (tone === "typed") {
    return "구조";
  }
  return tone === "candidate" ? "후보" : "이름 단서";
}

function confidenceLabel(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (normalized === "high") return "높음";
  if (normalized === "medium") return "보통";
  if (normalized === "low") return "낮음";
  return value;
}

function edgeKindLabel(edge: VisualEdge): string {
  if (edge.kind.startsWith("candidate")) {
    return "후보 근거";
  }
  if (edge.kind.startsWith("structural_")) {
    return "구조 관계";
  }
  if (edge.kind === "contains" || edge.kind === "group_contains") {
    return "포함 관계";
  }
  if (edge.kind === "db_constraint" || edge.kind === "db_fk") {
    return "DB 제약";
  }
  if (edge.kind === "code_call") {
    return "코드 호출";
  }
  if (edge.kind === "code_handle") {
    return "라우트 처리";
  }
  if (edge.kind === "code_flow") {
    return "이름 단서";
  }
  return "관계";
}

function compactPath(path?: string | null): string | null {
  if (!path) {
    return null;
  }
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  const file = parts[parts.length - 1];
  return file && parts.length > 1 ? `.../${file}` : file ?? null;
}

function isApiItem(item: CodeInventoryItem): boolean {
  const kind = item.kind.trim().toLowerCase();
  return kind === "route" || kind === "api";
}

function columnMeta(column: DbInventoryTable["columns"][number]): string {
  if (column.isPrimaryKey) {
    return "PK";
  }
  if (column.isForeignKey) {
    return "FK";
  }
  return column.dataType ?? "";
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
