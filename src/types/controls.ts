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
  restoringSnapshot: boolean;
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
  indexing: boolean;
  deleting: boolean;
  canSaveProfile: boolean;
  canIndexProfile: boolean;
  dbIndexBlockedReason: string | null;
  setProfileName: (value: string) => void;
  setProfileSource: (value: DbProfileSource) => void;
  setProfilePath: (value: string) => void;
  setConnectionString: (value: string) => void;
  pickPath: (directory?: boolean) => void;
  saveProfile: () => void;
  indexProfile: () => void;
  deleteProfile: () => void;
  openTable: (tableKey: string) => void;
  openColumn: (tableKey: string, columnName: string) => void;
};

export type VisualMapControls = {
  currentMap: VisualMap | null;
  mode: string;
  focusId: string | null;
  compositionFocusIds: string[];
  relationView: "connections" | "calls" | "data" | "impact";
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
  toggleCompositionFocus: (focusId: string) => void;
  clearCompositionFocus: () => void;
  setRelationView: (view: "connections" | "calls" | "data" | "impact") => void;
  setChangeIntent: (intent: ChangeIntent) => void;
  runSearch: (value?: string) => void;
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
