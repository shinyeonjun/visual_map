import {
  AddRegular as Plus,
  ArrowSyncRegular as LoaderCircle,
  BracesRegular as Braces,
  ChatRegular as MessageSquareText,
  ChevronDownRegular as ChevronDown,
  DatabaseRegular as Database,
  DeleteRegular as Delete,
  DismissCircleRegular as StopCircle,
  ErrorCircleRegular as CircleAlert,
  FolderOpenRegular as FolderOpen,
  FolderRegular as FolderCode,
  OrganizationRegular as Network,
  SearchRegular as Search,
  SettingsRegular as Settings2,
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
  Tooltip,
} from "@fluentui/react-components";
import { useEffect, useMemo, useRef, useState } from "react";
import { Inspector } from "./map/Inspector";
import { MapCanvas } from "./map/MapCanvas";
import { PREVIEW_MAP, PREVIEW_SELECTION } from "./map/previewMap";
import type { EvidenceRef, MapView, Selection } from "./map/types";
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
  const [deleteTarget, setDeleteTarget] = useState<Workspace | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mapView, setMapView] = useState<MapView | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [analyzingWorkspaceId, setAnalyzingWorkspaceId] = useState<string | null>(null);
  const [cancellingWorkspaceId, setCancellingWorkspaceId] = useState<string | null>(null);
  const [analysisProgress, setAnalysisProgress] = useState<AnalysisProgressEvent | null>(null);
  const [analysisStartedAt, setAnalysisStartedAt] = useState<number | null>(null);
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
        setWorkspaces(workspaceRows);
        setProviders(providerRows);
        setEngineRegistry(registry);
        const firstWorkspaceId = workspaceRows[0]?.id ?? null;
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

  useEffect(() => {
    if (previewing) {
      setSelection(selectedId === PREVIEW_SELECTION.id ? PREVIEW_SELECTION : null);
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

  async function runAnalysis() {
    const workspace = activeWorkspace;
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
      label: "코드 분석을 시작합니다",
    });
    setAnalysisStartedAt(Date.now());
    setAnalyzingWorkspaceId(workspace.id);
    try {
      const result = await analyzeWorkspace(workspace.id);
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
        engineRegistry={engineRegistry}
        onOpenProvider={() => setProviderOpen(true)}
        onAnalyze={() => void runAnalysis()}
        onCancel={() => void cancelAnalysis()}
        analyzing={Boolean(activeWorkspaceId) && analyzingWorkspaceId === activeWorkspaceId}
        cancelling={Boolean(activeWorkspaceId) && cancellingWorkspaceId === activeWorkspaceId}
        analysisProgress={analysisProgress}
      />
      <div className="workbench">
        <ToolRail onCreate={() => setCreateOpen(true)} />
        <ProjectRail
          workspaces={workspaces}
          activeWorkspaceId={activeWorkspaceId}
          onSelect={selectWorkspace}
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
              <MapCanvas view={displayedMap} selectedId={selectedId} onSelect={setSelectedId} />
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
              onAnalyze={() => void runAnalysis()}
              onCancel={() => void cancelAnalysis()}
              cancelling={cancellingWorkspaceId === activeWorkspace.id}
            />
          )}
        </main>
        <aside className="detail-column">
          <div className="detail-head">
            <strong>{selection?.title ?? "선택 없음"}</strong>
          </div>
          <Inspector selection={selection} onOpenEvidence={openEvidence} />
          <ChatPanel workspace={activeWorkspace} status={factStatus} selection={selection} />
        </aside>
      </div>
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
    </div>
  );
}

function Header({
  workspace,
  engineRegistry,
  onOpenProvider,
  onAnalyze,
  onCancel,
  analyzing,
  cancelling,
  analysisProgress,
}: {
  workspace: Workspace | null;
  engineRegistry: EngineRegistry | null;
  onOpenProvider: () => void;
  onAnalyze: () => void;
  onCancel: () => void;
  analyzing: boolean;
  cancelling: boolean;
  analysisProgress: AnalysisProgressEvent | null;
}) {
  const engineCount = engineRegistry?.engines.filter((engine) => engine.available).length ?? 0;
  const engineTotal = engineRegistry?.engines.length ?? 2;
  const activeModel = workspace?.provider ? modelFor(workspace.provider.kind, workspace.provider.model) : null;
  return (
    <header className="app-header">
      <div className="brand-block">
        <span className="brand-mark" aria-hidden="true">
          CW
        </span>
        <div>
          <strong>Codebase workspace</strong>
          <span>검증된 코드 구조 지도</span>
        </div>
      </div>
      <div className="header-context">
        <span className="context-label">프로젝트</span>
        <span className="context-value">{workspace?.name ?? "연결되지 않음"}</span>
        {workspace ? <span className="context-path">{workspace.repoPath}</span> : null}
      </div>
      <div className="header-actions">
        <span className={`engine-pill ${engineCount === engineTotal ? "ok" : "warn"}`}>
          <TerminalSquare fontSize={14} /> 엔진 {engineCount}/{engineTotal}
        </span>
        <Button
          className="analysis-button"
          appearance="primary"
          icon={analyzing ? <StopCircle fontSize={15} /> : <Network fontSize={15} />}
          onClick={analyzing ? onCancel : onAnalyze}
          disabled={!workspace || cancelling}
          title={analyzing ? analysisProgress?.label : "현재 코드로 새 지도를 만듭니다"}
        >
          {analyzing ? (cancelling ? "취소 중" : "분석 중 · 취소") : "분석"}
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

function ToolRail({ onCreate }: { onCreate: () => void }) {
  return (
    <nav className="tool-rail" aria-label="주요 도구">
      <ToolButton label="구조 지도" active icon={<Network />} />
      <ToolButton label="검색 · 준비 중" disabled icon={<Search />} />
      <ToolButton label="데이터베이스 · 준비 중" disabled icon={<Database />} />
      <span className="tool-spacer" />
      <ToolButton label="프로젝트 추가" onClick={onCreate} icon={<Plus />} />
      <ToolButton label="설정 · 준비 중" disabled icon={<Settings2 />} />
    </nav>
  );
}

function ToolButton({
  label,
  icon,
  active = false,
  disabled = false,
  onClick,
}: {
  label: string;
  icon: React.ReactNode;
  active?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}) {
  return (
    <Tooltip content={label} relationship="label" positioning="after">
      <button
        type="button"
        className={active ? "tool active" : "tool"}
        aria-label={label}
        disabled={disabled}
        onClick={onClick}
      >
        {icon}
      </button>
    </Tooltip>
  );
}

function ProjectRail({
  workspaces,
  activeWorkspaceId,
  onSelect,
  onCreate,
  onDelete,
  analyzingWorkspaceId,
}: {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  onSelect: (workspaceId: string) => void;
  onCreate: () => void;
  onDelete: (workspace: Workspace) => void;
  analyzingWorkspaceId: string | null;
}) {
  return (
    <aside className="project-rail">
      <div className="rail-heading">
        <span>프로젝트</span>
        <button type="button" onClick={onCreate} aria-label="작업공간 추가">
          <Plus fontSize={14} />
        </button>
      </div>
      <div className="workspace-list">
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
      </div>
      {workspaces.length === 0 ? <p className="rail-empty">연결된 프로젝트가 없습니다.</p> : null}
    </aside>
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

function LedgerMetric({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value.toLocaleString()}</strong>
    </div>
  );
}

function ChatPanel({
  workspace,
  status,
  selection,
}: {
  workspace: Workspace | null;
  status: FactGraphStatus | null;
  selection: Selection | null;
}) {
  const reason = !workspace
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
