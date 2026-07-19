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
import { codeInventoryItemCount, type CodeInventory, type DbInventory } from "./types/workspace";
import type { InventorySnapshot } from "./types/visual-map";
import { prepareSearchIndex } from "./visual/search";

function App() {
  const [sourceManagerOpen, setSourceManagerOpen] = useState(false);
  const [appPaths, setAppPaths] = useState<AppPaths | null>(null);
  const [appPathError, setAppPathError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [snapshotRestoring, setSnapshotRestoring] = useState(false);
  const [snapshotRecoveryNotice, setSnapshotRecoveryNotice] = useState<string | null>(null);
  const busyActionRef = useRef<string | null>(null);
  const codeInventoryRef = useRef<CodeInventory | null>(null);
  const dbInventoryRef = useRef<DbInventory | null>(null);

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
    setBusyAction(action);
    try {
      await task();
    } finally {
      if (busyActionRef.current === action) {
        busyActionRef.current = null;
        setBusyAction(null);
      }
    }
  }

  const workspaces = useWorkspaces({ withBusy });
  const { engineRegistry, engineError } = useEngineRegistry();
  const visual = useVisualMap({ currentWorkspaceId: workspaces.currentWorkspace?.id ?? null });
  async function saveInventorySnapshot(
    workspaceId: string,
    codeInventory: CodeInventory | null,
    dbInventory: DbInventory | null,
  ) {
    if (await visual.saveInventorySnapshot(workspaceId, codeInventory, dbInventory)) {
      setSnapshotRecoveryNotice(null);
    }
  }
  const code = useCodeInventory({
    currentWorkspace: workspaces.currentWorkspace,
    withBusy,
    setCurrentWorkspace: workspaces.setCurrentWorkspace,
    refreshWorkspaces: workspaces.refreshWorkspaces,
    saveInventorySnapshot,
    getDbInventory: () => dbInventoryRef.current,
  });
  const db = useDbProfiles({
    currentWorkspace: workspaces.currentWorkspace,
    withBusy,
    setCurrentWorkspace: workspaces.setCurrentWorkspace,
    refreshWorkspaces: workspaces.refreshWorkspaces,
    clearVisualMap: visual.clearVisualMap,
    saveInventorySnapshot,
    getCodeInventory: () => codeInventoryRef.current,
  });

  codeInventoryRef.current = code.codeInventory;
  dbInventoryRef.current = db.dbInventory;

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
    void invoke<InventorySnapshot | null>("load_inventory_snapshot", { workspaceId: workspace.id })
      .then((snapshot) => {
        if (cancelled || !snapshot) {
          return;
        }
        if (snapshot.staleReasons?.length) {
          visual.noteSnapshotLoaded(snapshot);
          setSnapshotRecoveryNotice(
            `읽은 결과가 최신이 아닙니다: ${snapshot.staleReasons.join(", ")}. 코드와 DB를 다시 읽어 주세요.`,
          );
          return;
        }
        visual.noteSnapshotLoaded(snapshot);

        const restoredCode = codeInventoryFromSnapshot(snapshot, workspace.codeProject ?? workspace.name);
        const restoredDb = dbInventoryFromSnapshot(
          snapshot,
          db.activeProfile?.id ?? workspace.activeDbProfileId ?? "snapshot",
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

  const activeBusyAction = busyAction ?? (snapshotRestoring ? "snapshot-restore" : null);
  const busy = Boolean(activeBusyAction);
  const repoPathError = repoPathErrorFor(workspaces.repoPath, workspaces.repoSourceMode);
  const currentStatus = currentOperationStatus({
    busyAction: activeBusyAction,
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
      ? { phase: "error" as const, label: "Snapshot", message: snapshotRecoveryNotice }
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
    db,
    visual,
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
