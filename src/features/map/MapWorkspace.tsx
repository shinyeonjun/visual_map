import { Database, FolderCog, Layers3, Network, PanelLeftClose, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { EngineRegistry } from "../../types/engine";
import type { VisualMap, VisualNode } from "../../types/visual-map";
import { codeInventoryItemCount } from "../../types/workspace";
import { AppErrorBoundary } from "../../components/common/AppErrorBoundary";
import { focusDbProfileSetup } from "../../components/common/focusSourceSetup";
import { CoverageNotice } from "./CoverageNotice";
import { StructureCanvas } from "./StructureCanvas";
import { MapInspector } from "./MapInspector";
import { MapExplorer } from "./MapExplorer";
import { MapSourcePanel } from "./MapSourcePanel";
import { MapStatusBar } from "./MapStatusBar";
import { MapTopBar } from "./MapTopBar";
import { deriveModules, isDerived } from "./structureModel";
import { targetKindForMode, type TargetItem } from "./targetModel";
import { AnalysisSetupDialog, type AnalysisProgress, type AnalysisSetupChoice } from "./AnalysisSetupDialog";

export function MapWorkspace({
  sourceManagerOpen,
  setSourceManagerOpen,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
  engineError,
  devSlot,
  busyNotice = null,
  analysisSetupWorkspace,
  analysisInitializing = false,
  analysisProgress,
  analysisError = null,
  onStartAnalysis = () => undefined,
  onCancelAnalysis = () => undefined,
  onSaveDbConnection = async () => false,
  dbConnectionError = null,
  onOpenDbConnection = () => undefined,
}: {
  sourceManagerOpen: boolean;
  setSourceManagerOpen: (open: boolean) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  devSlot?: ReactNode;
  busyNotice?: string | null;
  analysisSetupWorkspace?: import("../../types/workspace").Workspace | null;
  analysisInitializing?: boolean;
  analysisProgress?: AnalysisProgress;
  analysisError?: string | null;
  onStartAnalysis?: (choice: AnalysisSetupChoice) => void;
  onCancelAnalysis?: () => void;
  onSaveDbConnection?: () => Promise<boolean>;
  dbConnectionError?: string | null;
  onOpenDbConnection?: () => void;
}) {
  const [explorerOpen, setExplorerOpen] = useState(true);
  const [dbConnectionModalOpen, setDbConnectionModalOpen] = useState(false);
  const [targetReveal, setTargetReveal] = useState<TargetReveal | null>(null);
  const [targetRevealPath, setTargetRevealPath] = useState<string[] | undefined>();
  const [inventorySelection, setInventorySelection] = useState<VisualNode | null>(null);
  const hasAnswerSource =
    codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasSnapshotState =
    hasAnswerSource || visualMapControls.snapshotStaleReasons.length > 0 || Boolean(visualMapControls.snapshotSavedAt);
  const workspaceShellReady = workspaceControls.initialized && hasWorkspace;
  const workspaceReady = workspaceShellReady;
  const inspectorControls = inventorySelection
    ? { ...visualMapControls, selectedNode: inventorySelection, selectedEdge: null }
    : visualMapControls;
  const inspectorVisible = Boolean(inspectorControls.selectedNode || inspectorControls.selectedEdge);
  const sourcePanelOpen = sourceManagerOpen && workspaceShellReady;
  const explorerPanelOpen = explorerOpen && workspaceShellReady && !sourcePanelOpen;
  const workspaceId = workspaceControls.currentWorkspace?.id ?? null;
  // Areas outlive any one selection: remembering the last overview is what lets
  // the canvas keep the project on screen while something inside it is open.
  const overviewAreasRef = useRef<VisualNode[]>([]);
  const overviewMapRef = useRef<VisualMap | null>(null);
  const committedMapForAreas = visualMapControls.currentMap;
  if (committedMapForAreas) {
    const areas = committedMapForAreas.nodes.filter(
      (node) => node.kind === "group-domain" && (node.depth ?? 0) === 0 && !node.parentId,
    );
    // Detail projections carry sibling groups for context, but their order and
    // edge set are not the overview. Keep the last committed overview as the
    // stable canvas frame while a group is open.
    if (areas.length > 0 && (!committedMapForAreas.focus.startsWith("group:") || !overviewMapRef.current)) {
      overviewAreasRef.current = areas;
      overviewMapRef.current = committedMapForAreas;
    }
  }
  const overviewAreas = overviewAreasRef.current;
  const overviewMap = overviewMapRef.current;
  const visibleMode =
    visualMapControls.loading && visualMapControls.currentMap
      ? visualMapControls.currentMap.mode
      : visualMapControls.mode;
  const committedAnswerFocus = answerFocusId(visualMapControls);
  const focusedArchitectureGroup = visualMapControls.currentMap?.focus.startsWith("group:")
    ? (visualMapControls.currentMap.nodes.find(
        (node) => node.id === visualMapControls.currentMap?.focus && node.kind === "group-domain",
      ) ?? null)
    : null;
  const answerHasTarget =
    ["api-flow", "search-focus", "table-usage", "column-impact"].includes(visibleMode) && Boolean(committedAnswerFocus);
  // Requesting a target IS requesting its answer — the status must not wait for
  // the surface to flip, or descending from the overview reads as silence.
  const answerLoading =
    workspaceReady &&
    visualMapControls.loading &&
    Boolean(targetKindForMode(visualMapControls.mode)) &&
    Boolean(validAnswerFocus(visualMapControls.focusId));
  const answerReady = workspaceReady && !visualMapControls.loading && answerHasTarget && Boolean(committedAnswerFocus);
  const answerStatus = answerLoading
    ? "선택한 대상 분석 중"
    : answerReady && committedAnswerFocus
      ? `답 준비 완료: ${answerTargetTitle(visualMapControls, committedAnswerFocus)}`
      : "";
  useLayoutEffect(() => {
    setExplorerOpen(Boolean(workspaceId));
    setTargetReveal(null);
    setTargetRevealPath(undefined);
    setInventorySelection(null);
    overviewAreasRef.current = [];
    overviewMapRef.current = null;
  }, [workspaceId]);

  // One navigation axis instead of a mode toggle: the breadcrumb root is the
  // whole-structure map, and picking any target expands it inside that map.
  function showOverview() {
    setTargetReveal(null);
    setTargetRevealPath(undefined);
    setInventorySelection(null);
    visualMapControls.clearSelection();
    visualMapControls.showMode("atlas", null);
  }

  function openTargetInCanvas(item: TargetItem) {
    setInventorySelection(null);
    visualMapControls.clearSelection();
    const area = findAreaForTarget(item, overviewAreas);
    if (!area) {
      setInventorySelection(targetAsNode(item));
      return;
    }
    setTargetReveal({ item, areaId: area.id, moduleId: null });
    setTargetRevealPath([area.id]);
    visualMapControls.showMode("atlas", area.id);
  }

  function closeInspector() {
    setInventorySelection(null);
    visualMapControls.clearSelection();
  }

  function makeRoomForLeftPanel() {
    if (window.innerWidth <= 1100 && inspectorVisible) closeInspector();
  }

  function openDbConnectionModal() {
    onOpenDbConnection();
    setSourceManagerOpen(false);
    setDbConnectionModalOpen(true);
  }

  async function saveDbConnection() {
    if (await onSaveDbConnection()) {
      setDbConnectionModalOpen(false);
    }
  }

  useEffect(() => {
    if (!hasWorkspace && sourceManagerOpen) {
      setSourceManagerOpen(false);
    }
  }, [hasWorkspace, sourceManagerOpen, setSourceManagerOpen]);

  useEffect(() => {
    if (window.innerWidth <= 1100 && inspectorVisible) {
      setExplorerOpen(false);
      if (sourceManagerOpen) setSourceManagerOpen(false);
    }
  }, [inspectorVisible, sourceManagerOpen, setSourceManagerOpen]);

  useEffect(() => {
    const map = visualMapControls.currentMap;
    if (!targetReveal || !map || visualMapControls.loading) return;
    const selectTarget = (item: TargetItem) => {
      const target = findTargetNode(item, map.nodes);
      if (target) {
        setInventorySelection(null);
        visualMapControls.selectNode(target);
      } else {
        visualMapControls.clearSelection();
        setInventorySelection(targetAsNode(item));
      }
    };

    if (!targetReveal.moduleId && map.focus === targetReveal.areaId) {
      const module = findTargetModule(targetReveal, map.nodes);
      if (module) {
        setTargetRevealPath([targetReveal.areaId, module.id]);
        if (isDerived(module)) {
          selectTarget(targetReveal.item);
          setTargetReveal(null);
          return;
        }
        setTargetReveal((current) => current && { ...current, moduleId: module.id });
        visualMapControls.showMode("atlas", module.id);
        return;
      }
      selectTarget(targetReveal.item);
      setTargetReveal(null);
    }

    if (targetReveal.moduleId && map.focus === targetReveal.moduleId) {
      selectTarget(targetReveal.item);
      setTargetReveal(null);
    }
  }, [targetReveal, visualMapControls]);

  const leftPanel = sourcePanelOpen ? "sources" : explorerPanelOpen ? "layers" : "closed";

  return (
    <div
      className="map-app-shell"
      data-view="map"
      data-left-panel={leftPanel}
      data-inspector={inspectorVisible ? "open" : "closed"}
    >
      <MapTopBar
        key={workspaceId ?? "no-workspace"}
        sourceManagerOpen={sourcePanelOpen}
        onToggleSourceManager={() => {
          if (!sourcePanelOpen) makeRoomForLeftPanel();
          setExplorerOpen(false);
          setSourceManagerOpen(!sourcePanelOpen);
        }}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />

      <p
        className="map-live-status"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        data-state={answerLoading ? "loading" : answerReady ? "ready" : "idle"}
      >
        {answerStatus}
      </p>

      <main className="map-workspace" aria-label="백엔드 구조 지도">
        <section className="map-canvas-layer" aria-label="비주얼 맵">
          {busyNotice ? (
            <p className="map-toast" role="status" aria-live="polite">
              {busyNotice}
            </p>
          ) : null}

          {workspaceShellReady && hasSnapshotState ? (
            <div className="map-coverage-overlay">
              <CoverageNotice
                codeInventory={workspaceControls.codeInventory}
                onOpenSources={() => {
                  setExplorerOpen(false);
                  setSourceManagerOpen(true);
                }}
              />
            </div>
          ) : null}

          {!workspaceControls.initialized || !hasWorkspace ? (
            <section className="map-onboarding" aria-labelledby="map-onboarding-title">
              <header className="map-onboarding-heading">
                <span className="map-onboarding-mark" aria-hidden="true">
                  <Network size={21} />
                </span>
                <div>
                  <h1 id="map-onboarding-title">
                    {workspaceControls.initialized ? "프로젝트를 지도에 올리세요" : "프로젝트를 확인하고 있습니다"}
                  </h1>
                  <p>저장소를 연결하면 이 캔버스에서 API·코드·데이터 흐름을 단계별로 펼쳐 볼 수 있습니다.</p>
                </div>
              </header>
              <MapSourcePanel
                workspaceControls={workspaceControls}
                dbProfileControls={dbProfileControls}
                visualMapControls={visualMapControls}
                onEditDbConnection={openDbConnectionModal}
              />
            </section>
          ) : (
            <AppErrorBoundary fallback={<SurfaceFailure />}>
              <StructureCanvas
                areas={overviewAreas}
                openId={focusedArchitectureGroup?.id ?? null}
                revealPath={targetRevealPath}
                edges={overviewMap?.edges ?? []}
                map={visualMapControls.currentMap}
                onSelectEdge={(edge) => {
                  setInventorySelection(null);
                  visualMapControls.selectEdge(edge);
                }}
                onSelectNode={(node) => {
                  setInventorySelection(null);
                  visualMapControls.selectNode(node);
                }}
                onExpandNode={(node) => visualMapControls.showMode("atlas", node.id)}
                onCollapse={showOverview}
                onExpandArea={(node) =>
                  visualMapControls.showMode("atlas", node.id === focusedArchitectureGroup?.id ? null : node.id)
                }
              />
            </AppErrorBoundary>
          )}
        </section>

        {workspaceShellReady ? (
          <nav className="map-tool-rail" aria-label="캔버스 도구">
            <button
              type="button"
              className={explorerPanelOpen ? "active" : ""}
              onClick={() => {
                if (!explorerPanelOpen) makeRoomForLeftPanel();
                setSourceManagerOpen(false);
                setExplorerOpen(!explorerPanelOpen);
              }}
              aria-label={explorerPanelOpen ? "레이어 닫기" : "레이어 열기"}
              aria-pressed={explorerPanelOpen}
            >
              {explorerPanelOpen ? <PanelLeftClose size={18} /> : <Layers3 size={18} />}
            </button>
            <button
              type="button"
              className={sourcePanelOpen ? "active" : ""}
              onClick={() => {
                if (!sourcePanelOpen) makeRoomForLeftPanel();
                setExplorerOpen(false);
                setSourceManagerOpen(!sourcePanelOpen);
              }}
              aria-label="소스 관리"
              aria-pressed={sourcePanelOpen}
            >
              <FolderCog size={18} />
            </button>
            <button
              type="button"
              onClick={() => {
                makeRoomForLeftPanel();
                setExplorerOpen(false);
                setSourceManagerOpen(true);
                window.requestAnimationFrame(() => focusDbProfileSetup(dbProfileControls));
              }}
              aria-label="데이터베이스 연결"
            >
              <Database size={18} />
            </button>
          </nav>
        ) : null}

        {explorerPanelOpen ? (
          <aside className="map-floating-panel map-explorer-panel" aria-label="레이어 탐색기">
            <header className="map-floating-panel-head">
              <span>
                <strong>레이어</strong>
                <small>전체 항목에서 캔버스 위치 찾기</small>
              </span>
              <button type="button" onClick={() => setExplorerOpen(false)} aria-label="레이어 닫기">
                <X size={16} />
              </button>
            </header>
            <MapExplorer
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={inspectorControls}
              onOpenDatabase={() => {
                setExplorerOpen(false);
                setSourceManagerOpen(true);
                window.requestAnimationFrame(() => focusDbProfileSetup(dbProfileControls));
              }}
              onOpenSources={() => {
                setExplorerOpen(false);
                setSourceManagerOpen(true);
              }}
              onSelectTarget={openTargetInCanvas}
            />
          </aside>
        ) : null}

        {sourcePanelOpen ? (
          <aside className="map-floating-panel map-source-panel" aria-label="소스 관리">
            <header className="map-floating-panel-head">
              <span>
                <strong>소스 관리</strong>
                <small>저장소와 데이터베이스 연결</small>
              </span>
              <button type="button" onClick={() => setSourceManagerOpen(false)} aria-label="소스 관리 닫기">
                <X size={16} />
              </button>
            </header>
            <MapSourcePanel
              workspaceControls={workspaceControls}
              dbProfileControls={dbProfileControls}
              visualMapControls={visualMapControls}
              onEditDbConnection={openDbConnectionModal}
            />
          </aside>
        ) : null}

        {inspectorVisible ? (
          <aside className="map-floating-panel map-inspector-panel" aria-label="선택 항목 근거">
            <AppErrorBoundary fallback={<SurfaceFailure />}>
              <MapInspector
                onClose={closeInspector}
                title="근거"
                variant="full"
                workspaceControls={workspaceControls}
                dbProfileControls={dbProfileControls}
                visualMapControls={inspectorControls}
                focusedGroup={focusedArchitectureGroup}
              />
            </AppErrorBoundary>
          </aside>
        ) : null}
      </main>

      {workspaceShellReady && hasSnapshotState ? (
        <MapStatusBar
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
          engineRegistry={engineRegistry}
          engineError={engineError}
          devSlot={devSlot}
        />
      ) : null}

      {analysisSetupWorkspace && (
        <AnalysisSetupDialog
          workspace={analysisSetupWorkspace}
          dbProfileControls={dbProfileControls}
          busy={analysisInitializing}
          progress={analysisProgress}
          error={analysisError}
          onStart={onStartAnalysis}
          onCancel={onCancelAnalysis}
        />
      )}
      {dbConnectionModalOpen && workspaceControls.currentWorkspace && (
        <AnalysisSetupDialog
          variant="connection"
          workspace={workspaceControls.currentWorkspace}
          dbProfileControls={dbProfileControls}
          busy={dbProfileControls.saving}
          error={dbConnectionError}
          onSave={saveDbConnection}
          onCancel={() => setDbConnectionModalOpen(false)}
        />
      )}
    </div>
  );
}

function SurfaceFailure() {
  return (
    <section className="workspace-initializing" role="alert" data-surface-error="true">
      <strong>이 화면을 표시하지 못했습니다</strong>
      <span>저장된 분석 결과는 유지됩니다. 다른 화면을 선택하거나 앱을 다시 열어 주세요.</span>
    </section>
  );
}

function answerFocusId(visualMapControls: VisualMapControls): string | null {
  const value =
    visualMapControls.loading && visualMapControls.currentMap
      ? visualMapControls.currentMap.focus
      : (visualMapControls.focusId ?? visualMapControls.currentMap?.focus ?? null);
  return validAnswerFocus(value);
}

function validAnswerFocus(value: string | null | undefined): string | null {
  return value && value !== "narrow-focus" && value !== "overview" && !value.startsWith("group:") ? value : null;
}

function findAreaForTarget(item: TargetItem, areas: VisualNode[]): VisualNode | null {
  const pathSegments = normalizePath(item.sourcePath).split("/").filter(Boolean).map(normalizeCanvasText);
  const targetText = normalizeCanvasText(
    [item.title, item.meta, item.sourcePath, item.group].filter(Boolean).join(" "),
  );
  if (!targetText) return null;
  let best: { area: VisualNode; score: number } | null = null;
  for (const area of areas) {
    const areaTitle = normalizeCanvasText(area.title);
    const pathIndex = pathSegments.lastIndexOf(areaTitle);
    const labels = [
      area.title,
      ...(area.metrics?.topApi ?? []),
      ...(area.metrics?.topCode ?? []),
      ...(area.metrics?.topDb ?? []),
    ]
      .map(normalizeCanvasText)
      .filter((label) => label.length >= 3);
    const textScore = labels.reduce((highest, label) => {
      if (targetText === label) return Math.max(highest, 100);
      if (targetText.includes(label)) return Math.max(highest, 70 + Math.min(label.length, 20));
      if (label.includes(targetText)) return Math.max(highest, 45);
      return highest;
    }, 0);
    const score = pathIndex < 0 ? textScore : 200 + pathIndex * 10 + areaTitle.length;
    if (score > (best?.score ?? 0)) best = { area, score };
  }
  return best?.area ?? null;
}

type TargetReveal = { item: TargetItem; areaId: string; moduleId: string | null };

function findTargetModule(reveal: TargetReveal, nodes: VisualNode[]): VisualNode | null {
  const sourcePath = normalizePath(reveal.item.sourcePath);
  const engineGroups = nodes.filter((node) => node.kind === "group-domain" && node.parentId === reveal.areaId);
  const groups =
    engineGroups.length > 0
      ? engineGroups
      : deriveModules(
          reveal.areaId,
          nodes.filter((node) => node.id !== reveal.areaId && node.kind !== "group-domain"),
        ).modules;
  const [best] = groups
    .map((group) => {
      const groupPath = normalizePath(group.location?.path);
      const title = normalizeCanvasText(group.title);
      const score =
        groupPath && sourcePath.includes(groupPath)
          ? groupPath.length + 100
          : title && normalizeCanvasText(sourcePath).includes(title)
            ? title.length
            : 0;
      return { group, score };
    })
    .sort((left, right) => right.score - left.score);
  return best?.score ? best.group : null;
}

function findTargetNode(item: TargetItem, nodes: VisualNode[]): VisualNode | null {
  const exact = nodes.find((node) => node.id === item.focusId);
  if (exact) return exact;
  const sourcePath = normalizePath(item.sourcePath);
  return nodes.find((node) => sourcePath && normalizePath(node.location?.path) === sourcePath) ?? null;
}

function targetAsNode(item: TargetItem): VisualNode {
  return {
    id: item.focusId,
    kind: item.kind === "api" ? "route" : item.kind,
    title: item.title,
    subtitle: item.meta,
    layer: item.kind === "table" || item.kind === "column" ? "database" : item.kind === "api" ? "api" : "code",
    source: "inventory",
    location: item.sourcePath ? { path: item.sourcePath, line: null, column: null } : null,
  };
}

function normalizePath(value: string | null | undefined): string {
  return (value ?? "").replace(/\\/g, "/").toLocaleLowerCase("ko-KR");
}

function normalizeCanvasText(value: string): string {
  return value.toLocaleLowerCase("ko-KR").replace(/[\s/_:.,()[\]{}-]+/g, "");
}

function answerTargetTitle(visualMapControls: VisualMapControls, focusId: string): string {
  const map = visualMapControls.currentMap;
  return (
    map?.nodes.find((node) => node.id === focusId)?.title ??
    map?.apiReading?.subject ??
    map?.reviewBoard?.subject ??
    focusId.replace(/^(?:code|db):/, "")
  );
}
