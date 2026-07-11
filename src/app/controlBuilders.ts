import type { useCodeInventory } from "../hooks/useCodeInventory";
import type { useDbProfiles } from "../hooks/useDbProfiles";
import type { useVisualMap } from "../hooks/useVisualMap";
import type { useWorkspaces } from "../hooks/useWorkspaces";
import type { OperationStatus } from "../types/operation";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../types/controls";
import type { EngineRegistry } from "../types/engine";
import { codeInventoryCodeItems, dbProfileSourceUsesPath, type CodeInventoryItem } from "../types/workspace";
import type { VisualEdge, VisualNode } from "../types/visual-map";

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
  db,
  visual,
  busy,
  busyAction,
}: {
  operationStatus: OperationStatus;
  repoPathError: string | null;
  workspaces: WorkspacesState;
  code: CodeState;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  db: DbState;
  visual: VisualState;
  busy: boolean;
  busyAction: string | null;
}): WorkspaceControls {
  const currentWorkspaceMatchesForm = Boolean(
    workspaces.currentWorkspace &&
      workspaces.currentWorkspace.name === workspaces.workspaceName.trim() &&
      workspaces.currentWorkspace.repoPath === workspaces.repoPath.trim(),
  );
  const codeEngine = engineRegistry?.engines.find((engine) => engine.role === "code") ?? null;
  const codeIndexBlockedReason = engineError
    ? "코드 읽기 도구 상태 확인 오류"
    : engineRegistry && !codeEngine?.available
      ? codeEngine?.error ?? "코드 읽기 도구 설치 필요"
      : null;

  return {
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
    codeIndexing: busyAction === "code-index",
    codeLoading: busyAction === "code-load",
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
    createWorkspace: workspaces.createWorkspace,
    openWorkspace: workspaces.openWorkspace,
    indexCodeRepository: () => void code.indexCodeRepository(),
    loadCodeInventory: () => void code.loadCodeInventory(),
    selectCodeItem: (item) => {
      code.setSelectedCodeItem(item);
      db.setSelectedDbTableKey(null);
      visual.showMapMode(codeItemMode(item), `code:${item.id}`);
    },
    refreshWorkspaces: () => void workspaces.refreshWorkspaces(),
    repairWorkspaceFromBackup: (workspaceId) => void workspaces.repairWorkspaceFromBackup(workspaceId),
  };
}

function codeItemMode(item: CodeInventoryItem): string {
  return item.kind === "route" || item.kind === "api" ? "api-flow" : "search-focus";
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
    testing: busyAction === "db-test",
    indexing: busyAction === "db-index",
    loading: busyAction === "db-load",
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
    canTestConnection: Boolean(
      canRunDbEngine &&
      activeProfileMatchesForm &&
        db.activeProfile &&
        (dbProfileSourceUsesPath(db.activeProfile.source) || db.dbConnectionString.trim()),
    ),
    dbIndexBlockedReason,
    canLoadInventory: Boolean(db.activeProfile),
    setProfileName: db.setDbProfileName,
    setProfileSource: db.setDbProfileSource,
    setProfilePath: db.setDbProfilePath,
    setConnectionString: db.setDbConnectionString,
    pickPath: () => void db.pickDbPath(),
    saveProfile: () => void db.saveDbProfile(),
    testConnection: () => void db.testDbConnection(),
    indexProfile: () => void db.indexDbProfile(),
    loadInventory: () => void db.loadDbInventory(),
    selectTable: (tableKey) => {
      db.setSelectedDbTableKey(tableKey);
      code.setSelectedCodeItem(null);
      visual.showMapMode("table-usage", `db:table:${tableKey}`);
    },
    selectColumn: (tableKey, columnName) => {
      db.setSelectedDbTableKey(tableKey);
      code.setSelectedCodeItem(null);
      visual.showMapMode("column-impact", `db:column:${tableKey}:${columnName}`);
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

  return {
    currentMap: visual.visualMap,
    mode: visual.mapMode,
    snapshotSavedAt: visual.snapshotSavedAt,
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
    showMode: visual.showMapMode,
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
    selectNode: (node: VisualNode) => {
      visual.setSelectedVisualNode(node);
      visual.setSelectedVisualEdge(null);
      if (node.id.startsWith("db:table:")) {
        selectDbOnly(node.id.slice("db:table:".length));
      } else if (node.id.startsWith("db:column:")) {
        const tableKey = tableKeyFromColumnNodeId(node.id);
        if (tableKey) {
          selectDbOnly(tableKey);
        }
      } else if (node.id.startsWith("code:")) {
        const item = codeItemFromNodeId(node.id, code.codeInventory);
        if (item) {
          selectCodeOnly(item);
        } else {
          db.setSelectedDbTableKey(null);
        }
      }
    },
    selectEdge: (edge: VisualEdge) => {
      visual.setSelectedVisualEdge(edge);
      visual.setSelectedVisualNode(null);
      const tableKey = tableKeyFromDbNodeId(edge.from) ?? tableKeyFromDbNodeId(edge.to);
      if (tableKey) {
        selectDbOnly(tableKey);
      } else {
        db.setSelectedDbTableKey(null);
      }
    },
    clearSelection: visual.clearVisualSelection,
  };
}

function tableKeyFromDbNodeId(nodeId: string): string | null {
  if (nodeId.startsWith("db:table:")) {
    return nodeId.slice("db:table:".length);
  }
  return tableKeyFromColumnNodeId(nodeId);
}

function tableKeyFromColumnNodeId(nodeId: string): string | null {
  const body = nodeId.startsWith("db:column:") ? nodeId.slice("db:column:".length) : "";
  const splitIndex = body.lastIndexOf(":");
  return splitIndex > 0 ? body.slice(0, splitIndex) : null;
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
