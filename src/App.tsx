import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import "./styles/index.css";
import type { AppPaths } from "./components/common/DevDiagnostics";
import { DevDiagnostics } from "./components/common/DevDiagnostics";
import { WorkbenchView } from "./components/workbench/WorkbenchView";
import { currentOperationStatus, repoPathErrorFor } from "./app/appState";
import { toUserError } from "./app/operationStatus";
import { buildDbProfileControls, buildVisualMapControls, buildWorkspaceControls } from "./app/controlBuilders";
import { hasTauriRuntime } from "./app/tauriRuntime";
import { useCodeInventory } from "./hooks/useCodeInventory";
import { useDbProfiles } from "./hooks/useDbProfiles";
import { useEngineRegistry } from "./hooks/useEngineRegistry";
import { useVisualMap } from "./hooks/useVisualMap";
import { useWorkspaces } from "./hooks/useWorkspaces";
import { codeInventoryFromSnapshot, dbInventoryFromSnapshot } from "./inventory/snapshotRestore";
import { dbProfileSourceUsesPath, codeInventoryItemCount } from "./types/workspace";
import type {
  InitializeWorkspaceAnalysisRequest,
  SaveDbProfileRequest,
  WorkspaceAnalysisResult,
} from "./types/workspace";
import type { InventoryBootstrap } from "./types/visual-map";
import { prepareSearchIndex } from "./visual/search";
import type { AnalysisSetupChoice } from "./components/workbench/ProjectAnalysisSetupModal";

function App() {
  const [sourceManagerOpen, setSourceManagerOpen] = useState(false);
  const [appPaths, setAppPaths] = useState<AppPaths | null>(null);
  const [appPathError, setAppPathError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [latestOperationAction, setLatestOperationAction] = useState<string | null>(null);
  const [snapshotRestoring, setSnapshotRestoring] = useState(false);
  const [snapshotRecoveryNotice, setSnapshotRecoveryNotice] = useState<string | null>(null);
  const [analysisSetupWorkspace, setAnalysisSetupWorkspace] = useState<import("./types/workspace").Workspace | null>(null);
  const [analysisInitializing, setAnalysisInitializing] = useState(false);
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const [dbConnectionError, setDbConnectionError] = useState<string | null>(null);
  const busyActionRef = useRef<string | null>(null);

  useEffect(() => {
    if (!import.meta.env.DEV || !hasTauriRuntime()) {
      return;
    }

    invoke<AppPaths>("get_app_paths")
      .then(setAppPaths)
      .catch((error: unknown) => setAppPathError(String(error)));
  }, []);

  async function withBusy(action: string, task: () => Promise<void>) {
    if (busyActionRef.current) {
      return;
    }

    busyActionRef.current = action;
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
    onOperation: setLatestOperationAction,
  });
  async function refreshInventorySnapshot(workspaceId: string) {
    if (await visual.refreshInventorySnapshot(workspaceId)) {
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
    const timer = window.setTimeout(() => prepareSearchIndex(code.codeInventory, db.dbInventory), 0);
    return () => window.clearTimeout(timer);
  }, [code.codeInventory, db.dbInventory]);

  useLayoutEffect(() => {
    const workspace = workspaces.currentWorkspace;
    if (!workspace) {
      setSnapshotRestoring(false);
      setAnalysisSetupWorkspace(null);
      return;
    }

    setSnapshotRecoveryNotice(null);
    setAnalysisError(null);
    setAnalysisSetupWorkspace(null);
    setSnapshotRestoring(true);
    let cancelled = false;
    let needsAnalysisSetup = false;
    void invoke<InventoryBootstrap | null>("load_inventory_bootstrap", { workspaceId: workspace.id })
      .then((bootstrap) => {
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
  }, [workspaces.currentWorkspace?.id]);

  useEffect(() => {
    const workspaceId = workspaces.currentWorkspace?.id;
    if (!workspaceId || !hasTauriRuntime()) {
      return;
    }

    const refreshFreshness = () => {
      void invoke<string[]>("refresh_snapshot_freshness", { workspaceId })
        .then((reasons) => {
          visual.noteSnapshotFreshness(reasons);
          setSnapshotRecoveryNotice(null);
        })
        .catch(() => {
          // A workspace without a saved snapshot has nothing to refresh yet.
        });
    };

    window.addEventListener("focus", refreshFreshness);
    return () => window.removeEventListener("focus", refreshFreshness);
  }, [workspaces.currentWorkspace?.id]);

  const activeBusyAction = busyAction ?? (analysisInitializing ? "workspace-initialize" : snapshotRestoring ? "snapshot-restore" : null);
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
          configuredWorkspace = await invoke<import("./types/workspace").Workspace>("save_db_profile", { request: saveRequest });
          workspaces.setCurrentWorkspace(configuredWorkspace);
          await workspaces.refreshWorkspaces(configuredWorkspace.id);
        }

        const request: InitializeWorkspaceAnalysisRequest = {
          workspaceId: configuredWorkspace.id,
          analysisMode: choice,
          dbProfileId: connectDb ? configuredWorkspace.activeDbProfileId : null,
          connectionString: connectDb && !sourceUsesPath ? dbProfileControls.connectionString.trim() : null,
        };
        const result = await invoke<WorkspaceAnalysisResult>("initialize_workspace_analysis", { request });
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

        const partialErrors = [choice !== "db-only" ? result.codeError : null, connectDb ? result.dbError : null].filter(Boolean);
        if (!result.snapshotSaved || partialErrors.length > 0) {
          setAnalysisError(partialErrors.join("\n") || "통합 스냅샷을 저장하지 못했습니다");
          return;
        }

        setAnalysisSetupWorkspace(null);
        await refreshInventorySnapshot(result.workspace.id);
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
        const updated = await invoke<import("./types/workspace").Workspace>("save_db_profile", { request });
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
  const devSlot = import.meta.env.DEV && hasTauriRuntime() ? <DevDiagnostics paths={appPaths} error={appPathError} /> : null;

  return (
    <WorkbenchView
      sourceManagerOpen={sourceManagerOpen}
      setSourceManagerOpen={setSourceManagerOpen}
      workspaceControls={workspaceControls}
      dbProfileControls={dbProfileControls}
      visualMapControls={visualMapControls}
      engineRegistry={engineRegistry}
      engineError={engineError}
      devSlot={devSlot}
      analysisSetupWorkspace={analysisSetupWorkspace}
      analysisInitializing={analysisInitializing}
      analysisError={analysisError}
      onStartAnalysis={startWorkspaceAnalysis}
      onCancelAnalysis={() => setAnalysisSetupWorkspace(null)}
      onOpenAnalysis={() => {
        if (workspaces.currentWorkspace) {
          setAnalysisError(null);
          setAnalysisSetupWorkspace(workspaces.currentWorkspace);
        }
      }}
      onSaveDbConnection={saveDbConnection}
      dbConnectionError={dbConnectionError}
      onOpenDbConnection={() => setDbConnectionError(null)}
    />
  );
}

export default App;
