import { dbTableIdentityKey } from "../inventory/dbIdentity";

export type DbProfileSource =
  "sqlite" | "ddl-sqlite" | "postgres" | "yugabytedb" | "mysql" | "mariadb" | "sqlserver" | "oracle";

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

export type InitializeWorkspaceAnalysisRequest = {
  workspaceId: string;
  analysisMode?: "code-only" | "db-only" | "code-and-db";
  dbProfileId?: string | null;
  connectionString?: string | null;
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

export function isProjectCodeItem(item: CodeInventoryItem): boolean {
  return !item.filePath?.trim().startsWith("<");
}

type CodeRouteSurface = "backend-api" | "ui-navigation" | "unknown";

function codeRouteSurface(route: CodeInventoryItem): CodeRouteSurface {
  if (route.kind.trim().toLowerCase() === "ui-route") return "ui-navigation";
  const detail = route.detail;
  if (detail && typeof detail === "object" && !Array.isArray(detail)) {
    const value = (detail as Record<string, unknown>).routeSurface ?? (detail as Record<string, unknown>).route_surface;
    if (value === "backend-api" || value === "ui-navigation") return value;
  }
  if (looksLikeFrontendNavigationRoute(route)) {
    return "ui-navigation";
  }
  // Older snapshots predate the surface contract; their Route inventory was API-only.
  return "backend-api";
}

function looksLikeFrontendNavigationRoute(route: CodeInventoryItem): boolean {
  const path = route.filePath?.replace(/\\/g, "/").toLowerCase();
  if (!path || !/\.(tsx|jsx|vue|svelte)$/.test(path)) {
    return false;
  }
  return /(^|\/)(frontend|client|web|ui)(\/|$)/.test(path) || /\/(pages|screens|views)\//.test(path);
}

export function isUiRoute(route: CodeInventoryItem): boolean {
  return codeRouteSurface(route) === "ui-navigation";
}

function isBackendApiRoute(route: CodeInventoryItem): boolean {
  return !isUiRoute(route);
}

export function codeInventoryBackendRoutes(inventory: CodeInventory | null | undefined): CodeInventoryItem[] {
  return (inventory?.routes ?? []).filter(isBackendApiRoute);
}

export function codeInventoryUiRoutes(inventory: CodeInventory | null | undefined): CodeInventoryItem[] {
  return (inventory?.routes ?? []).filter(isUiRoute);
}

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
  evidence?: CodeEvidenceSummary | null;
  calls: CodeCall[];
  clientRequests?: ClientRequest[];
  handles?: CodeHandle[];
  relationGaps?: CodeInventoryGap[];
  partial?: boolean;
};

export type CodeEvidenceSummary = {
  schema: "code-memory.evidence-summary.v1" | string;
  sourceSchema?: string | null;
  projectRoot?: string | null;
  collectors: CodeEvidenceCollector[];
  factCount: number;
  relationCount: number;
  diagnosticCount: number;
  diagnostics: CodeEvidenceDiagnostic[];
  diagnosticsHidden: number;
};

export type CodeEvidenceCollector = {
  id: string;
  capability: string;
  mode: "passive" | "tool-assisted" | string;
  status: "collected" | "partial" | "not-detected" | "unavailable" | "failed" | string;
  detectedBy: string[];
  detectedByTotal: number;
  tool?: string | null;
  toolOrigin?: string | null;
  toolVersion?: string | null;
  factCount: number;
  relationCount: number;
  diagnosticCount: number;
};

export type CodeEvidenceDiagnostic = {
  collector: string;
  level: string;
  code: string;
  message: string;
  path?: string | null;
};

export type ClientRequest = {
  id: string;
  client: string;
  method?: string | null;
  rawUrl: string;
  path?: string | null;
  sourceFile: string;
  line: number;
  endLine?: number;
  callerId?: string | null;
  resolution: "static-confirmed" | "runtime-confirmed" | "candidate" | "unknown" | "excluded" | string;
  confidence?: number | null;
  evidence: string[];
};

export type CodeAnalysisLanguage = {
  id: string;
  name: string;
  provider: string;
  filesFound: number;
  filesIndexed: number;
  filesExcluded: number;
  filesMissing: number;
  status: string;
  exclusionReason?: string;
  exclusionScope?: string;
};

type CodeAnalysisFramework = {
  id: string;
  language: string;
  name: string;
  adapter: string;
  status: string;
  factCount: number;
  relationCount: number;
};

export type CodeAnalysisQuality = {
  languages: CodeAnalysisLanguage[];
  frameworks: CodeAnalysisFramework[];
  indexedLanguages: number;
  partialLanguages: number;
  failedLanguages: number;
  detectedFrameworks: number;
  /**
   * Languages whose files exist on disk but reached none of the index.
   *
   * Derived from file counts rather than `status`, because the engine reports a
   * whole-language skip as `excluded` — an intentional-sounding word for what
   * the reader experiences as a missing part of the map.
   */
  blindSpots: CodeAnalysisLanguage[];
  /** Languages that were indexed, but not completely. */
  partialSpots: CodeAnalysisLanguage[];
  filesFound: number;
  filesIndexed: number;
};

export function codeInventoryAnalysisQuality(inventory: CodeInventory | null | undefined): CodeAnalysisQuality | null {
  const architecture = inventory?.architecture;
  if (!architecture || typeof architecture !== "object") {
    return null;
  }
  const record = architecture as Record<string, unknown>;
  const languages = readQualityArray(record.languages)
    .map((value) => ({
      id: readString(value, "id"),
      name: readString(value, "name"),
      provider: readString(value, "provider"),
      filesFound: readNumber(value, "filesFound", "files_found"),
      filesIndexed: readNumber(value, "filesIndexed", "files_indexed"),
      filesExcluded: readNumber(value, "filesExcluded", "files_excluded"),
      filesMissing: readNumber(value, "filesMissing", "files_missing"),
      status: readString(value, "status"),
      exclusionReason: readString(value, "exclusionReason", "exclusion_reason", "reason") || undefined,
      exclusionScope: readString(value, "exclusionScope", "exclusion_scope") || undefined,
    }))
    .filter((language) => language.id && language.name);
  const frameworks = readQualityArray(record.frameworks)
    .map((value) => ({
      id: readString(value, "id"),
      language: readString(value, "language"),
      name: readString(value, "name"),
      adapter: readString(value, "adapter"),
      status: readString(value, "status"),
      factCount: readNumber(value, "factCount", "fact_count"),
      relationCount: readNumber(value, "relationCount", "relation_count"),
    }))
    .filter((framework) => framework.id && framework.name);
  if (languages.length === 0 && frameworks.length === 0) {
    return null;
  }
  return {
    languages,
    frameworks,
    indexedLanguages: languages.filter((language) => language.status === "indexed").length,
    partialLanguages: languages.filter((language) => language.status === "indexed-partial").length,
    failedLanguages: languages.filter(
      (language) => !["indexed", "indexed-partial", "excluded"].includes(language.status),
    ).length,
    detectedFrameworks: frameworks.filter((framework) => framework.status === "detected").length,
    blindSpots: languages.filter((language) => language.filesFound > 0 && language.filesIndexed === 0),
    partialSpots: languages.filter(
      (language) => language.filesIndexed > 0 && language.filesIndexed < language.filesFound,
    ),
    filesFound: languages.reduce((total, language) => total + language.filesFound, 0),
    filesIndexed: languages.reduce((total, language) => total + language.filesIndexed, 0),
  };
}

function readQualityArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value)
    ? value.filter((item): item is Record<string, unknown> => Boolean(item && typeof item === "object"))
    : [];
}

function readString(value: Record<string, unknown>, ...keys: string[]): string {
  for (const key of keys) {
    if (typeof value[key] === "string") return value[key] as string;
  }
  return "";
}

function readNumber(value: Record<string, unknown>, ...keys: string[]): number {
  for (const key of keys) {
    if (typeof value[key] === "number" && Number.isFinite(value[key])) return value[key] as number;
  }
  return 0;
}

export type CodeInventoryGap = {
  id?: string;
  kind: string;
  from: string;
  to: string;
  message: string;
};

export function codeInventoryItemCount(inventory: CodeInventory | null | undefined): number {
  if (!inventory) {
    return 0;
  }
  return (
    Object.values(inventory.summary).reduce((sum, count) => sum + count, 0) + codeInventoryUiRoutes(inventory).length
  );
}

export function codeInventoryRouteCount(inventory: CodeInventory | null | undefined): number {
  return codeInventoryBackendRoutes(inventory).length;
}

export function codeInventoryFileCount(inventory: CodeInventory | null | undefined): number {
  return inventory?.summary.files ?? 0;
}

export function codeInventorySymbolCount(inventory: CodeInventory | null | undefined): number {
  if (!inventory) {
    return 0;
  }
  return Object.entries(inventory.summary)
    .filter(([key]) => key !== "routes" && key !== "files")
    .reduce((sum, [, count]) => sum + count, 0);
}

export function dbInventoryTableCount(inventory: DbInventory | null | undefined): number {
  if (!inventory) {
    return 0;
  }
  return inventory.partial || inventory.truncated
    ? inventory.tables.length
    : (inventory.totalTables ?? inventory.tables.length);
}

export function dbInventoryTotalTableCount(inventory: DbInventory | null | undefined): number {
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
  const routes = codeInventoryBackendRoutes(inventory);
  const selected = selectedId ? (routes.find((route) => route.id === selectedId) ?? null) : null;
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
  const normalized = normalizeRouteName(subject, method);
  if (!method || normalized.toUpperCase().startsWith(method.toUpperCase() + " ")) {
    return normalized;
  }
  return method.toUpperCase() + " " + normalized;
}

export function normalizeRouteName(subject: string, method: string | null | undefined): string {
  if (!method) {
    return subject;
  }
  const prefix = method.toUpperCase() + " ";
  let normalized = subject.trim();
  while (normalized.toUpperCase().startsWith(prefix + prefix)) {
    normalized = normalized.slice(prefix.length);
  }
  return normalized;
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
