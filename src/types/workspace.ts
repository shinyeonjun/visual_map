import { dbTableIdentityKey } from "../inventory/dbIdentity";

export type DbProfileSource =
  | "sqlite"
  | "ddl-sqlite"
  | "postgres"
  | "yugabytedb"
  | "mysql"
  | "mariadb"
  | "sqlserver"
  | "oracle";

export const DB_PROFILE_SOURCE_OPTIONS: { value: DbProfileSource; label: string }[] = [
  { value: "ddl-sqlite", label: "SQLite DDL" },
  { value: "sqlite", label: "SQLite" },
  { value: "postgres", label: "PostgreSQL" },
  { value: "yugabytedb", label: "YugabyteDB (YSQL)" },
  { value: "mysql", label: "MySQL" },
  { value: "mariadb", label: "MariaDB" },
  { value: "sqlserver", label: "SQL Server" },
  { value: "oracle", label: "Oracle" },
];

export function dbProfileSourceLabel(source: DbProfileSource): string {
  return DB_PROFILE_SOURCE_OPTIONS.find((option) => option.value === source)?.label ?? source;
}

export function dbProfileSourceUsesPath(source: DbProfileSource): boolean {
  return source === "sqlite" || source === "ddl-sqlite";
}

const CODE_KIND_CHIPS: Record<string, string> = {
  route: "ROUTE",
  api: "API",
  function: "FUNC",
  method: "FUNC",
  class: "CLASS",
  file: "FILE",
  module: "MOD",
  service: "SVC",
  repository: "REPO",
  handler: "HNDL",
  controller: "CTRL",
  unknown: "CHECK",
};

export function codeKindChip(kind: string): string {
  const key = kind.trim().toLowerCase();
  return CODE_KIND_CHIPS[key] ?? kind.slice(0, 5).toUpperCase();
}

export type Workspace = {
  id: string;
  name: string;
  repoPath: string;
  repoSource: RepoSourceMode;
  repoOrigin?: string | null;
  codeProject?: string | null;
  engineCache?: WorkspaceEngineCache | null;
  dbProfiles: DbProfile[];
  activeDbProfileId?: string | null;
  createdAt: string;
  updatedAt: string;
};

export type WorkspaceRecoveryWarning = {
  workspaceId: string;
  kind: "backup-recovered" | "unrecoverable" | string;
  message: string;
  action: "repair-from-backup" | "recreate-workspace" | string;
};

type WorkspaceEngineCache = {
  codeCachePath?: string | null;
  dbCacheDir?: string | null;
};

export type RepoSourceMode = "local" | "github";

export function workspaceRepoInputValue(workspace: Workspace): string {
  return workspace.repoOrigin ?? workspace.repoPath;
}

export type DbProfile = {
  id: string;
  name: string;
  source: DbProfileSource;
  path?: string | null;
  host?: string | null;
  port?: number | null;
  database?: string | null;
  username?: string | null;
  cachePath: string;
  lastIndexedAt?: string | null;
  passwordStored: false;
};

export type CreateWorkspaceRequest = {
  name: string;
  repoPath: string;
};

export type SaveDbProfileRequest = {
  workspaceId: string;
  name: string;
  source: DbProfileSource;
  path?: string | null;
};

export type IndexDbProfileRequest = {
  workspaceId: string;
  profileId: string;
  connectionString?: string | null;
};

export type IndexCodeRequest = {
  workspaceId: string;
};

export type DbInventoryColumn = {
  key?: string | null;
  tableKey?: string | null;
  name: string;
  dataType?: string | null;
  nullable?: boolean | null;
  isPrimaryKey: boolean;
  isForeignKey: boolean;
};

export type DbDependentObject = {
  key: string;
  kind: "view" | "trigger" | "routine" | string;
  name: string;
  relation: string;
  columnKeys?: string[];
};

export type DbInventoryTable = {
  key?: string | null;
  database?: string | null;
  schema?: string | null;
  name: string;
  columns: DbInventoryColumn[];
  foreignKeys?: DbForeignKey[];
  inboundForeignKeys?: DbForeignKey[];
  constraints?: DbConstraint[];
  indexes?: DbIndex[];
  dependents?: DbDependentObject[];
};

export type DbForeignKey = {
  key?: string | null;
  name?: string | null;
  tableKey?: string | null;
  tableSchema?: string | null;
  table?: string | null;
  columns: string[];
  columnKeys?: string[];
  referencedTableKey?: string | null;
  referencedSchema?: string | null;
  referencedTable: string;
  referencedColumns: string[];
  referencedColumnKeys?: string[];
};

export type DbConstraint = {
  key?: string | null;
  name?: string | null;
  kind: string;
  columns?: string[];
  columnKeys?: string[];
  referencedTableKey?: string | null;
  referencedSchema?: string | null;
  referencedTable?: string | null;
  referencedColumns?: string[];
  referencedColumnKeys?: string[];
  expression?: string | null;
  source?: string;
};

export type DbIndex = {
  key?: string | null;
  name: string;
  columns?: string[];
  columnKeys?: string[];
  unique?: boolean;
  primary?: boolean;
  predicate?: string | null;
  expression?: string | null;
};

type DbInventoryGap = {
  id: string;
  kind: string;
  message: string;
  tableKey?: string | null;
};

export function dbInventoryTableKey(table: DbInventoryTable): string {
  return dbTableIdentityKey(table.schema, table.name);
}

export type DbInventory = {
  profileId: string;
  tables: DbInventoryTable[];
  partial?: boolean;
  snapshotKey?: string | null;
  contractVersion?: string | null;
  capabilityWarnings?: string[];
  limitRequested?: number | null;
  limitApplied?: number | null;
  limitClamped?: boolean | null;
  resultCount?: number | null;
  totalTables?: number | null;
  truncated?: boolean | null;
  gaps?: DbInventoryGap[];
};

export type CodeInventoryItem = {
  id: string;
  kind: string;
  name: string;
  filePath?: string | null;
  line?: number | null;
  column?: number | null;
  endLine?: number | null;
  endColumn?: number | null;
  project?: string;
  qualifiedName?: string;
  engineLabel?: string;
  detail: unknown;
};

export type CodeInventory = {
  project: string;
  routes: CodeInventoryItem[];
  services: CodeInventoryItem[];
  files: CodeInventoryItem[];
  handlers: CodeInventoryItem[];
  repositories: CodeInventoryItem[];
  functions: CodeInventoryItem[];
  classes: CodeInventoryItem[];
  modules: CodeInventoryItem[];
  unknown: CodeInventoryItem[];
  summary: CodeInventorySummary;
  architecture?: unknown;
  calls: CodeCall[];
  handles?: CodeHandle[];
  partial?: boolean;
};

export function codeInventoryItemCount(inventory: CodeInventory | null | undefined): number {
  if (!inventory) {
    return 0;
  }
  return Object.values(inventory.summary).reduce((sum, count) => sum + count, 0);
}

export function codeInventoryRouteCount(inventory: CodeInventory | null | undefined): number {
  return inventory?.summary.routes ?? 0;
}

export function codeInventoryFileCount(inventory: CodeInventory | null | undefined): number {
  return inventory?.summary.files ?? 0;
}

export function codeInventorySymbolCount(inventory: CodeInventory | null | undefined): number {
  if (!inventory) {
    return 0;
  }
  const { routes, files, ...symbols } = inventory.summary;
  return Object.values(symbols).reduce((sum, count) => sum + count, 0);
}

export function dbInventoryTableCount(inventory: DbInventory | null | undefined): number {
  return inventory?.totalTables ?? inventory?.tables.length ?? 0;
}

export function codeInventoryCodeItems(inventory: CodeInventory | null | undefined): CodeInventoryItem[] {
  if (!inventory) {
    return [];
  }
  return [
    ...inventory.services,
    ...inventory.handlers,
    ...inventory.repositories,
    ...inventory.functions,
    ...inventory.classes,
    ...inventory.modules,
    ...inventory.unknown,
  ];
}

export function codeInventoryDefaultRoute(
  inventory: CodeInventory | null | undefined,
  selectedId?: string | null,
): CodeInventoryItem | null {
  const routes = inventory?.routes ?? [];
  const selected = selectedId ? routes.find((route) => route.id === selectedId) ?? null : null;
  if (selected || routes.length === 0) {
    return selected;
  }

  const callDegree = new Map<string, number>();
  for (const call of inventory?.calls ?? []) {
    callDegree.set(call.from, (callDegree.get(call.from) ?? 0) + 1);
  }
  const routeScore = new Map<string, number>();
  for (const handle of inventory?.handles ?? []) {
    routeScore.set(handle.route, (routeScore.get(handle.route) ?? 0) + 100 + (callDegree.get(handle.handler) ?? 0));
  }

  let best = routes[0];
  for (const route of routes.slice(1)) {
    const score = (routeScore.get(route.id) ?? 0) + routeSpecificity(route);
    const bestScore = (routeScore.get(best.id) ?? 0) + routeSpecificity(best);
    if (score > bestScore || (score === bestScore && route.id < best.id)) {
      best = route;
    }
  }
  return best;
}

export function codeRouteMethod(route: CodeInventoryItem): string | null {
  const identity = `${route.qualifiedName ?? ""} ${route.id}`;
  return routeMethodFromIdentity(identity);
}

export function routeMethodFromIdentity(identity: string | null | undefined): string | null {
  return identity?.match(/__route__([A-Z]+)__/i)?.[1]?.toUpperCase() ?? null;
}

export function routeDisplayName(subject: string, method: string | null | undefined): string {
  if (!method || subject.toUpperCase().startsWith(`${method.toUpperCase()} `)) {
    return subject;
  }
  return `${method.toUpperCase()} ${subject}`;
}

function routeSpecificity(route: CodeInventoryItem): number {
  const segments = route.name.split(/[/?]/).filter(Boolean);
  const staticSegments = segments.filter((segment) => !segment.startsWith(":") && !segment.startsWith("{")).length;
  if (staticSegments === 0) return -1_000;
  return staticSegments * 120 + Math.min(route.name.length, 80) - (route.id.includes("__route__ANY__") ? 500 : 0);
}

type CodeCall = {
  from: string;
  to: string;
};

/** Raw engine HANDLES direction: handler -> route. Product projections reverse it to Route -> Handler. */
type CodeHandle = {
  handler: string;
  route: string;
};

type CodeInventorySummary = {
  routes: number;
  handlers: number;
  services: number;
  repositories: number;
  functions: number;
  classes: number;
  modules: number;
  files: number;
  unknown: number;
};
