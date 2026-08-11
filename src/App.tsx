import {
  AddRegular as Plus,
  ArrowSyncRegular as LoaderCircle,
  BracesRegular as Braces,
  ChatRegular as MessageSquareText,
  ChevronDownRegular as ChevronDown,
  ChevronRightRegular as ChevronRight,
  CodeRegular as Code,
  DeleteRegular as Delete,
  DismissCircleRegular as StopCircle,
  ErrorCircleRegular as CircleAlert,
  FolderOpenRegular as FolderOpen,
  FolderRegular as FolderCode,
  HomeRegular as Home,
  MapRegular as Map,
  OrganizationRegular as Network,
  PanelRightRegular as PanelRight,
  SearchRegular as Search,
  WarningRegular as TriangleAlert,
  WindowConsoleRegular as TerminalSquare,
} from "@fluentui/react-icons";
import { listen } from "@tauri-apps/api/event";
import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  Textarea,
} from "@fluentui/react-components";
import { useEffect, useMemo, useRef, useState } from "react";
import { Inspector } from "./map/Inspector";
import { MapCanvas } from "./map/MapCanvas";
import type { TraceView } from "./map/MapCanvas";
import { PREVIEW_MAP, previewSelectionFor } from "./map/previewMap";
import { flattenAreas, owningAreaId } from "./map/types";
import type { EvidenceRef, MapArea, MapNode, MapView, Selection } from "./map/types";
import {
  defaultEffortFor,
  defaultModelFor,
  effortLabel,
  effortOptionsFor,
  modelFor,
  modelsFor,
} from "./providerModels";
import type {
  AiProviderAvailability,
  AnalysisProgressEvent,
  CommandError,
  EngineRegistry,
  FactGraphStatus,
  ProviderKind,
  ReasoningEffort,
  Workspace,
} from "./contracts";
import {
  analyzeWorkspace,
  cancelWorkspaceAnalysis,
  chooseRepositoryFolder,
  createWorkspace,
  deleteWorkspace,
  getEngineRegistry,
  getFactGraphStatus,
  getMapSelection,
  getMapView,
  hasDesktopRuntime,
  listProviders,
  listWorkspaces,
  openSourceLocation,
  setWorkspaceProvider,
} from "./desktop";

type LoadState = "loading" | "ready" | "error";
type DockTab = "evidence" | "chat";
const AI_EVIDENCE_CONSENT_VERSION = "v1";

const DEVELOPMENT_PREVIEW =
  import.meta.env.DEV &&
  typeof window !== "undefined" &&
  !hasDesktopRuntime() &&
  new URLSearchParams(window.location.search).has("preview");

const PREVIEW_WORKSPACE: Workspace = {
  schemaVersion: 2,
  id: "preview-commerce-platform",
  name: "commerce-platform",
  repoPath: "D:\\workspace\\commerce-platform",
  provider: { kind: "codex", model: "gpt-5.6-sol", effort: "high" },
  createdAt: Date.UTC(2026, 7, 9),
  updatedAt: Date.UTC(2026, 7, 9),
};

const PREVIEW_FACT_STATUS: FactGraphStatus = {
  schemaVersion: 1,
  snapshotId: "a1b2c3d4",
  sourceRevision: "main",
  nodeCount: 214,
  edgeCount: 109,
  evidenceCount: 87,
  coverageCount: 42,
};

const PREVIEW_ENGINE_REGISTRY: EngineRegistry = {
  mode: "dev",
  engineDir: "design-preview",
  engines: [
    {
      id: "tree-sitter",
      label: "Tree-sitter",
      role: "syntax facts",
      available: true,
      integrity: "preview",
      error: null,
    },
    {
      id: "flowline",
      label: "Flowline Static Analyzer",
      role: "relations and traces",
      available: true,
      integrity: "preview",
      error: null,
    },
  ],
};

interface MapSearchResult {
  id: string;
  kind: "영역" | "구현";
  label: string;
  detail: string;
}

export default function App() {
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [providers, setProviders] = useState<AiProviderAvailability[]>([]);
  const [engineRegistry, setEngineRegistry] = useState<EngineRegistry | null>(null);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [factStatus, setFactStatus] = useState<FactGraphStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [providerOpen, setProviderOpen] = useState(false);
  const [analysisConsentTarget, setAnalysisConsentTarget] = useState<Workspace | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Workspace | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mapView, setMapView] = useState<MapView | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [traceAreaId, setTraceAreaId] = useState<string | null>(null);
  const [traceView, setTraceView] = useState<TraceView | null>(null);
  const [analyzingWorkspaceId, setAnalyzingWorkspaceId] = useState<string | null>(null);
  const [cancellingWorkspaceId, setCancellingWorkspaceId] = useState<string | null>(null);
  const [analysisProgress, setAnalysisProgress] = useState<AnalysisProgressEvent | null>(null);
  const [analysisStartedAt, setAnalysisStartedAt] = useState<number | null>(null);
  const [dockTab, setDockTab] = useState<DockTab>("chat");
  const activeWorkspaceIdRef = useRef<string | null>(null);
  const cancellationRequests = useRef(new Set<string>());

  const activeWorkspace = useMemo(
    () => workspaces.find((workspace) => workspace.id === activeWorkspaceId) ?? null,
    [activeWorkspaceId, workspaces],
  );

  useEffect(() => {
    activeWorkspaceIdRef.current = activeWorkspaceId;
  }, [activeWorkspaceId]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([listWorkspaces(), listProviders(), getEngineRegistry()])
      .then(([workspaceRows, providerRows, registry]) => {
        if (cancelled) return;
        const availableWorkspaces = DEVELOPMENT_PREVIEW ? [PREVIEW_WORKSPACE] : workspaceRows;
        setWorkspaces(availableWorkspaces);
        setProviders(providerRows);
        setEngineRegistry(registry);
        const firstWorkspaceId = availableWorkspaces[0]?.id ?? null;
        // Keep the stale-request guard in sync in the same turn as the state
        // update. Updating it in a later effect leaves a real window where the
        // newly visible workspace can start an analysis whose result is then
        // mistaken for a result from an inactive workspace.
        activeWorkspaceIdRef.current = firstWorkspaceId;
        setActiveWorkspaceId(firstWorkspaceId);
        setLoadState("ready");
      })
      .catch((reason: unknown) => {
        if (cancelled) return;
        setError(errorMessage(reason));
        setLoadState("error");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    setSelectedId(null);
    setSelection(null);
    setMapView(null);
    setAnalysisProgress(null);
    if (!activeWorkspaceId || !hasDesktopRuntime()) {
      setFactStatus(null);
      return;
    }
    let cancelled = false;
    setFactStatus(null);
    Promise.all([getFactGraphStatus(activeWorkspaceId), getMapView(activeWorkspaceId)])
      .then(([status, view]) => {
        if (cancelled) return;
        setFactStatus(status);
        setMapView(view);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(errorMessage(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [activeWorkspaceId]);

  useEffect(() => {
    if (!hasDesktopRuntime()) return;
    let disposed = false;
    let stop: (() => void) | null = null;
    void listen<AnalysisProgressEvent>("analysis-progress", ({ payload }) => {
      if (!disposed && payload.workspaceId === activeWorkspaceIdRef.current) {
        setAnalysisProgress(payload);
      }
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch((reason: unknown) => {
        if (!disposed) setError(errorMessage(reason));
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  /*
    Nothing is drawn until the engine publishes a snapshot. The preview map
    stands in during development so the canvas can be worked on, and it says
    so on screen — a map that cannot be told apart from an analysed one is
    exactly the failure this product must not have.
  */
  const previewing = import.meta.env.DEV && !hasDesktopRuntime() && Boolean(activeWorkspace) && !factStatus?.snapshotId;
  const displayedMap: MapView | null = previewing ? PREVIEW_MAP : mapView;
  const displayedFactStatus = previewing ? PREVIEW_FACT_STATUS : factStatus;
  const displayedEngineRegistry = previewing ? PREVIEW_ENGINE_REGISTRY : engineRegistry;

  useEffect(() => {
    if (previewing) {
      setSelection(previewSelectionFor(selectedId));
      return;
    }
    if (!activeWorkspaceId || !selectedId || !mapView || !hasDesktopRuntime()) {
      setSelection(null);
      return;
    }
    let cancelled = false;
    getMapSelection(activeWorkspaceId, selectedId)
      .then((detail) => {
        if (!cancelled) setSelection(detail);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(errorMessage(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [activeWorkspaceId, mapView, previewing, selectedId]);

  useEffect(() => {
    if (selection) setDockTab("evidence");
  }, [selection]);

  /*
    The flow view holds the paths of the area it was opened for, captured once.
    Selecting a step inside it must move the evidence panel without rebuilding
    the drawing under the reader's cursor, so this deliberately does not follow
    `selectedId`.
  */
  useEffect(() => {
    if (!traceAreaId) {
      setTraceView(null);
      return;
    }
    const area = flattenAreas(displayedMap?.areas ?? []).find((item) => item.id === traceAreaId);
    if (!area) {
      setTraceView(null);
      return;
    }
    if (previewing) {
      // The preview area carries its own chain; it is one path, not a set.
      const traces = area.trace ? [{ id: area.trace.id, state: area.trace.state, steps: area.nodes }] : [];
      setTraceView({ areaId: area.id, title: area.name, summary: area.summary, traces });
      return;
    }
    if (!activeWorkspaceId || !hasDesktopRuntime()) return;
    let cancelled = false;
    getMapSelection(activeWorkspaceId, traceAreaId)
      .then((detail) => {
        if (cancelled) return;
        setTraceView({
          areaId: area.id,
          title: detail?.title ?? area.name,
          summary: area.summary,
          traces: detail?.traces ?? [],
        });
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(errorMessage(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [activeWorkspaceId, displayedMap, previewing, traceAreaId]);

  function openTrace(areaId: string) {
    setTraceAreaId(areaId);
    setSelectedId(areaId);
  }

  function closeTrace() {
    setTraceAreaId(null);
  }

  /** The panel describes whatever is picked; its flow belongs to the owning area. */
  function openTraceForSelection() {
    const owner = displayedMap ? owningAreaId(displayedMap, selectedId) : null;
    if (owner) openTrace(owner);
  }

  function requestAnalysis() {
    const workspace = activeWorkspace;
    if (!workspace || analyzingWorkspaceId) return;
    if (!workspace.provider || hasAiEvidenceConsent(workspace)) {
      void runAnalysis(workspace);
      return;
    }
    setAnalysisConsentTarget(workspace);
  }

  function acceptAnalysisConsent(workspace: Workspace) {
    rememberAiEvidenceConsent(workspace);
    setAnalysisConsentTarget(null);
    if (activeWorkspaceIdRef.current === workspace.id) {
      void runAnalysis(workspace);
    }
  }

  async function runAnalysis(workspace: Workspace) {
    if (!workspace || analyzingWorkspaceId) return;
    if (!workspace.provider) {
      setError("코드 의미 지도를 만들 Codex 또는 Claude 모델을 먼저 설정하세요.");
      setProviderOpen(true);
      return;
    }
    setError(null);
    setAnalysisProgress({
      workspaceId: workspace.id,
      stage: "starting",
      completed: 0,
      total: 1,
      label: "캐시 없이 코드를 처음부터 분석합니다",
    });
    setAnalysisStartedAt(Date.now());
    setAnalyzingWorkspaceId(workspace.id);
    try {
      const result = await analyzeWorkspace(workspace.id, "fresh");
      if (activeWorkspaceIdRef.current !== workspace.id) return;
      setFactStatus(result.factGraph);
      const view = await getMapView(workspace.id);
      if (activeWorkspaceIdRef.current !== workspace.id) return;
      setMapView(view);
      setSelectedId(null);
      setSelection(null);
      if (result.semanticError) {
        setError(`코드 사실은 저장됐지만 의미 지도를 만들지 못했습니다: ${result.semanticError}`);
      } else if (!view) {
        setError("분석은 끝났지만 현재 snapshot과 일치하는 의미 지도가 없습니다.");
      }
    } catch (reason: unknown) {
      if (activeWorkspaceIdRef.current === workspace.id && !cancellationRequests.current.has(workspace.id)) {
        setError(errorMessage(reason));
      }
    } finally {
      cancellationRequests.current.delete(workspace.id);
      setAnalyzingWorkspaceId((current) => (current === workspace.id ? null : current));
      setCancellingWorkspaceId((current) => (current === workspace.id ? null : current));
      setAnalysisStartedAt(null);
    }
  }

  async function cancelAnalysis() {
    const workspaceId = analyzingWorkspaceId;
    if (!workspaceId || cancellingWorkspaceId) return;
    cancellationRequests.current.add(workspaceId);
    setCancellingWorkspaceId(workspaceId);
    try {
      const accepted = await cancelWorkspaceAnalysis(workspaceId);
      if (!accepted) {
        cancellationRequests.current.delete(workspaceId);
        setCancellingWorkspaceId(null);
        setError("이미 분석이 끝났거나 취소할 작업을 찾지 못했습니다.");
      }
    } catch (reason: unknown) {
      cancellationRequests.current.delete(workspaceId);
      setCancellingWorkspaceId(null);
      setError(errorMessage(reason));
    }
  }

  async function removeWorkspace(workspace: Workspace) {
    if (analyzingWorkspaceId === workspace.id) return;
    try {
      await deleteWorkspace(workspace.id);
      forgetAiEvidenceConsent(workspace);
      const remaining = workspaces.filter((item) => item.id !== workspace.id);
      setWorkspaces(remaining);
      if (activeWorkspaceIdRef.current === workspace.id) {
        const nextId = remaining[0]?.id ?? null;
        activeWorkspaceIdRef.current = nextId;
        setActiveWorkspaceId(nextId);
      }
      setDeleteTarget(null);
    } catch (reason: unknown) {
      setError(errorMessage(reason));
    }
  }

  function openEvidence(evidence: EvidenceRef) {
    const workspace = activeWorkspace;
    if (!workspace) return;
    void openSourceLocation(workspace.id, evidence.path, evidence.line ?? null, null, "vscode").catch(
      (reason: unknown) => setError(errorMessage(reason)),
    );
  }

  function upsertWorkspace(workspace: Workspace) {
    setWorkspaces((current) => {
      const without = current.filter((item) => item.id !== workspace.id);
      return [workspace, ...without];
    });
    activeWorkspaceIdRef.current = workspace.id;
    setActiveWorkspaceId(workspace.id);
  }

  function selectWorkspace(workspaceId: string) {
    activeWorkspaceIdRef.current = workspaceId;
    setActiveWorkspaceId(workspaceId);
  }

  return (
    <div className="app-shell">
      <Header
        workspace={activeWorkspace}
        engineRegistry={displayedEngineRegistry}
        status={displayedFactStatus}
        mapView={displayedMap}
        selection={selection}
        onSelect={setSelectedId}
        onOpenProvider={() => setProviderOpen(true)}
        onAnalyze={requestAnalysis}
        onCancel={() => void cancelAnalysis()}
        analyzing={Boolean(activeWorkspaceId) && analyzingWorkspaceId === activeWorkspaceId}
        cancelling={Boolean(activeWorkspaceId) && cancellingWorkspaceId === activeWorkspaceId}
        analysisProgress={analysisProgress}
      />
      <div className="workbench">
        <NavigationRail
          workspaces={workspaces}
          activeWorkspaceId={activeWorkspaceId}
          areas={displayedMap?.areas ?? []}
          selectedId={selectedId}
          onSelect={selectWorkspace}
          onSelectArea={setSelectedId}
          onCreate={() => setCreateOpen(true)}
          onDelete={setDeleteTarget}
          analyzingWorkspaceId={analyzingWorkspaceId}
        />
        <main className="canvas-stage" aria-label="코드베이스 구조 지도">
          {loadState === "loading" ? (
            <CenteredState icon={<LoaderCircle className="spin" />} title="작업공간을 불러오는 중" />
          ) : loadState === "error" ? (
            <CenteredState icon={<CircleAlert />} title="작업공간을 열지 못했습니다" detail={error ?? undefined} />
          ) : displayedMap ? (
            <>
              {previewing ? <PreviewBanner /> : null}
              <MapCanvas
                view={displayedMap}
                selectedId={selectedId}
                onSelect={setSelectedId}
                traceView={traceView}
                onOpenTrace={openTrace}
                onCloseTrace={closeTrace}
              />
              {activeWorkspace && analyzingWorkspaceId === activeWorkspace.id ? (
                <AnalysisStatus
                  variant="overlay"
                  progress={analysisProgress}
                  startedAt={analysisStartedAt}
                  cancelling={cancellingWorkspaceId === activeWorkspace.id}
                  onCancel={() => void cancelAnalysis()}
                />
              ) : null}
            </>
          ) : !activeWorkspace ? (
            <EmptyCanvas onCreate={() => setCreateOpen(true)} />
          ) : (
            <WorkspaceCanvas
              workspace={activeWorkspace}
              status={factStatus}
              analyzing={analyzingWorkspaceId === activeWorkspace.id}
              progress={analysisProgress}
              startedAt={analysisStartedAt}
              onAnalyze={requestAnalysis}
              onCancel={() => void cancelAnalysis()}
              cancelling={cancellingWorkspaceId === activeWorkspace.id}
            />
          )}
        </main>
        <aside className="detail-column">
          <div className="detail-head">
            <div>
              <span>{selection ? "선택 상세" : "워크스페이스"}</span>
              <strong>{selection?.title ?? activeWorkspace?.name ?? "선택 없음"}</strong>
            </div>
            <PanelRight fontSize={17} aria-hidden="true" />
          </div>
          <div className="dock-tabs" role="tablist" aria-label="오른쪽 패널">
            <button
              type="button"
              role="tab"
              aria-selected={dockTab === "evidence"}
              className={dockTab === "evidence" ? "active" : undefined}
              onClick={() => setDockTab("evidence")}
            >
              <Code fontSize={15} /> 근거
              {selection?.evidence.length ? <span>{selection.evidence.length}</span> : null}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={dockTab === "chat"}
              className={dockTab === "chat" ? "active" : undefined}
              onClick={() => setDockTab("chat")}
            >
              <MessageSquareText fontSize={15} /> 대화
            </button>
          </div>
          <div className="dock-content">
            {dockTab === "evidence" ? (
              <Inspector
                selection={selection}
                onHighlight={setSelectedId}
                onOpenEvidence={openEvidence}
                onOpenTrace={traceView ? undefined : openTraceForSelection}
              />
            ) : (
              <ChatPanel
                workspace={activeWorkspace}
                status={displayedFactStatus}
                selection={selection}
                previewing={previewing}
              />
            )}
          </div>
        </aside>
      </div>
      <StatusLedger
        workspace={activeWorkspace}
        engineRegistry={displayedEngineRegistry}
        status={displayedFactStatus}
        previewing={previewing}
        analyzing={Boolean(activeWorkspace) && analyzingWorkspaceId === activeWorkspace?.id}
      />
      {error && loadState !== "error" ? (
        <ErrorNotice key={error} message={error} onClose={() => setError(null)} />
      ) : null}
      {createOpen ? (
        <CreateWorkspaceDialog
          onClose={() => setCreateOpen(false)}
          onCreated={(workspace) => {
            upsertWorkspace(workspace);
            setCreateOpen(false);
          }}
          onError={setError}
        />
      ) : null}
      {providerOpen && activeWorkspace ? (
        <ProviderDialog
          workspace={activeWorkspace}
          providers={providers}
          onClose={() => setProviderOpen(false)}
          onSaved={(workspace) => {
            upsertWorkspace(workspace);
            setProviderOpen(false);
          }}
          onError={setError}
        />
      ) : null}
      {deleteTarget ? (
        <DeleteWorkspaceDialog
          workspace={deleteTarget}
          onClose={() => setDeleteTarget(null)}
          onConfirm={() => void removeWorkspace(deleteTarget)}
        />
      ) : null}
      {analysisConsentTarget ? (
        <AiEvidenceConsentDialog
          workspace={analysisConsentTarget}
          onClose={() => setAnalysisConsentTarget(null)}
          onConfirm={() => acceptAnalysisConsent(analysisConsentTarget)}
        />
      ) : null}
    </div>
  );
}

function aiEvidenceConsentKey(workspace: Workspace): string | null {
  if (!workspace.provider) return null;
  return aiEvidenceConsentStorageKey(workspace.id, workspace.provider.kind);
}

function aiEvidenceConsentStorageKey(workspaceId: string, providerKind: ProviderKind): string {
  return `codebase-workspace.ai-source-evidence-consent.${AI_EVIDENCE_CONSENT_VERSION}:${workspaceId}:${providerKind}`;
}

function hasAiEvidenceConsent(workspace: Workspace): boolean {
  const key = aiEvidenceConsentKey(workspace);
  if (!key) return false;
  try {
    return window.localStorage.getItem(key) === "accepted";
  } catch {
    return false;
  }
}

function rememberAiEvidenceConsent(workspace: Workspace) {
  const key = aiEvidenceConsentKey(workspace);
  if (!key) return;
  try {
    window.localStorage.setItem(key, "accepted");
  } catch {
    // A blocked storage API must not make consent implicit. This explicit
    // confirmation still authorizes only the analysis being started now.
  }
}

function forgetAiEvidenceConsent(workspace: Workspace) {
  for (const providerKind of ["codex", "claude"] as const) {
    const key = aiEvidenceConsentStorageKey(workspace.id, providerKind);
    try {
      window.localStorage.removeItem(key);
    } catch {
      // The record is not a credential. A blocked storage API is harmless.
    }
  }
}

function Header({
  workspace,
  engineRegistry,
  status,
  mapView,
  selection,
  onSelect,
  onOpenProvider,
  onAnalyze,
  onCancel,
  analyzing,
  cancelling,
  analysisProgress,
}: {
  workspace: Workspace | null;
  engineRegistry: EngineRegistry | null;
  status: FactGraphStatus | null;
  mapView: MapView | null;
  selection: Selection | null;
  onSelect: (id: string) => void;
  onOpenProvider: () => void;
  onAnalyze: () => void;
  onCancel: () => void;
  analyzing: boolean;
  cancelling: boolean;
  analysisProgress: AnalysisProgressEvent | null;
}) {
  const [query, setQuery] = useState("");
  const [searchFocused, setSearchFocused] = useState(false);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const engineCount = engineRegistry?.engines.filter((engine) => engine.available).length ?? 0;
  const engineTotal = engineRegistry?.engines.length ?? 2;
  const activeModel = workspace?.provider ? modelFor(workspace.provider.kind, workspace.provider.model) : null;
  const searchResults = useMemo(() => searchMap(mapView, query), [mapView, query]);

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchRef.current?.focus();
      }
      if (event.key === "Escape" && document.activeElement === searchRef.current) {
        setQuery("");
        searchRef.current?.blur();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  function chooseResult(result: MapSearchResult) {
    onSelect(result.id);
    setQuery("");
    setSearchFocused(false);
    searchRef.current?.blur();
  }

  return (
    <header className="app-header">
      <div className="brand-block">
        <span className="brand-mark" aria-hidden="true">
          <Map fontSize={18} />
        </span>
        <strong>Codebase</strong>
        <span>Workspace</span>
      </div>
      <div className="header-context" aria-label="현재 위치">
        <span>워크스페이스</span>
        <ChevronRight fontSize={13} aria-hidden="true" />
        <strong>{workspace?.name ?? "프로젝트 없음"}</strong>
        {selection ? (
          <>
            <ChevronRight fontSize={13} aria-hidden="true" />
            <em>{selection.title}</em>
          </>
        ) : null}
      </div>
      <div className="header-search">
        <Input
          id="global-map-search"
          ref={searchRef}
          aria-label="지도 검색"
          value={query}
          onChange={(_, data) => setQuery(data.value)}
          onFocus={() => setSearchFocused(true)}
          onBlur={() => window.setTimeout(() => setSearchFocused(false), 120)}
          contentBefore={<Search fontSize={16} />}
          contentAfter={<kbd>Ctrl K</kbd>}
          placeholder="영역, API, 심벌 검색"
          disabled={!mapView}
        />
        {searchFocused && query.trim() ? (
          <div className="search-results" role="listbox" aria-label="지도 검색 결과">
            {searchResults.length > 0 ? (
              searchResults.map((result) => (
                <button key={result.id} type="button" role="option" onClick={() => chooseResult(result)}>
                  <span className="search-result-icon" aria-hidden="true">
                    {result.kind === "영역" ? <Network fontSize={15} /> : <Code fontSize={15} />}
                  </span>
                  <span>
                    <strong>{result.label}</strong>
                    <small>{result.detail}</small>
                  </span>
                  <ChevronRight fontSize={13} aria-hidden="true" />
                </button>
              ))
            ) : (
              <p>일치하는 지도 항목이 없습니다.</p>
            )}
          </div>
        ) : null}
      </div>
      <div className="header-health" aria-label="분석 상태 요약">
        <span>
          <small>스냅샷</small>
          <strong>{status?.snapshotId?.slice(0, 8) ?? "없음"}</strong>
        </span>
        <span className={engineCount === engineTotal ? "ok" : "warn"}>
          <small>분석 엔진</small>
          <strong>
            {engineCount}/{engineTotal}
          </strong>
        </span>
        <span>
          <small>근거</small>
          <strong>{status?.evidenceCount.toLocaleString("ko-KR") ?? "—"}</strong>
        </span>
      </div>
      <div className="header-actions">
        <Button
          className="analysis-button"
          appearance="primary"
          icon={analyzing ? <StopCircle fontSize={15} /> : <Network fontSize={15} />}
          onClick={analyzing ? onCancel : onAnalyze}
          disabled={!workspace || cancelling}
          title={
            analyzing
              ? analysisProgress?.label
              : "이전 분석 결과를 재사용하지 않고 현재 코드를 처음부터 다시 분석합니다"
          }
        >
          {analyzing ? (cancelling ? "취소 중" : "분석 중 · 취소") : "재분석"}
        </Button>
        <Button className="provider-button" appearance="secondary" onClick={onOpenProvider} disabled={!workspace}>
          <span>
            <small>AI 모델</small>
            <strong>{activeModel?.label ?? workspace?.provider?.model ?? "모델 설정"}</strong>
            {workspace?.provider ? <em>{effortLabel(workspace.provider.effort ?? "high")}</em> : null}
          </span>
          <ChevronDown fontSize={15} />
        </Button>
      </div>
    </header>
  );
}

function NavigationRail({
  workspaces,
  activeWorkspaceId,
  areas,
  selectedId,
  onSelect,
  onSelectArea,
  onCreate,
  onDelete,
  analyzingWorkspaceId,
}: {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  areas: MapArea[];
  selectedId: string | null;
  onSelect: (workspaceId: string) => void;
  onSelectArea: (areaId: string | null) => void;
  onCreate: () => void;
  onDelete: (workspace: Workspace) => void;
  analyzingWorkspaceId: string | null;
}) {
  return (
    <nav className="navigation-rail" aria-label="지도 탐색">
      <div className="nav-section nav-primary">
        <button type="button" className={!selectedId ? "nav-row active" : "nav-row"} onClick={() => onSelectArea(null)}>
          <Home fontSize={18} />
          <span>
            <strong>개요</strong>
            <small>전체 구조</small>
          </span>
        </button>
      </div>

      <div className="nav-section nav-outline">
        <div className="nav-section-title">
          <span>의미 영역</span>
          <small>{areas.length.toLocaleString("ko-KR")}</small>
        </div>
        <div className="area-outline">
          {areas.map((area) => (
            <button
              type="button"
              className={areaContainsSelection(area, selectedId) ? "nav-row active" : "nav-row"}
              onClick={() => onSelectArea(area.id)}
              key={area.id}
            >
              <span className="area-outline-mark" aria-hidden="true" />
              <span>
                <strong>{area.name}</strong>
                <small>{area.originalName ?? area.summary}</small>
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="nav-section nav-tools">
        <button type="button" className="nav-row" onClick={() => document.getElementById("global-map-search")?.focus()}>
          <Search fontSize={17} />
          <span>
            <strong>검색</strong>
            <small>영역 · API · 심벌</small>
          </span>
        </button>
      </div>

      <div className="nav-spacer" />

      <div className="nav-section nav-workspaces">
        <div className="nav-section-title">
          <span>프로젝트</span>
          <button type="button" onClick={onCreate} aria-label="작업공간 추가">
            <Plus fontSize={14} />
          </button>
        </div>
        {workspaces.map((workspace) => (
          <div className="workspace-item" key={workspace.id}>
            <button
              type="button"
              className={workspace.id === activeWorkspaceId ? "workspace-row active" : "workspace-row"}
              onClick={() => onSelect(workspace.id)}
            >
              <FolderCode fontSize={16} />
              <span>
                <strong>{workspace.name}</strong>
                <small>{lastPathSegment(workspace.repoPath)}</small>
              </span>
            </button>
            <button
              type="button"
              className="workspace-delete"
              onClick={() => onDelete(workspace)}
              disabled={analyzingWorkspaceId === workspace.id}
              aria-label={`${workspace.name} 프로젝트 삭제`}
              title={analyzingWorkspaceId === workspace.id ? "분석 중에는 삭제할 수 없습니다" : "프로젝트 삭제"}
            >
              <Delete fontSize={14} />
            </button>
          </div>
        ))}
        {workspaces.length === 0 ? <p className="rail-empty">연결된 프로젝트가 없습니다.</p> : null}
      </div>
    </nav>
  );
}

/** Says out loud that the canvas is not showing analysed code. */
function PreviewBanner() {
  return (
    <div className="preview-banner" role="status">
      <TriangleAlert fontSize={14} aria-hidden="true" />
      미리보기 데이터 · 실제 코드 분석 결과가 아닙니다.
    </div>
  );
}

function EmptyCanvas({ onCreate }: { onCreate: () => void }) {
  return (
    <section className="empty-canvas">
      <div className="empty-coordinate">프로젝트 없음</div>
      <div className="empty-copy">
        <span className="empty-symbol" aria-hidden="true">
          <FolderOpen fontSize={28} />
        </span>
        <p className="eyebrow">시작하기</p>
        <h1>
          코드를 읽기 전에
          <br />
          구조부터 봅니다.
        </h1>
        <p>로컬 프로젝트를 연결하면 코드 구조와 확인된 실행 경로를 하나의 캔버스에서 볼 수 있습니다.</p>
        <Button appearance="primary" icon={<FolderOpen fontSize={17} />} onClick={onCreate}>
          프로젝트 폴더 연결
        </Button>
      </div>
    </section>
  );
}

function WorkspaceCanvas({
  workspace,
  status,
  analyzing,
  progress,
  startedAt,
  onAnalyze,
  onCancel,
  cancelling,
}: {
  workspace: Workspace;
  status: FactGraphStatus | null;
  analyzing: boolean;
  progress: AnalysisProgressEvent | null;
  startedAt: number | null;
  onAnalyze: () => void;
  onCancel: () => void;
  cancelling: boolean;
}) {
  const hasSnapshot = Boolean(status?.snapshotId);
  return (
    <section className="workspace-canvas">
      <div className="canvas-coordinate">{workspace.name}</div>
      <div className="ledger-strip" aria-label="분석 원장">
        <LedgerMetric label="노드" value={status?.nodeCount ?? 0} />
        <LedgerMetric label="관계" value={status?.edgeCount ?? 0} />
        <LedgerMetric label="근거" value={status?.evidenceCount ?? 0} />
        <LedgerMetric label="파일" value={status?.coverageCount ?? 0} />
      </div>
      <div className="canvas-message">
        {analyzing ? (
          <AnalysisStatus
            variant="embedded"
            progress={progress}
            startedAt={startedAt}
            cancelling={cancelling}
            onCancel={onCancel}
          />
        ) : (
          <>
            <Network fontSize={30} />
            <p className="eyebrow">{hasSnapshot ? "분석 준비 완료" : "분석 결과 없음"}</p>
            <h2>{hasSnapshot ? "검증된 구조를 불러왔습니다." : "아직 분석 결과가 없습니다."}</h2>
            <p>
              {hasSnapshot
                ? `snapshot ${status?.snapshotId ?? ""}`
                : "새 canonical Fact Graph에 저장된 결과만 이 캔버스에 표시됩니다."}
            </p>
            <Button appearance="primary" icon={<Network fontSize={17} />} onClick={onAnalyze}>
              {hasSnapshot ? "의미 지도 다시 만들기" : "코드 분석 시작"}
            </Button>
          </>
        )}
      </div>
    </section>
  );
}

function AnalysisStatus({
  variant,
  progress,
  startedAt,
  cancelling,
  onCancel,
}: {
  variant: "overlay" | "embedded";
  progress: AnalysisProgressEvent | null;
  startedAt: number | null;
  cancelling: boolean;
  onCancel: () => void;
}) {
  const elapsedSeconds = useElapsedSeconds(startedAt);
  const completed = Math.max(0, progress?.completed ?? 0);
  const total = Math.max(1, progress?.total ?? 1);
  return (
    <section className={`analysis-status ${variant}`} aria-label="코드 분석 상태" aria-live="polite">
      <div className="analysis-status-head">
        <span className="analysis-status-spinner" aria-hidden="true">
          <LoaderCircle className="spin" />
        </span>
        <span>
          <small>{analysisStageLabel(progress?.stage)}</small>
          <strong>
            {cancelling ? "실행 중인 분석을 안전하게 중지하는 중" : (progress?.label ?? "코드를 분석하는 중")}
          </strong>
        </span>
        <time>경과 {formatElapsed(elapsedSeconds)}</time>
      </div>
      <progress aria-label="현재 단계 진행률" max={total} value={Math.min(completed, total)} />
      <div className="analysis-status-foot">
        <span>
          현재 단계 {total > 1 ? `${completed.toLocaleString("ko-KR")}/${total.toLocaleString("ko-KR")}` : "실행 중"}
        </span>
        <small>전체 완료 예상치가 아닌 현재 단계의 실제 상태입니다.</small>
        <Button
          appearance="subtle"
          size="small"
          icon={<StopCircle fontSize={14} />}
          onClick={onCancel}
          disabled={cancelling}
        >
          {cancelling ? "취소 중" : "분석 취소"}
        </Button>
      </div>
    </section>
  );
}

function DeleteWorkspaceDialog({
  workspace,
  onClose,
  onConfirm,
}: {
  workspace: Workspace;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open onOpenChange={(_, data) => (!data.open ? onClose() : undefined)}>
      <DialogSurface className="dialog-surface" aria-label="프로젝트 삭제 확인">
        <DialogBody>
          <DialogTitle>프로젝트 연결을 삭제할까요?</DialogTitle>
          <DialogContent className="dialog-content">
            <p className="dialog-lead">
              <strong>{workspace.name}</strong>의 저장된 지도와 대화 문맥을 이 앱에서 삭제합니다.
            </p>
            <p className="dialog-note">원본 코드 폴더는 절대 삭제하지 않습니다.</p>
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" type="button" onClick={onClose}>
              취소
            </Button>
            <Button appearance="primary" type="button" onClick={onConfirm}>
              앱에서 삭제
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function AiEvidenceConsentDialog({
  workspace,
  onClose,
  onConfirm,
}: {
  workspace: Workspace;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const provider = workspace.provider?.kind === "claude" ? "Claude CLI" : "Codex CLI";
  return (
    <Dialog open onOpenChange={(_, data) => (!data.open ? onClose() : undefined)}>
      <DialogSurface className="dialog-surface" aria-label="AI 코드 근거 전송 동의">
        <DialogBody>
          <DialogTitle>AI 의미 분석을 시작할까요?</DialogTitle>
          <DialogContent className="dialog-content">
            <p className="dialog-lead">
              <strong>{workspace.name}</strong>에서 선택된 소스 코드 근거 발췌가 {provider}에 전달됩니다. CLI 설정에
              따라 외부 AI 서비스로 전송될 수 있습니다.
            </p>
            <p className="dialog-note">
              정적 코드 사실은 로컬에서 만들고, 알려진 비밀값 패턴은 전송 전에 마스킹합니다. 자동 마스킹이 모든 형태의
              비밀값을 보장하지는 않습니다.
            </p>
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" type="button" onClick={onClose}>
              취소
            </Button>
            <Button appearance="primary" type="button" onClick={onConfirm}>
              동의하고 분석
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

function LedgerMetric({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value.toLocaleString()}</strong>
    </div>
  );
}

function StatusLedger({
  workspace,
  engineRegistry,
  status,
  previewing,
  analyzing,
}: {
  workspace: Workspace | null;
  engineRegistry: EngineRegistry | null;
  status: FactGraphStatus | null;
  previewing: boolean;
  analyzing: boolean;
}) {
  const availableEngines = engineRegistry?.engines.filter((engine) => engine.available).length ?? 0;
  const engineTotal = engineRegistry?.engines.length ?? 2;
  return (
    <footer className="status-ledger" aria-label="워크스페이스 상태">
      <span className="status-ledger-primary">
        <i className={analyzing ? "pulse" : ""} aria-hidden="true" />
        {previewing ? "디자인 미리보기" : analyzing ? "분석 진행 중" : "정적 분석 준비"}
      </span>
      <span>
        프로젝트 <strong>{workspace?.name ?? "없음"}</strong>
      </span>
      <span>
        스냅샷 <code>{status?.snapshotId?.slice(0, 8) ?? "—"}</code>
      </span>
      <span>
        노드 <strong>{status?.nodeCount.toLocaleString("ko-KR") ?? "—"}</strong>
      </span>
      <span>
        관계 <strong>{status?.edgeCount.toLocaleString("ko-KR") ?? "—"}</strong>
      </span>
      <span>
        파일 <strong>{status?.coverageCount.toLocaleString("ko-KR") ?? "—"}</strong>
      </span>
      <span className="status-ledger-end">
        엔진{" "}
        <strong>
          {availableEngines}/{engineTotal}
        </strong>
      </span>
    </footer>
  );
}

function ChatPanel({
  workspace,
  status,
  selection,
  previewing,
}: {
  workspace: Workspace | null;
  status: FactGraphStatus | null;
  selection: Selection | null;
  previewing: boolean;
}) {
  const reason = previewing
    ? "디자인 미리보기에서는 대화를 전송하지 않습니다."
    : !workspace
      ? "프로젝트를 연결하면 대화 문맥이 만들어집니다."
      : !workspace.provider
        ? "Codex 또는 Claude 모델을 먼저 설정하세요."
        : !status?.snapshotId
          ? "검증된 분석 snapshot이 있어야 질문할 수 있습니다."
          : null;
  return (
    <section className="chat-panel">
      <div className="chat-heading">
        <div>
          <span>대화</span>
          <strong>지도에 질문</strong>
        </div>
        <MessageSquareText fontSize={18} />
      </div>
      {/* What the question is about, so the reader never has to guess. */}
      <div className="chat-context">
        <span>문맥</span>
        <p>{selection ? selection.title : (workspace?.name ?? "선택된 지도 없음")}</p>
      </div>
      <div className="chat-empty">
        <Braces fontSize={22} />
        <p>{reason ?? "영역이나 관계를 선택한 뒤 질문하세요."}</p>
      </div>
      <div className="chat-composer">
        <Textarea aria-label="지도에 질문" placeholder="선택한 구조에 대해 질문" disabled={Boolean(reason)} />
        <Button appearance="primary" type="button" disabled={Boolean(reason)}>
          보내기
        </Button>
      </div>
    </section>
  );
}

function CreateWorkspaceDialog({
  onClose,
  onCreated,
  onError,
}: {
  onClose: () => void;
  onCreated: (workspace: Workspace) => void;
  onError: (error: string) => void;
}) {
  const [name, setName] = useState("");
  const [repoPath, setRepoPath] = useState("");
  const [saving, setSaving] = useState(false);

  async function pickFolder() {
    const selected = await chooseRepositoryFolder();
    if (!selected) return;
    setRepoPath(selected);
    if (!name) setName(lastPathSegment(selected));
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!name.trim() || !repoPath.trim()) return;
    setSaving(true);
    try {
      onCreated(await createWorkspace(name.trim(), repoPath.trim()));
    } catch (reason) {
      onError(errorMessage(reason));
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(_, data) => (!data.open ? onClose() : undefined)}>
      <DialogSurface className="dialog-surface" aria-label="프로젝트 연결">
        <form onSubmit={submit}>
          <DialogBody>
            <DialogTitle>프로젝트 폴더 연결</DialogTitle>
            <DialogContent className="dialog-content">
              <p className="dialog-lead">분석할 로컬 코드베이스를 작업공간에 추가합니다.</p>
              <Field label="표시 이름" required>
                <Input
                  value={name}
                  onChange={(_, data) => setName(data.value)}
                  autoFocus
                  maxLength={120}
                  placeholder="예: commerce-platform"
                />
              </Field>
              <Field label="로컬 폴더" required>
                <span className="path-input">
                  <Input value={repoPath} onChange={(_, data) => setRepoPath(data.value)} placeholder="D:\\project" />
                  <Button type="button" onClick={() => void pickFolder()}>
                    찾기
                  </Button>
                </span>
              </Field>
              {!hasDesktopRuntime() ? (
                <p className="dialog-note">폴더 선택과 저장은 데스크톱 앱에서 동작합니다.</p>
              ) : null}
            </DialogContent>
            <DialogActions>
              <Button appearance="secondary" type="button" onClick={onClose}>
                취소
              </Button>
              <Button appearance="primary" type="submit" disabled={saving || !name.trim() || !repoPath.trim()}>
                {saving ? "연결 중" : "연결"}
              </Button>
            </DialogActions>
          </DialogBody>
        </form>
      </DialogSurface>
    </Dialog>
  );
}

function ProviderDialog({
  workspace,
  providers,
  onClose,
  onSaved,
  onError,
}: {
  workspace: Workspace;
  providers: AiProviderAvailability[];
  onClose: () => void;
  onSaved: (workspace: Workspace) => void;
  onError: (error: string) => void;
}) {
  const [kind, setKind] = useState<ProviderKind>(workspace.provider?.kind ?? "codex");
  const [model, setModel] = useState(() =>
    defaultModelFor(workspace.provider?.kind ?? "codex", workspace.provider?.model),
  );
  const [effort, setEffort] = useState<ReasoningEffort>(() => {
    const initialKind = workspace.provider?.kind ?? "codex";
    const initialModel = defaultModelFor(initialKind, workspace.provider?.model);
    return defaultEffortFor(initialKind, initialModel, workspace.provider?.effort ?? "high");
  });
  const [saving, setSaving] = useState(false);
  const models = modelsFor(kind);
  const efforts = effortOptionsFor(kind, model);
  const activeProvider = providers.find((provider) => provider.kind === kind);
  const canSave = activeProvider?.installed === true;

  /* Each provider runs its own models, so switching kind moves the choice with it. */
  function chooseKind(next: ProviderKind) {
    const nextModel = defaultModelFor(next, workspace.provider?.kind === next ? workspace.provider.model : null);
    setKind(next);
    setModel(nextModel);
    setEffort(
      defaultEffortFor(
        next,
        nextModel,
        workspace.provider?.kind === next ? (workspace.provider.effort ?? "high") : "high",
      ),
    );
  }

  function chooseModel(nextModel: string) {
    setModel(nextModel);
    setEffort(defaultEffortFor(kind, nextModel, effort));
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!model.trim()) return;
    setSaving(true);
    try {
      onSaved(await setWorkspaceProvider(workspace.id, kind, model.trim(), effort));
    } catch (reason) {
      onError(errorMessage(reason));
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(_, data) => (!data.open ? onClose() : undefined)}>
      <DialogSurface className="dialog-surface provider-dialog" aria-label="AI 모델 설정">
        <form onSubmit={submit}>
          <DialogBody>
            <DialogTitle>AI 모델 설정</DialogTitle>
            <DialogContent className="dialog-content provider-dialog-content">
              <p className="dialog-lead">지도 의미 분석과 이후 대화가 같은 CLI 설정을 사용합니다.</p>

              <section className="settings-section" aria-labelledby="provider-setting-title">
                <div className="settings-label" id="provider-setting-title">
                  CLI 연결
                </div>
                <div className="provider-options">
                  {providers.map((provider) => (
                    <button
                      type="button"
                      key={provider.kind}
                      className={provider.kind === kind ? "provider-option active" : "provider-option"}
                      aria-pressed={provider.kind === kind}
                      disabled={!provider.installed}
                      onClick={() => chooseKind(provider.kind)}
                    >
                      <span className="provider-icon" aria-hidden="true">
                        <TerminalSquare fontSize={18} />
                      </span>
                      <span>
                        <strong>{provider.label} CLI</strong>
                        <small>
                          {provider.installed ? (provider.version ?? "연결됨") : "현재 기기에서 찾을 수 없음"}
                        </small>
                      </span>
                      <i
                        className={provider.installed ? "provider-status connected" : "provider-status"}
                        aria-hidden="true"
                      />
                    </button>
                  ))}
                </div>
              </section>

              <section className="settings-section" aria-labelledby="model-setting-title">
                <div className="settings-label" id="model-setting-title">
                  모델
                </div>
                <div className="model-grid" role="radiogroup" aria-label="AI 모델">
                  {models.map((entry) => (
                    <button
                      type="button"
                      role="radio"
                      aria-checked={entry.id === model}
                      className={entry.id === model ? "model-card active" : "model-card"}
                      key={entry.id}
                      onClick={() => chooseModel(entry.id)}
                    >
                      <span className="model-card-topline">
                        <strong>{entry.label}</strong>
                        <small>{entry.family}</small>
                      </span>
                      <span>{entry.note}</span>
                      <code>{entry.id}</code>
                    </button>
                  ))}
                </div>
              </section>

              <section className="settings-section" aria-labelledby="effort-setting-title">
                <div className="settings-label-row">
                  <div className="settings-label" id="effort-setting-title">
                    추론 강도
                  </div>
                  <span>기본값: 높음</span>
                </div>
                <div className="effort-control" role="radiogroup" aria-label="추론 강도">
                  {efforts.map((option) => (
                    <button
                      type="button"
                      role="radio"
                      aria-checked={option.id === effort}
                      className={option.id === effort ? "effort-option active" : "effort-option"}
                      key={option.id}
                      onClick={() => setEffort(option.id)}
                    >
                      <strong>{option.label}</strong>
                      <small>{option.id}</small>
                    </button>
                  ))}
                </div>
              </section>

              <div className="cli-receipt" aria-label="CLI 실행 설정">
                <span>실제 실행값</span>
                <code>
                  {kind === "codex"
                    ? `codex --model ${model} --config model_reasoning_effort="${effort}"`
                    : `claude --model ${model} --effort ${effort}`}
                </code>
              </div>
              {!canSave ? <p className="dialog-note">선택한 CLI가 설치되어 있어야 설정을 저장할 수 있습니다.</p> : null}
            </DialogContent>
            <DialogActions>
              <Button appearance="secondary" type="button" onClick={onClose}>
                취소
              </Button>
              <Button appearance="primary" type="submit" disabled={saving || !model.trim() || !canSave}>
                {saving ? "저장 중" : "설정 저장"}
              </Button>
            </DialogActions>
          </DialogBody>
        </form>
      </DialogSurface>
    </Dialog>
  );
}

function CenteredState({ icon, title, detail }: { icon: React.ReactNode; title: string; detail?: string }) {
  return (
    <section className="centered-state">
      {icon}
      <h2>{title}</h2>
      {detail ? <p>{detail}</p> : null}
    </section>
  );
}

function ErrorNotice({ message, onClose }: { message: string; onClose: () => void }) {
  const { summary, detail } = splitErrorMessage(message);
  return (
    <div className="error-toast" role="alert">
      <CircleAlert fontSize={18} aria-hidden="true" />
      <div className="error-toast-copy">
        <strong>{summary}</strong>
        {detail ? (
          <details>
            <summary>자세히</summary>
            <pre>{detail}</pre>
          </details>
        ) : null}
      </div>
      <button type="button" onClick={onClose} aria-label="오류 닫기">
        닫기
      </button>
    </div>
  );
}

function searchMap(view: MapView | null, rawQuery: string): MapSearchResult[] {
  const query = rawQuery.trim().toLocaleLowerCase("ko-KR");
  if (!view || !query) return [];
  const results: MapSearchResult[] = [];
  for (const area of flattenAreas(view.areas)) {
    const areaText = `${area.name} ${area.originalName ?? ""} ${area.summary}`.toLocaleLowerCase("ko-KR");
    if (areaText.includes(query)) {
      results.push({
        id: area.id,
        kind: "영역",
        label: area.name,
        detail: area.originalName ? `${area.originalName} · ${area.summary}` : area.summary,
      });
    }
    for (const node of area.nodes) {
      if (`${node.name} ${node.kind}`.toLocaleLowerCase("ko-KR").includes(query)) {
        results.push({ id: node.id, kind: "구현", label: node.name, detail: `${area.name} · ${node.kind}` });
      }
    }
  }
  return results.slice(0, 8);
}

function areaContainsSelection(area: MapArea, selectedId: string | null): boolean {
  if (!selectedId) return false;
  return (
    area.id === selectedId ||
    area.nodes.some((node: MapNode) => node.id === selectedId) ||
    area.areas.some((child) => areaContainsSelection(child, selectedId))
  );
}

function lastPathSegment(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? path;
}

function useElapsedSeconds(startedAt: number | null): number {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (startedAt === null) {
      setElapsed(0);
      return;
    }
    const update = () => setElapsed(Math.max(0, Math.floor((Date.now() - startedAt) / 1000)));
    update();
    const timer = window.setInterval(update, 1_000);
    return () => window.clearInterval(timer);
  }, [startedAt]);
  return elapsed;
}

function formatElapsed(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return minutes > 0 ? `${minutes}분 ${String(remainder).padStart(2, "0")}초` : `${remainder}초`;
}

function analysisStageLabel(stage: string | undefined): string {
  if (!stage || stage === "starting") return "분석 준비";
  if (stage === "discovery" || stage === "manifest") return "소스 확인";
  if (stage === "project-model" || stage === "planning") return "분석 계획";
  if (stage === "provider-setup" || stage === "provider-selection") return "언어 분석기 준비";
  if (stage === "providers") return "정적 코드 분석";
  if (stage === "facts-ready") return "코드 사실 저장 완료";
  if (stage === "semantic") return "AI 의미 지도 구성";
  return "코드 분석";
}

function splitErrorMessage(message: string): { summary: string; detail: string | null } {
  const normalized = message.trim() || "작업을 완료하지 못했습니다.";
  const firstLine = normalized.split(/\r?\n/, 1)[0] ?? normalized;
  if (normalized.length <= 220 && !normalized.includes("\n")) {
    return { summary: normalized, detail: null };
  }
  const candidate = firstLine.slice(0, 180);
  const boundary = Math.max(candidate.lastIndexOf(". "), candidate.lastIndexOf(" | "), candidate.lastIndexOf(" "));
  const summary = `${candidate.slice(0, boundary >= 96 ? boundary : 180).trimEnd()}…`;
  return { summary, detail: normalized };
}

function errorMessage(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object") {
    const value = reason as CommandError;
    return value.detail || value.message || "작업을 완료하지 못했습니다.";
  }
  return "작업을 완료하지 못했습니다.";
}
