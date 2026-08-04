import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import "./styles/index.css";
import type { AppPaths } from "./components/common/DevDiagnostics";
import { DevDiagnostics } from "./components/common/DevDiagnostics";
import { currentOperationStatus, repoPathErrorFor } from "./app/appState";
import { toUserError } from "./app/operationStatus";
import {
  validateInventoryBootstrap,
  validateStringArray,
  validateWorkspace,
  validateWorkspaceAnalysisResult,
} from "./app/runtimeContracts";
import { buildDbProfileControls, buildVisualMapControls, buildWorkspaceControls } from "./app/controlBuilders";
import { hasTauriRuntime } from "./app/tauriRuntime";
import { useCodeInventory } from "./hooks/useCodeInventory";
import { useDbProfiles } from "./hooks/useDbProfiles";
import { useEngineRegistry } from "./hooks/useEngineRegistry";
import { useVisualMap } from "./hooks/useVisualMap";
import { useWorkspaces } from "./hooks/useWorkspaces";
import { codeInventoryFromSnapshot, dbInventoryFromSnapshot } from "./inventory/snapshotRestore";
import { dbProfileSourceUsesPath, codeInventoryItemCount } from "./types/workspace";
import type { InitializeWorkspaceAnalysisRequest, SaveDbProfileRequest } from "./types/workspace";
import { scheduleSearchIndex } from "./visual/search";
import type { AnalysisProgress, AnalysisSetupChoice } from "./features/map/AnalysisSetupDialog";

import { MapWorkspace } from "./features/map/MapWorkspace";

function App() {
  const [sourceManagerOpen, setSourceManagerOpen] = useState(false);
  const [appPaths, setAppPaths] = useState<AppPaths | null>(null);
  const [appPathError, setAppPathError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [busyNotice, setBusyNotice] = useState<string | null>(null);
  const [latestOperationAction, setLatestOperationAction] = useState<string | null>(null);
  const [snapshotRestoring, setSnapshotRestoring] = useState(false);
  const [snapshotRecoveryNotice, setSnapshotRecoveryNotice] = useState<string | null>(null);
  const [analysisSetupWorkspace, setAnalysisSetupWorkspace] = useState<import("./types/workspace").Workspace | null>(
    null,
  );
  const [analysisInitializing, setAnalysisInitializing] = useState(false);
  const [analysisProgress, setAnalysisProgress] = useState<AnalysisProgress>({ percent: 0, label: "분석 준비 중" });
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const [dbConnectionError, setDbConnectionError] = useState<string | null>(null);
  const [snapshotBootstrappedWorkspaceId, setSnapshotBootstrappedWorkspaceId] = useState<string | null>(null);
  const busyActionRef = useRef<string | null>(null);
  const busyNoticeTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (!import.meta.env.DEV || !hasTauriRuntime()) {
      return;
    }

    invoke<AppPaths>("get_app_paths")
      .then(setAppPaths)
      .catch((error: unknown) => setAppPathError(String(error)));
  }, []);

  useEffect(
    () => () => {
      if (busyNoticeTimerRef.current !== null) {
        window.clearTimeout(busyNoticeTimerRef.current);
      }
    },
    [],
  );

  async function withBusy(action: string, task: () => Promise<void>) {
    if (busyActionRef.current) {
      setBusyNotice(`현재 ${busyActionLabel(busyActionRef.current)} 작업이 진행 중입니다. 완료 후 다시 시도하세요.`);
      if (busyNoticeTimerRef.current !== null) {
        window.clearTimeout(busyNoticeTimerRef.current);
      }
      busyNoticeTimerRef.current = window.setTimeout(() => {
        setBusyNotice(null);
        busyNoticeTimerRef.current = null;
      }, 2600);
      return;
    }

    busyActionRef.current = action;
    setBusyNotice(null);
    setLatestOperationAction(action);
    setBusyAction(action);
    try {
      await task();
    } finally {
      if (busyActionRef.current === action) {
        busyActionRef.current = null;
        setLatestOperationAction(action);
        setBusyAction(null);
      }
    }
  }

  const workspaces = useWorkspaces({ withBusy });
  const { engineRegistry, engineError } = useEngineRegistry();
  const visual = useVisualMap({
    currentWorkspaceId: workspaces.currentWorkspace?.id ?? null,
    bootstrapReady: snapshotBootstrappedWorkspaceId === (workspaces.currentWorkspace?.id ?? null),
    onOperation: setLatestOperationAction,
  });
  async function refreshInventorySnapshot(workspaceId: string) {
    if (await visual.refreshInventorySnapshot(workspaceId)) {
      setSnapshotBootstrappedWorkspaceId(workspaceId);
      setSnapshotRecoveryNotice(null);
    }
  }
  const code = useCodeInventory({
    currentWorkspace: workspaces.currentWorkspace,
    withBusy,
    setCurrentWorkspace: workspaces.setCurrentWorkspace,
    refreshWorkspaces: workspaces.refreshWorkspaces,
    refreshInventorySnapshot,
  });
  const db = useDbProfiles({
    currentWorkspace: workspaces.currentWorkspace,
    withBusy,
    setCurrentWorkspace: workspaces.setCurrentWorkspace,
    refreshWorkspaces: workspaces.refreshWorkspaces,
    clearVisualMap: visual.clearVisualMap,
    refreshInventorySnapshot,
  });

  useEffect(() => {
    if (!code.codeInventory && !db.dbInventory) {
      return;
    }
    return scheduleSearchIndex(code.codeInventory, db.dbInventory);
  }, [code.codeInventory, db.dbInventory]);

  useLayoutEffect(() => {
    const workspace = workspaces.currentWorkspace;
    if (!workspace) {
      setSnapshotBootstrappedWorkspaceId(null);
      setSnapshotRestoring(false);
      setAnalysisSetupWorkspace(null);
      return;
    }

    setSnapshotRecoveryNotice(null);
    setAnalysisError(null);
    setAnalysisSetupWorkspace(null);
    setSnapshotBootstrappedWorkspaceId(null);
    setSnapshotRestoring(true);
    let cancelled = false;
    let needsAnalysisSetup = false;
    void invoke<unknown>("load_inventory_bootstrap", { workspaceId: workspace.id })
      .then((value) => {
        const bootstrap = validateInventoryBootstrap(value);
        if (cancelled) {
          return;
        }
        if (!bootstrap) {
          needsAnalysisSetup = true;
          return;
        }
        const { snapshot, summary } = bootstrap;
        visual.noteSnapshotLoaded(snapshot);

        const restoredCode = codeInventoryFromSnapshot(snapshot, workspace.codeProject ?? workspace.name, summary);
        const restoredDb = dbInventoryFromSnapshot(
          snapshot,
          db.activeProfile?.id ?? workspace.activeDbProfileId ?? "snapshot",
          summary,
        );
        if (codeInventoryItemCount(restoredCode) > 0) {
          code.restoreCodeInventory(restoredCode);
        }
        if (restoredDb.tables.length) {
          db.restoreDbInventory(restoredDb, null);
        }
        setSnapshotBootstrappedWorkspaceId(workspace.id);
      })
      .catch(() => {
        if (!cancelled) {
          setSnapshotRecoveryNotice("저장된 읽기 결과를 확인할 수 없습니다. 코드와 DB를 다시 읽어 주세요.");
        }
      })
      .finally(() => {
        if (!cancelled) {
          setSnapshotRestoring(false);
          if (needsAnalysisSetup) {
            setAnalysisSetupWorkspace(workspace);
          }
        }
      });

    return () => {
      cancelled = true;
    };
    // The workspace id is the lifecycle boundary. Control objects are recreated by their hooks
    // and adding them here would re-run snapshot restoration on every inventory/state update.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaces.currentWorkspace?.id]);

  useEffect(() => {
    const workspaceId = workspaces.currentWorkspace?.id;
    if (!workspaceId || snapshotBootstrappedWorkspaceId !== workspaceId || !hasTauriRuntime()) {
      return;
    }
    let cancelled = false;

    const refreshFreshness = () => {
      void invoke<unknown>("refresh_snapshot_freshness", { workspaceId })
        .then((value) => validateStringArray(value, "스냅샷 최신성 결과"))
        .then((reasons) => {
          if (cancelled) {
            return;
          }
          visual.noteSnapshotFreshness(reasons);
          setSnapshotRecoveryNotice(null);
        })
        .catch(() => {
          // A workspace without a saved snapshot has nothing to refresh yet.
        });
    };

    window.addEventListener("focus", refreshFreshness);
    // The app is normally already focused when a workspace is restored. Do
    // not wait for a later focus transition to reveal changes made while it
    // was closed.
    refreshFreshness();
    return () => {
      cancelled = true;
      window.removeEventListener("focus", refreshFreshness);
    };
    // visual is a hook result object; the workspace/bootstrap ids intentionally gate this refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshotBootstrappedWorkspaceId, workspaces.currentWorkspace?.id]);

  const activeBusyAction =
    busyAction ?? (analysisInitializing ? "workspace-initialize" : snapshotRestoring ? "snapshot-restore" : null);
  const busy = Boolean(activeBusyAction);
  const repoPathError = repoPathErrorFor(workspaces.repoPath, workspaces.repoSourceMode);
  const currentStatus = currentOperationStatus({
    busyAction: activeBusyAction,
    latestAction: latestOperationAction,
    workspaceStatus: workspaces.workspaceStatus,
    workspaceError: workspaces.workspaceError,
    codeStatus: code.codeStatus,
    codeError: code.codeError,
    codeErrorDetail: code.codeErrorDetail,
    dbStatus: db.dbStatus,
    dbError: db.dbError,
    dbErrorDetail: db.dbErrorDetail,
    mapStatus: visual.visualMapStatus,
    mapLoading: visual.visualMapLoading,
    mapError: visual.visualMapError,
    mapErrorDetail: visual.visualMapErrorDetail,
  });
  const operationStatus =
    snapshotRecoveryNotice && currentStatus.phase !== "running" && currentStatus.phase !== "error"
      ? { phase: "error" as const, label: "저장 결과", message: snapshotRecoveryNotice }
      : currentStatus;
  async function refreshGithubWorkspace() {
    if (await workspaces.refreshGithubWorkspace()) {
      await code.indexCodeRepository();
    }
  }
  async function startWorkspaceAnalysis(choice: AnalysisSetupChoice) {
    const setupWorkspace = analysisSetupWorkspace;
    if (!setupWorkspace) {
      return;
    }

    const connectDb = choice !== "code-only";
    const sourceUsesPath = dbProfileSourceUsesPath(dbProfileControls.profileSource);
    if (connectDb) {
      if (!dbProfileControls.profileName.trim()) {
        setAnalysisError("DB 연결 이름을 입력하세요");
        return;
      }
      if (sourceUsesPath && !dbProfileControls.profilePath.trim()) {
        setAnalysisError("DB 파일 또는 DDL 경로를 입력하세요");
        return;
      }
      if (!sourceUsesPath && !dbProfileControls.connectionString.trim()) {
        setAnalysisError("이번 분석에 사용할 DB 연결 문자열을 입력하세요");
        return;
      }
    }

    setAnalysisError(null);
    setAnalysisProgress({ percent: 5, label: "분석 준비 중" });
    setAnalysisInitializing(true);
    setSourceManagerOpen(false);
    await withBusy("workspace-initialize", async () => {
      try {
        let configuredWorkspace = setupWorkspace;
        if (connectDb) {
          const saveRequest: SaveDbProfileRequest = {
            workspaceId: setupWorkspace.id,
            name: dbProfileControls.profileName.trim(),
            source: dbProfileControls.profileSource,
            path: sourceUsesPath ? dbProfileControls.profilePath.trim() : null,
          };
          configuredWorkspace = validateWorkspace(await invoke<unknown>("save_db_profile", { request: saveRequest }));
          workspaces.setCurrentWorkspace(configuredWorkspace);
          await workspaces.refreshWorkspaces(configuredWorkspace.id);
          setAnalysisProgress({ percent: 20, label: "DB 연결 준비 완료" });
        }

        setAnalysisProgress({ percent: connectDb ? 30 : 20, label: "코드·DB 구조 분석 중", determinate: false });

        const request: InitializeWorkspaceAnalysisRequest = {
          workspaceId: configuredWorkspace.id,
          analysisMode: choice,
          dbProfileId: connectDb ? configuredWorkspace.activeDbProfileId : null,
          connectionString: connectDb && !sourceUsesPath ? dbProfileControls.connectionString.trim() : null,
        };
        const result = validateWorkspaceAnalysisResult(
          await invoke<unknown>("initialize_workspace_analysis", { request }),
        );
        setAnalysisProgress({ percent: 86, label: "분석 결과 정리 중", determinate: true });
        workspaces.setCurrentWorkspace(result.workspace);
        if (result.code?.inventory) {
          code.restoreCodeInventory(result.code.inventory, result.workspace.id);
        } else {
          code.clearCodeInventory();
        }
        if (result.db?.inventory) {
          db.restoreDbInventory(result.db.inventory, null, result.workspace.id);
        } else {
          db.clearDbInventory();
        }
        await workspaces.refreshWorkspaces(result.workspace.id);

        const partialErrors = [
          choice !== "db-only" ? result.codeError : null,
          connectDb ? result.dbError : null,
        ].filter(Boolean);
        if (!result.snapshotSaved || partialErrors.length > 0) {
          setAnalysisError(partialErrors.join("\n") || "통합 스냅샷을 저장하지 못했습니다");
          return;
        }

        setAnalysisProgress({ percent: 94, label: "시각화 준비 중" });
        setAnalysisSetupWorkspace(null);
        await refreshInventorySnapshot(result.workspace.id);
        setAnalysisProgress({ percent: 100, label: "시각화 준비 완료" });
      } catch (error) {
        const uiError = toUserError(error, "프로젝트 분석을 시작하지 못했습니다");
        setAnalysisError(uiError.message);
      } finally {
        setAnalysisInitializing(false);
      }
    });
    setAnalysisInitializing(false);
  }

  async function saveDbConnection(): Promise<boolean> {
    const workspace = workspaces.currentWorkspace;
    if (!workspace) {
      setDbConnectionError("프로젝트를 먼저 여세요");
      return false;
    }

    const sourceUsesPath = dbProfileSourceUsesPath(dbProfileControls.profileSource);
    if (!dbProfileControls.profileName.trim()) {
      setDbConnectionError("DB 연결 이름을 입력하세요");
      return false;
    }
    if (sourceUsesPath && !dbProfileControls.profilePath.trim()) {
      setDbConnectionError("DB 파일 또는 DDL 경로를 입력하세요");
      return false;
    }

    let saved = false;
    setDbConnectionError(null);
    await withBusy("db-save", async () => {
      try {
        const request: SaveDbProfileRequest = {
          workspaceId: workspace.id,
          name: dbProfileControls.profileName.trim(),
          source: dbProfileControls.profileSource,
          path: sourceUsesPath ? dbProfileControls.profilePath.trim() : null,
        };
        const updated = validateWorkspace(await invoke<unknown>("save_db_profile", { request }));
        workspaces.setCurrentWorkspace(updated);
        db.clearDbInventory();
        visual.clearVisualMap();
        await workspaces.refreshWorkspaces(updated.id);
        saved = true;
      } catch (error) {
        setDbConnectionError(toUserError(error, "DB 연결을 저장하지 못했습니다").message);
      }
    });
    return saved;
  }
  const workspaceControls = buildWorkspaceControls({
    operationStatus,
    repoPathError,
    workspaces,
    code,
    engineRegistry,
    engineError,
    busy,
    busyAction: activeBusyAction,
    refreshGithubWorkspace: () => void refreshGithubWorkspace(),
  });
  const dbProfileControls = buildDbProfileControls({
    hasWorkspace: Boolean(workspaces.currentWorkspace),
    db,
    engineRegistry,
    engineError,
    code,
    visual,
    busy,
    busyAction: activeBusyAction,
  });
  const visualMapControls = buildVisualMapControls({ visual, code, db });
  const devSlot =
    import.meta.env.DEV && hasTauriRuntime() ? <DevDiagnostics paths={appPaths} error={appPathError} /> : null;

  return (
    <MapWorkspace
      sourceManagerOpen={sourceManagerOpen}
      setSourceManagerOpen={setSourceManagerOpen}
      workspaceControls={workspaceControls}
      dbProfileControls={dbProfileControls}
      visualMapControls={visualMapControls}
      engineRegistry={engineRegistry}
      engineError={engineError}
      devSlot={devSlot}
      busyNotice={busyNotice}
      analysisSetupWorkspace={analysisSetupWorkspace}
      analysisInitializing={analysisInitializing}
      analysisProgress={analysisProgress}
      analysisError={analysisError}
      onStartAnalysis={startWorkspaceAnalysis}
      onCancelAnalysis={() => setAnalysisSetupWorkspace(null)}
      onSaveDbConnection={saveDbConnection}
      dbConnectionError={dbConnectionError}
      onOpenDbConnection={() => setDbConnectionError(null)}
    />
  );
}

function busyActionLabel(action: string): string {
  const labels: Record<string, string> = {
    "workspace-create": "프로젝트 생성",
    "workspace-open": "프로젝트 열기",
    "workspace-refresh": "프로젝트 새로 읽기",
    "workspace-repair": "프로젝트 복구",
    "workspace-delete": "프로젝트 삭제",
    "workspace-clone": "저장소 복제",
    "code-index": "코드 분석",
    "db-save": "DB 연결 저장",
    "db-index": "DB 분석",
    "db-delete": "DB 연결 삭제",
    "workspace-initialize": "프로젝트 분석",
    "snapshot-restore": "저장 결과 복원",
  };
  return labels[action] ?? "이전 작업";
}

export default App;
