import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import "./styles/index.css";
import type { AppPaths } from "./components/common/DevDiagnostics";
import { DevDiagnostics } from "./components/common/DevDiagnostics";
import { WorkbenchView } from "./components/workbench/WorkbenchView";
import { currentOperationStatus, repoPathErrorFor } from "./app/appState";
import { buildDbProfileControls, buildVisualMapControls, buildWorkspaceControls } from "./app/controlBuilders";
import { hasTauriRuntime } from "./app/tauriRuntime";
import { useCodeInventory } from "./hooks/useCodeInventory";
import { useDbProfiles } from "./hooks/useDbProfiles";
import { useEngineRegistry } from "./hooks/useEngineRegistry";
import { useVisualMap } from "./hooks/useVisualMap";
import { useWorkspaces } from "./hooks/useWorkspaces";
import { codeInventoryFromSnapshot, dbInventoryFromSnapshot } from "./inventory/snapshotRestore";
import { codeInventoryItemCount } from "./types/workspace";
import type { InventoryBootstrap } from "./types/visual-map";
import { prepareSearchIndex } from "./visual/search";

function App() {
  const [sourceManagerOpen, setSourceManagerOpen] = useState(false);
  const [appPaths, setAppPaths] = useState<AppPaths | null>(null);
  const [appPathError, setAppPathError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [latestOperationAction, setLatestOperationAction] = useState<string | null>(null);
  const [snapshotRestoring, setSnapshotRestoring] = useState(false);
  const [snapshotRecoveryNotice, setSnapshotRecoveryNotice] = useState<string | null>(null);
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
      return;
    }

    setSnapshotRecoveryNotice(null);
    setSnapshotRestoring(true);
    let cancelled = false;
    void invoke<InventoryBootstrap | null>("load_inventory_bootstrap", { workspaceId: workspace.id })
      .then((bootstrap) => {
        if (cancelled || !bootstrap) {
          return;
        }
        const { snapshot, summary } = bootstrap;
        if (snapshot.staleReasons?.length) {
          visual.noteSnapshotLoaded(snapshot);
          return;
        }
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

  const activeBusyAction = busyAction ?? (snapshotRestoring ? "snapshot-restore" : null);
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
    />
  );
}

export default App;
