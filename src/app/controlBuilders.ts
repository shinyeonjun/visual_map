import type { useCodeInventory } from "../hooks/useCodeInventory";
import type { useDbProfiles } from "../hooks/useDbProfiles";
import type { useVisualMap } from "../hooks/useVisualMap";
import type { useWorkspaces } from "../hooks/useWorkspaces";
import type { OperationStatus } from "../types/operation";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../types/controls";
import type { EngineRegistry } from "../types/engine";
import {
  codeInventoryCodeItems,
  dbProfileSourceUsesPath,
  workspaceRepoInputValue,
  type CodeInventoryItem,
} from "../types/workspace";
import { dbColumnNodeId, dbTableNodeId, tableKeyFromDbNodeId } from "../visual/nodeIds";

type WorkspacesState = ReturnType<typeof useWorkspaces>;
type CodeState = ReturnType<typeof useCodeInventory>;
type DbState = ReturnType<typeof useDbProfiles>;
type VisualState = ReturnType<typeof useVisualMap>;

export function buildWorkspaceControls({
  operationStatus,
  repoPathError,
  workspaces,
  code,
  engineRegistry,
  engineError,
  busy,
  busyAction,
  refreshGithubWorkspace,
}: {
  operationStatus: OperationStatus;
  repoPathError: string | null;
  workspaces: WorkspacesState;
  code: CodeState;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  busy: boolean;
  busyAction: string | null;
  refreshGithubWorkspace: () => void;
}): WorkspaceControls {
  const currentWorkspaceMatchesForm = Boolean(
    workspaces.currentWorkspace &&
      workspaces.currentWorkspace.name === workspaces.workspaceName.trim() &&
      workspaceRepoInputValue(workspaces.currentWorkspace) === workspaces.repoPath.trim(),
  );
  const codeEngine = engineRegistry?.engines.find((engine) => engine.role === "code") ?? null;
  const codeIndexBlockedReason = engineError
    ? "코드 읽기 도구 상태 확인 오류"
    : engineRegistry && !codeEngine?.available
      ? codeEngine?.error ?? "코드 읽기 도구 설치 필요"
      : null;
  async function createWorkspaceAndReadCode() {
    const workspace = await workspaces.createWorkspace();
    if (workspace && !codeIndexBlockedReason) {
      await code.indexCodeRepository(workspace);
    }
  }

  return {
    initialized: workspaces.initialized,
    operationStatus,
    workspaces: workspaces.workspaces,
    recoveryWarnings: workspaces.recoveryWarnings,
    currentWorkspace: workspaces.currentWorkspace,
    repoSourceMode: workspaces.repoSourceMode,
    workspaceName: workspaces.workspaceName,
    repoPath: workspaces.repoPath,
    repoPathError,
    status: workspaces.workspaceStatus,
    error: workspaces.workspaceError,
    codeStatus: code.codeStatus,
    codeError: code.codeError,
    codeErrorDetail: code.codeErrorDetail,
    codeInventory: code.codeInventory,
    selectedCodeItem: code.selectedCodeItem,
    busy,
    creating: busyAction === "workspace-create",
    opening: busyAction === "workspace-open",
    refreshing: busyAction === "workspace-refresh",
    deleting: busyAction === "workspace-delete",
    codeIndexing: busyAction === "code-index",
    restoringSnapshot: busyAction === "snapshot-restore",
    canIndexCode: !codeIndexBlockedReason,
    codeIndexBlockedReason,
    canCreateWorkspace: Boolean(
      workspaces.workspaceName.trim() &&
        workspaces.repoPath.trim() &&
        !repoPathError &&
        !currentWorkspaceMatchesForm,
    ),
    setRepoSourceMode: workspaces.setRepoSourceMode,
    setWorkspaceName: workspaces.setWorkspaceName,
    setRepoPath: workspaces.setRepoPath,
    pickRepoPath: () => void workspaces.pickRepoPath(),
    createWorkspace: () => void createWorkspaceAndReadCode(),
    openWorkspace: workspaces.openWorkspace,
    refreshGithubWorkspace,
    indexCodeRepository: () => void code.indexCodeRepository(),
    refreshWorkspaces: () => void workspaces.refreshWorkspaces(),
    repairWorkspaceFromBackup: (workspaceId) => void workspaces.repairWorkspaceFromBackup(workspaceId),
    deleteWorkspace: (workspaceId) => void workspaces.deleteWorkspace(workspaceId),
  };
}

export function buildDbProfileControls({
  hasWorkspace,
  db,
  engineRegistry,
  engineError,
  code,
  visual,
  busy,
  busyAction,
}: {
  hasWorkspace: boolean;
  db: DbState;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  code: CodeState;
  visual: VisualState;
  busy: boolean;
  busyAction: string | null;
}): DbProfileControls {
  const activeProfileMatchesForm = Boolean(
    db.activeProfile &&
      db.activeProfile.name === db.dbProfileName.trim() &&
      db.activeProfile.source === db.dbProfileSource &&
      (!dbProfileSourceUsesPath(db.dbProfileSource) || (db.activeProfile.path ?? "") === db.dbProfilePath.trim()),
  );
  const dbEngine = engineRegistry?.engines.find((engine) => engine.role === "db") ?? null;
  const dbIndexBlockedReason = engineError
    ? "DB 읽기 도구 상태 확인 오류"
    : engineRegistry && !dbEngine?.available
      ? dbEngine?.error ?? "DB 읽기 도구 설치 필요"
      : null;
  const canRunDbEngine = !dbIndexBlockedReason;

  return {
    hasWorkspace,
    activeProfile: db.activeProfile,
    inventory: db.dbInventory,
    selectedTableKey: db.selectedDbTableKey,
    profileName: db.dbProfileName,
    profileSource: db.dbProfileSource,
    profilePath: db.dbProfilePath,
    connectionString: db.dbConnectionString,
    status: db.dbStatus,
    error: db.dbError,
    errorDetail: db.dbErrorDetail,
    busy,
    saving: busyAction === "db-save",
    indexing: busyAction === "db-index",
    deleting: busyAction === "db-delete",
    canSaveProfile: Boolean(
      !activeProfileMatchesForm &&
        db.dbProfileName.trim() &&
        (!dbProfileSourceUsesPath(db.dbProfileSource) || db.dbProfilePath.trim()),
    ),
    canIndexProfile: Boolean(
      canRunDbEngine &&
      activeProfileMatchesForm &&
        db.activeProfile &&
        (dbProfileSourceUsesPath(db.activeProfile.source) || db.dbConnectionString.trim()),
    ),
    dbIndexBlockedReason,
    setProfileName: db.setDbProfileName,
    setProfileSource: db.setDbProfileSource,
    setProfilePath: db.setDbProfilePath,
    setConnectionString: db.setDbConnectionString,
    pickPath: (directory) => void db.pickDbPath(directory),
    saveProfile: () => void db.saveDbProfile(),
    indexProfile: () => void db.indexDbProfile(),
    deleteProfile: () => void db.deleteDbProfile(),
    openTable: (tableKey) => {
      db.setSelectedDbTableKey(tableKey);
      code.setSelectedCodeItem(null);
      visual.showMapMode("table-usage", dbTableNodeId(tableKey));
    },
    openColumn: (tableKey, columnName) => {
      db.setSelectedDbTableKey(tableKey);
      code.setSelectedCodeItem(null);
      visual.showMapMode("column-impact", dbColumnNodeId(tableKey, columnName));
    },
  };
}

export function buildVisualMapControls({
  visual,
  code,
  db,
}: {
  visual: VisualState;
  code: CodeState;
  db: DbState;
}): VisualMapControls {
  const selectCodeOnly = (item: CodeInventoryItem) => {
    code.setSelectedCodeItem(item);
    db.setSelectedDbTableKey(null);
  };
  const selectDbOnly = (tableKey: string) => {
    db.setSelectedDbTableKey(tableKey);
    code.setSelectedCodeItem(null);
  };
  const showMode = (mode: string, focusId?: string | null) => {
    if (focusId?.startsWith("code:")) {
      code.setSelectedCodeItem(codeItemFromNodeId(focusId, code.codeInventory));
      db.setSelectedDbTableKey(null);
    } else {
      const tableKey = focusId ? tableKeyFromDbNodeId(focusId) : null;
      if (tableKey) {
        selectDbOnly(tableKey);
      } else {
        code.setSelectedCodeItem(null);
        db.setSelectedDbTableKey(null);
      }
    }
    visual.showMapMode(mode, focusId);
  };

  return {
    currentMap: visual.visualMap,
    mode: visual.mapMode,
    focusId: visual.mapFocusId,
    compositionFocusIds: visual.compositionFocusIds,
    relationView: visual.relationView,
    loading: visual.visualMapLoading,
    enriching: visual.visualMapEnriching,
    changeIntent: visual.changeIntent,
    snapshotSavedAt: visual.snapshotSavedAt,
    snapshotStaleReasons: visual.snapshotStaleReasons,
    snapshotSourceSummary: visual.snapshotSourceSummary,
    analysisCoverage: visual.analysisCoverage,
    projectionElapsedMs: visual.projectionElapsedMs,
    searchQuery: visual.searchQuery,
    searchPopoverOpen: visual.searchPopoverOpen,
    searchSummary: visual.searchSummary,
    searchGroups: visual.searchGroups,
    selectedNode: visual.selectedVisualNode,
    selectedEdge: visual.selectedVisualEdge,
    setSearchQuery: (value) =>
      visual.setSearchQuery(value, {
        codeInventory: code.codeInventory,
        dbInventory: db.dbInventory,
        selectCodeItem: selectCodeOnly,
        selectDbTable: selectDbOnly,
      }),
    showMode,
    toggleCompositionFocus: visual.toggleCompositionFocus,
    clearCompositionFocus: visual.clearCompositionFocus,
    setRelationView: visual.setRelationView,
    setChangeIntent: visual.setChangeIntent,
    runSearch: () =>
      visual.runSearch({
        codeInventory: code.codeInventory,
        dbInventory: db.dbInventory,
        selectCodeItem: selectCodeOnly,
        selectDbTable: selectDbOnly,
      }),
    selectSearchResult: visual.selectSearchResult,
    openSearchPopover: visual.openSearchPopover,
    closeSearchPopover: visual.closeSearchPopover,
    selectNode: visual.setSelectedVisualNode,
    selectEdge: visual.setSelectedVisualEdge,
    clearSelection: visual.clearVisualSelection,
  };
}

function codeItemFromNodeId(nodeId: string, inventory: CodeState["codeInventory"]): CodeInventoryItem | null {
  if (!nodeId.startsWith("code:")) {
    return null;
  }
  const id = nodeId.slice("code:".length);
  return (
    inventory?.routes.find((item) => item.id === id) ??
    codeInventoryCodeItems(inventory).find((item) => item.id === id) ??
    inventory?.files.find((item) => item.id === id) ??
    null
  );
}
