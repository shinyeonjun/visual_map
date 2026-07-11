import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import "./styles/index.css";
import type { AppPaths } from "./components/common/DevDiagnostics";
import { DevDiagnostics } from "./components/common/DevDiagnostics";
import type { View } from "./components/common/ViewSwitch";
import { AtlasView } from "./components/atlas/AtlasView";
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
import { codeInventoryItemCount, dbInventoryTableKey, type CodeInventory, type DbInventory, type DbProfile, type Workspace } from "./types/workspace";
import type { InventorySnapshot } from "./types/visual-map";

function App() {
  const [view, setViewState] = useState<View>("workbench");
  const [appPaths, setAppPaths] = useState<AppPaths | null>(null);
  const [appPathError, setAppPathError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
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

  function setView(view: View) {
    setViewState(view);
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
    const workspace = workspaces.currentWorkspace;
    if (!workspace) {
      return;
    }

    setSnapshotRecoveryNotice(null);
    let cancelled = false;
    const restoreCachedInventory = async (fallbackSnapshot?: InventorySnapshot) => {
      const expectsCode = fallbackSnapshot ? snapshotHasSource(fallbackSnapshot, "code") : false;
      const expectsDb = fallbackSnapshot ? snapshotHasSource(fallbackSnapshot, "db") : false;
      const restoredCode = await cachedCodeInventory(workspace, () => cancelled);
      if (cancelled) {
        return;
      }

      const profile = indexedDbProfile(workspace);
      const restoredDb = profile ? await cachedDbInventory(workspace, profile, () => cancelled) : null;
      if (cancelled) {
        return;
      }

      const fallbackCode = fallbackSnapshot
        ? codeInventoryFromSnapshot(fallbackSnapshot, workspace.codeProject ?? workspace.name)
        : null;
      const fallbackDb = fallbackSnapshot
        ? dbInventoryFromSnapshot(
            fallbackSnapshot,
            db.activeProfile?.id ?? workspace.activeDbProfileId ?? "snapshot",
          )
        : null;
      const codeCacheRecovered =
        restoredCode !== null &&
        (!fallbackSnapshot?.items.some((item) => item.source === "code") || codeInventoryItemCount(restoredCode) > 0);
      const dbCacheRecovered =
        restoredDb !== null &&
        (!fallbackSnapshot?.items.some((item) => item.source === "db") || restoredDb.tables.length > 0);
      if (codeCacheRecovered && restoredCode && codeInventoryItemCount(restoredCode) > 0) {
        code.restoreCodeInventory(restoredCode);
      } else if (expectsCode && fallbackCode && codeInventoryItemCount(fallbackCode) > 0) {
        code.restoreCodeInventory(fallbackCode);
      }
      if (dbCacheRecovered && restoredDb?.tables.length) {
        db.restoreDbInventory(restoredDb, dbInventoryTableKey(restoredDb.tables[0]));
      } else if (expectsDb && fallbackDb?.tables.length) {
        db.restoreDbInventory(fallbackDb, null);
      }

      const recoveredEveryExpectedSource = (!expectsCode || codeCacheRecovered) && (!expectsDb || dbCacheRecovered);
      if ((restoredCode || restoredDb) && (!fallbackSnapshot || recoveredEveryExpectedSource)) {
        setSnapshotRecoveryNotice(null);
        await visual.saveInventorySnapshot(workspace.id, restoredCode, restoredDb);
      } else if (fallbackSnapshot) {
        const missing = [expectsCode && !codeCacheRecovered ? "코드" : null, expectsDb && !dbCacheRecovered ? "DB" : null]
          .filter(Boolean)
          .join("·");
        visual.noteSnapshotLoaded(fallbackSnapshot.savedAt);
        setSnapshotRecoveryNotice(
          `${missing || "일부"} 캐시를 복구하지 못해 기존 snapshot을 보존했습니다. 코드와 DB를 다시 읽어 주세요.`,
        );
      }
    };

    void invoke<InventorySnapshot>("load_inventory_snapshot", { workspaceId: workspace.id })
      .then((snapshot) => {
        if (cancelled) {
          return;
        }
        if (snapshot.staleReasons?.length) {
          void restoreCachedInventory(snapshot);
          return;
        }
        visual.noteSnapshotLoaded(snapshot.savedAt);

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
        void restoreCachedInventory();
      });

    return () => {
      cancelled = true;
    };
  }, [workspaces.currentWorkspace?.id]);

  const busy = Boolean(busyAction);
  const repoPathError = repoPathErrorFor(workspaces.repoPath, workspaces.repoSourceMode);
  const currentStatus = currentOperationStatus({
    busyAction,
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
    busyAction,
  });
  const dbProfileControls = buildDbProfileControls({
    hasWorkspace: Boolean(workspaces.currentWorkspace),
    db,
    engineRegistry,
    engineError,
    code,
    visual,
    busy,
    busyAction,
  });
  const visualMapControls = buildVisualMapControls({ visual, code, db });
  const devSlot = import.meta.env.DEV && hasTauriRuntime() ? <DevDiagnostics paths={appPaths} error={appPathError} /> : null;

  return view === "workbench" ? (
    <WorkbenchView
      view={view}
      setView={setView}
      workspaceControls={workspaceControls}
      dbProfileControls={dbProfileControls}
      visualMapControls={visualMapControls}
      engineRegistry={engineRegistry}
      engineError={engineError}
      devSlot={devSlot}
    />
  ) : (
    <AtlasView
      view={view}
      setView={setView}
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

async function cachedCodeInventory(workspace: Workspace, cancelled: () => boolean): Promise<CodeInventory | null> {
  if (!workspace.codeProject || cancelled()) {
    return null;
  }

  try {
    return await invoke<CodeInventory>("get_code_inventory", { workspaceId: workspace.id });
  } catch {
    return null;
  }
}

async function cachedDbInventory(
  workspace: Workspace,
  profile: DbProfile,
  cancelled: () => boolean,
): Promise<DbInventory | null> {
  if (!profile.lastIndexedAt || cancelled()) {
    return null;
  }

  try {
    return await invoke<DbInventory>("get_db_inventory", { workspaceId: workspace.id, profileId: profile.id });
  } catch {
    return null;
  }
}

function indexedDbProfile(workspace: Workspace): DbProfile | null {
  const active = workspace.activeDbProfileId
    ? workspace.dbProfiles.find((profile) => profile.id === workspace.activeDbProfileId)
    : null;
  return active?.lastIndexedAt ? active : workspace.dbProfiles.find((profile) => profile.lastIndexedAt) ?? null;
}

function snapshotHasSource(snapshot: InventorySnapshot, source: "code" | "db"): boolean {
  return Boolean(snapshot.metadata?.[source]) || snapshot.items.some((item) => item.source === source);
}
