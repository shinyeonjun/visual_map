import type { AnalysisCoverage, ChangeIntent, VisualEdge, VisualMap, VisualNode } from "./visual-map";
import type { OperationStatus } from "./operation";
import type {
  CodeInventory,
  CodeInventoryItem,
  DbInventory,
  DbProfile,
  DbProfileSource,
  RepoSourceMode,
  Workspace,
  WorkspaceRecoveryWarning,
} from "./workspace";

export type WorkspaceControls = {
  initialized: boolean;
  operationStatus: OperationStatus;
  workspaces: Workspace[];
  recoveryWarnings: WorkspaceRecoveryWarning[];
  currentWorkspace: Workspace | null;
  repoSourceMode: RepoSourceMode;
  workspaceName: string;
  repoPath: string;
  repoPathError: string | null;
  status: string | null;
  error: string | null;
  codeStatus: string | null;
  codeError: string | null;
  codeErrorDetail: string | null;
  codeInventory: CodeInventory | null;
  selectedCodeItem: CodeInventoryItem | null;
  busy: boolean;
  creating: boolean;
  opening: boolean;
  refreshing: boolean;
  deleting: boolean;
  codeIndexing: boolean;
  codeLoading: boolean;
  canIndexCode: boolean;
  codeIndexBlockedReason: string | null;
  canCreateWorkspace: boolean;
  setRepoSourceMode: (value: RepoSourceMode) => void;
  setWorkspaceName: (value: string) => void;
  setRepoPath: (value: string) => void;
  pickRepoPath: () => void;
  createWorkspace: () => void;
  openWorkspace: (workspaceId: string) => void;
  refreshGithubWorkspace: () => void;
  indexCodeRepository: () => void;
  loadCodeInventory: () => void;
  openCodeItem: (item: CodeInventoryItem) => void;
  refreshWorkspaces: () => void;
  repairWorkspaceFromBackup: (workspaceId: string) => void;
  deleteWorkspace: (workspaceId: string) => void;
};

export type DbProfileControls = {
  hasWorkspace: boolean;
  activeProfile: DbProfile | null;
  inventory: DbInventory | null;
  selectedTableKey: string | null;
  profileName: string;
  profileSource: DbProfileSource;
  profilePath: string;
  connectionString: string;
  status: string | null;
  error: string | null;
  errorDetail: string | null;
  busy: boolean;
  saving: boolean;
  testing: boolean;
  indexing: boolean;
  loading: boolean;
  deleting: boolean;
  canSaveProfile: boolean;
  canTestConnection: boolean;
  canIndexProfile: boolean;
  dbIndexBlockedReason: string | null;
  canLoadInventory: boolean;
  setProfileName: (value: string) => void;
  setProfileSource: (value: DbProfileSource) => void;
  setProfilePath: (value: string) => void;
  setConnectionString: (value: string) => void;
  pickPath: () => void;
  saveProfile: () => void;
  testConnection: () => void;
  indexProfile: () => void;
  loadInventory: () => void;
  deleteProfile: () => void;
  openTable: (tableKey: string) => void;
  openColumn: (tableKey: string, columnName: string) => void;
};

export type VisualMapControls = {
  currentMap: VisualMap | null;
  mode: string;
  loading: boolean;
  enriching: boolean;
  changeIntent: ChangeIntent;
  snapshotSavedAt: string | null;
  snapshotStaleReasons: string[];
  snapshotSourceSummary: string | null;
  analysisCoverage: AnalysisCoverage | null;
  projectionElapsedMs: number | null;
  searchQuery: string;
  searchPopoverOpen: boolean;
  searchSummary: string | null;
  searchGroups: SearchResultGroup[];
  selectedNode: VisualNode | null;
  selectedEdge: VisualEdge | null;
  setSearchQuery: (value: string) => void;
  showMode: (mode: string, focusId?: string | null) => void;
  setChangeIntent: (intent: ChangeIntent) => void;
  runSearch: () => void;
  selectSearchResult: (result: SearchResult) => void;
  openSearchPopover: () => void;
  closeSearchPopover: () => void;
  selectNode: (node: VisualNode) => void;
  selectEdge: (edge: VisualEdge) => void;
  clearSelection: () => void;
};

export type SearchResultGroup = {
  title: string;
  results: SearchResult[];
};

export type SearchResult = {
  id: string;
  title: string;
  subtitle: string | null;
  focusId: string;
  tableKey?: string;
  codeItem?: CodeInventoryItem;
};

export function dbProfileWorkStarted(dbProfileControls: DbProfileControls): boolean {
  return Boolean(
    dbProfileControls.inventory ||
      dbProfileControls.activeProfile ||
      dbProfileControls.canSaveProfile ||
      dbProfileControls.profileName.trim() ||
      dbProfileControls.profilePath.trim() ||
      dbProfileControls.connectionString.trim(),
  );
}
