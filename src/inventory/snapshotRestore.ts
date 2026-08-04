import {
  type CodeInventory,
  type CodeInventoryItem,
  type DbConstraint,
  type DbDependentObject,
  type DbForeignKey,
  type DbIndex,
  type DbInventory,
  type DbInventoryTable,
} from "../types/workspace";
import type { InventoryItem, InventorySnapshot, InventorySummary, SnapshotLink } from "../types/visual-map";
import { columnRefFromNodeId } from "../visual/nodeIds";
import { dbTableNameFromIdentityKey, parseDbStableObjectKey } from "./dbIdentity";

export function codeInventoryFromSnapshot(
  snapshot: InventorySnapshot,
  project: string,
  inventorySummary?: InventorySummary,
): CodeInventory {
  const codeItems = snapshot.items.filter((item) => item.source === "code");
  const routes = codeItems
    .filter((item) => item.layer === "api")
    .map((item) => codeInventoryItemFromSnapshot(item, project));
  const confirmedRouteIds = new Set(
    (snapshot.links ?? [])
      .filter((link) => link.kind === "code_handle")
      .map((link) => link.from.replace(/^code:/, "")),
  );
  routes.sort((left, right) =>
    Number(!confirmedRouteIds.has(left.id)) - Number(!confirmedRouteIds.has(right.id)) ||
    left.id.localeCompare(right.id),
  );
  const codeSymbols = codeItems
    .filter((item) => item.layer === "code" && item.kind !== "file")
    .map((item) => codeInventoryItemFromSnapshot(item, project));
  const files = codeItems
    .filter((item) => item.kind === "file")
    .map((item) => codeInventoryItemFromSnapshot(item, project));
  const confirmedHandlerIds = new Set(
    (snapshot.links ?? [])
      .filter((link) => link.kind === "code_handle")
      .map((link) => link.to.replace(/^code:/, "")),
  );
  const category = (item: CodeInventoryItem) => (confirmedHandlerIds.has(item.id) ? "handler" : codeCategory(item));
  const handlers = codeSymbols.filter((item) => category(item) === "handler");
  const services = codeSymbols.filter((item) => category(item) === "service");
  const repositories = codeSymbols.filter((item) => category(item) === "repository");
  const functions = codeSymbols.filter((item) => category(item) === "function");
  const classes = codeSymbols.filter((item) => category(item) === "class");
  const modules = codeSymbols.filter((item) => category(item) === "module");
  const unknown = codeSymbols.filter((item) => category(item) === "code");

  return {
    project,
    routes,
    services,
    files,
    handlers,
    repositories,
    functions,
    classes,
    modules,
    unknown,
    summary: codeSummary(inventorySummary, {
      routes: routes.length,
      handlers: handlers.length,
      services: services.length,
      repositories: repositories.length,
      functions: functions.length,
      classes: classes.length,
      modules: modules.length,
      files: files.length,
      unknown: unknown.length,
    }),
    architecture: snapshot.metadata?.architecture ?? null,
    evidence: snapshot.metadata?.evidence ?? null,
    calls: (snapshot.links ?? [])
      .filter((link) => link.kind === "code_call" && link.truthClass === "confirmed")
      .map((link) => ({
        from: link.from.replace(/^code:/, ""),
        to: link.to.replace(/^code:/, ""),
      })),
    handles: (snapshot.links ?? [])
      .filter((link) => link.kind === "code_handle")
      .map((link) => ({
        route: link.from.replace(/^code:/, ""),
        handler: link.to.replace(/^code:/, ""),
      })),
    partial: Boolean(inventorySummary?.sources.code && inventorySummary.sources.code.total > codeItems.length),
  };
}

export function dbInventoryFromSnapshot(
  snapshot: InventorySnapshot,
  profileId: string,
  inventorySummary?: InventorySummary,
): DbInventory {
  const index = buildDbRestoreIndex(snapshot);
  const tables = index.tables
    .map((item): DbInventoryTable => {
      const tableKey = item.id.replace(/^db:table:/, "");
      const constraints = dbConstraintsForTable(index, item);
      const stableTableKey = isDbObjectKey(item.qualifiedName) ? item.qualifiedName : null;
      return {
        key: stableTableKey,
        database: parseDbStableObjectKey(stableTableKey)?.database ?? null,
        schema: item.path ?? null,
        name: item.name,
        columns: (index.columnsByParentId.get(item.id) ?? [])
          .map((column) => ({
            key: isDbColumnKey(column.qualifiedName) ? column.qualifiedName : null,
            tableKey: stableTableKey,
            name: column.name,
            dataType: column.path ?? null,
            nullable: column.nullable ?? null,
            isPrimaryKey: Boolean(column.isPrimaryKey),
            isForeignKey: Boolean(column.isForeignKey),
          })),
        foreignKeys: foreignKeysForTable(index, tableKey, "outbound"),
        inboundForeignKeys: foreignKeysForTable(index, tableKey, "inbound"),
        constraints,
        indexes: dbIndexesForTable(index, item),
        dependents: dbDependentsForTable(index, item),
      };
    });

  const dbGaps = snapshot.metadata?.gaps?.filter((gap) => gap.kind.startsWith("db-")) ?? [];
  return {
    profileId,
    tables,
    partial: Boolean(inventorySummary?.sources.db && inventorySummary.sources.db.total > index.loadedDbItemCount),
    snapshotKey: snapshot.metadata?.db?.snapshotKey ?? null,
    contractVersion: snapshot.metadata?.db?.contractVersion ?? null,
    limitRequested: snapshot.metadata?.db?.limitRequested ?? null,
    limitApplied: snapshot.metadata?.db?.limitApplied ?? null,
    limitClamped: snapshot.metadata?.db?.limitClamped ?? null,
    resultCount: snapshot.metadata?.db?.resultCount ?? null,
    totalTables: snapshot.metadata?.db?.totalTables ?? null,
    truncated: snapshot.metadata?.db?.truncated ?? null,
    capabilityWarnings: dbGaps.filter((gap) => gap.kind === "db-capability").map((gap) => gap.message),
    gaps: dbGaps
      .filter((gap) => gap.kind !== "db-capability")
      .map((gap) => ({
        id: gap.id.replace(/^gap:/, ""),
        kind: gap.kind,
        message: gap.message,
        tableKey: gap.relatedIds?.find((id) => id.startsWith("db:table:"))?.replace(/^db:table:/, "") ?? null,
      })),
  };
}

type DbRestoreIndex = {
  loadedDbItemCount: number;
  tables: InventoryItem[];
  itemsById: Map<string, InventoryItem>;
  columnsByParentId: Map<string, InventoryItem[]>;
  constraintsByParentId: Map<string, InventoryItem[]>;
  indexesByParentId: Map<string, InventoryItem[]>;
  containsByPair: Map<string, SnapshotLink>;
  foreignKeysByTableDirection: Map<string, SnapshotLink[]>;
  linksByKind: Map<string, SnapshotLink[]>;
};

function buildDbRestoreIndex(snapshot: InventorySnapshot): DbRestoreIndex {
  const itemsById = new Map<string, InventoryItem>();
  const tables: InventoryItem[] = [];
  const columnsByParentId = new Map<string, InventoryItem[]>();
  const constraintsByParentId = new Map<string, InventoryItem[]>();
  const indexesByParentId = new Map<string, InventoryItem[]>();
  let loadedDbItemCount = 0;

  for (const item of snapshot.items) {
    itemsById.set(item.id, item);
    if (item.source !== "db") continue;
    loadedDbItemCount += 1;
    if (item.kind === "table") tables.push(item);
    if (item.kind === "column" && item.parentId) addToIndex(columnsByParentId, item.parentId, item);
    if (item.kind === "constraint" && item.parentId) addToIndex(constraintsByParentId, item.parentId, item);
    if (item.kind === "index" && item.parentId) addToIndex(indexesByParentId, item.parentId, item);
  }

  const containsByPair = new Map<string, SnapshotLink>();
  const foreignKeysByTableDirection = new Map<string, SnapshotLink[]>();
  const linksByKind = new Map<string, SnapshotLink[]>();
  for (const link of snapshot.links ?? []) {
    addToIndex(linksByKind, link.kind, link);
    if (link.kind === "contains") {
      containsByPair.set(`${link.from}\0${link.to}`, link);
      continue;
    }
    if (link.kind !== "db_fk") continue;
    const from = dbColumnRef(link.from);
    const to = dbColumnRef(link.to);
    if (!from || !to) continue;
    addToIndex(foreignKeysByTableDirection, `outbound\0${from.tableKey}`, link);
    addToIndex(foreignKeysByTableDirection, `inbound\0${to.tableKey}`, link);
  }

  return {
    loadedDbItemCount,
    tables,
    itemsById,
    columnsByParentId,
    constraintsByParentId,
    indexesByParentId,
    containsByPair,
    foreignKeysByTableDirection,
    linksByKind,
  };
}

function addToIndex<T>(index: Map<string, T[]>, key: string, value: T): void {
  const values = index.get(key);
  if (values) {
    values.push(value);
  } else {
    index.set(key, [value]);
  }
}

export function codeInventoryItemFromSnapshot(item: InventoryItem, project: string): CodeInventoryItem {
  const id = item.id.replace(/^code:/, "");
  return {
    id,
    kind: item.kind,
    name: item.name,
    filePath: item.location?.path ?? item.path ?? null,
    line: item.location?.line ?? null,
    column: item.location?.column ?? null,
    endLine: item.location?.endLine ?? null,
    endColumn: item.location?.endColumn ?? null,
    project: item.projectId ?? project,
    qualifiedName: item.qualifiedName ?? id,
    engineLabel: item.engineLabel ?? item.kind,
    detail: item,
  };
}

function codeSummary(
  summary: InventorySummary | undefined,
  fallback: CodeInventory["summary"],
): CodeInventory["summary"] {
  const groups = summary?.sources.code?.groups;
  if (!groups) {
    return fallback;
  }
  return {
    routes: groups.routes ?? 0,
    handlers: groups.handlers ?? 0,
    services: groups.services ?? 0,
    repositories: groups.repositories ?? 0,
    functions: groups.functions ?? 0,
    classes: groups.classes ?? 0,
    modules: groups.modules ?? 0,
    files: groups.files ?? 0,
    unknown: groups.unknown ?? 0,
  };
}

function codeCategory(item: CodeInventoryItem): string {
  const text = `${item.kind} ${item.name}`.toLowerCase();
  if (text.includes("handler") || text.includes("controller")) {
    return "handler";
  }
  if (text.includes("repository") || text.includes("repo") || text.includes("dao")) {
    return "repository";
  }
  if (text.includes("service")) {
    return "service";
  }
  if (text.includes("function") || text.includes("method")) {
    return "function";
  }
  if (text.includes("class")) {
    return "class";
  }
  if (text.includes("module") || text.includes("package")) {
    return "module";
  }
  return "code";
}

function foreignKeysForTable(
  index: DbRestoreIndex,
  tableKey: string,
  direction: "outbound" | "inbound",
): DbForeignKey[] {
  const grouped = new Map<string, DbForeignKey>();
  for (const link of index.foreignKeysByTableDirection.get(`${direction}\0${tableKey}`) ?? []) {
    const from = dbColumnRef(link.from);
    const to = dbColumnRef(link.to);
    if (!from || !to) continue;
    const sourceTable = index.itemsById.get(`db:table:${from.tableKey}`);
    const referencedTable = index.itemsById.get(`db:table:${to.tableKey}`);
    const key = linkEvidence(link, "db-object-key");
    const groupKey = `${key ?? link.label ?? ""}\0${from.tableKey}\0${to.tableKey}`;
    const existing = grouped.get(groupKey);
    if (existing) {
      existing.columns.push(from.column);
      existing.referencedColumns.push(to.column);
      const sourceColumnKey = linkEvidence(link, "db-column-key");
      const referencedColumnKey = linkEvidence(link, "db-referenced-column-key");
      if (sourceColumnKey) existing.columnKeys?.push(sourceColumnKey);
      if (referencedColumnKey) existing.referencedColumnKeys?.push(referencedColumnKey);
      continue;
    }
    grouped.set(groupKey, {
      key,
      name: link.label ?? null,
      tableKey: isDbObjectKey(sourceTable?.qualifiedName) ? sourceTable?.qualifiedName : null,
      tableSchema: sourceTable?.path ?? null,
      table: sourceTable?.name ?? tableNameFromKey(from.tableKey),
      columns: [from.column],
      columnKeys: optionalArray(linkEvidence(link, "db-column-key")),
      referencedTableKey: isDbObjectKey(referencedTable?.qualifiedName) ? referencedTable?.qualifiedName : null,
      referencedSchema: referencedTable?.path ?? null,
      referencedTable: referencedTable?.name ?? tableNameFromKey(to.tableKey),
      referencedColumns: [to.column],
      referencedColumnKeys: optionalArray(linkEvidence(link, "db-referenced-column-key")),
    });
  }
  return [...grouped.values()];
}

function dbConstraintsForTable(index: DbRestoreIndex, table: InventoryItem): DbConstraint[] {
  return (index.constraintsByParentId.get(table.id) ?? [])
    .map((item) => {
      const evidence = dbObjectEvidence(index, table.id, item.id);
      return {
        key: linkEvidence(evidence, "db-object-key") ?? (isDbObjectKey(item.qualifiedName) ? item.qualifiedName : null),
        name: linkEvidence(evidence, "db-object-name"),
        kind: linkEvidence(evidence, "db-constraint-kind") ?? item.engineLabel?.replace(/^Constraint:/, "") ?? "unknown",
        columns: linkEvidenceArray(evidence, "db-columns"),
        columnKeys: linkEvidenceArray(evidence, "db-column-keys"),
        referencedTableKey: linkEvidence(evidence, "db-referenced-table-key"),
        referencedSchema: linkEvidence(evidence, "db-referenced-schema"),
        referencedTable: linkEvidence(evidence, "db-referenced-table"),
        referencedColumns: linkEvidenceArray(evidence, "db-referenced-columns"),
        referencedColumnKeys: linkEvidenceArray(evidence, "db-referenced-column-keys"),
        expression: linkEvidence(evidence, "db-expression") ?? item.path ?? null,
        source: linkEvidence(evidence, "db-contract-field") ?? "snapshot",
      };
    });
}

function dbIndexesForTable(index: DbRestoreIndex, table: InventoryItem): DbIndex[] {
  return (index.indexesByParentId.get(table.id) ?? [])
    .map((item) => {
      const evidence = dbObjectEvidence(index, table.id, item.id);
      return {
        key: linkEvidence(evidence, "db-object-key") ?? (isDbObjectKey(item.qualifiedName) ? item.qualifiedName : null),
        name: linkEvidence(evidence, "db-object-name") ?? item.name,
        columns: linkEvidenceArray(evidence, "db-columns"),
        columnKeys: linkEvidenceArray(evidence, "db-column-keys"),
        unique: linkEvidence(evidence, "db-index-unique") === "true",
        primary: linkEvidence(evidence, "db-index-primary") === "true",
        predicate: linkEvidence(evidence, "db-index-predicate"),
        expression: linkEvidence(evidence, "db-index-expression"),
      };
    });
}

function dbDependentsForTable(index: DbRestoreIndex, table: InventoryItem): DbDependentObject[] {
  const columnIds = new Set((index.columnsByParentId.get(table.id) ?? []).map((item) => item.id));
  const grouped = new Map<string, DbDependentObject>();

  const links = [
    ...(index.linksByKind.get("db_trigger") ?? []),
    ...(index.linksByKind.get("db_dependency") ?? []),
  ];
  for (const link of links) {
    const trigger = link.kind === "db_trigger" && link.from === table.id;
    const dependency = link.kind === "db_dependency" && (link.to === table.id || columnIds.has(link.to));
    if (!trigger && !dependency) continue;

    const object = index.itemsById.get(trigger ? link.to : link.from);
    if (!object || !isDbDependentKey(object.qualifiedName, object.kind)) continue;

    const existing = grouped.get(object.qualifiedName) ?? {
      key: object.qualifiedName,
      kind: object.kind,
      name: object.name,
      relation: linkEvidence(link, "db-relation") ?? dependentRelation(object.kind),
      columnKeys: [],
    };
    const columnKeys = new Set(existing.columnKeys ?? []);
    for (const key of linkEvidenceArray(link, "db-column-keys")) columnKeys.add(key);
    const endpointKey = linkEvidence(link, "db-column-key");
    if (endpointKey) columnKeys.add(endpointKey);
    existing.columnKeys = [...columnKeys].sort();
    grouped.set(object.qualifiedName, existing);
  }

  return [...grouped.values()].sort(
    (left, right) => left.key.localeCompare(right.key) || left.relation.localeCompare(right.relation),
  );
}

function dependentRelation(kind: string): string {
  if (kind === "trigger") return "table_has_trigger";
  if (kind === "view") return "view_depends_on";
  return "routine_depends_on";
}

function dbObjectEvidence(index: DbRestoreIndex, tableId: string, objectId: string): SnapshotLink | undefined {
  return index.containsByPair.get(`${tableId}\0${objectId}`);
}

function linkEvidence(link: SnapshotLink | undefined, kind: string): string | null {
  return link?.evidence?.find((evidence) => evidence.kind === kind)?.text ?? null;
}

function linkEvidenceArray(link: SnapshotLink | undefined, kind: string): string[] {
  const value = linkEvidence(link, kind);
  if (!value) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function isDbObjectKey(value: string | null | undefined): value is string {
  return parseDbStableObjectKey(value)?.kind === "table";
}

function isDbColumnKey(value: string | null | undefined): value is string {
  return parseDbStableObjectKey(value)?.kind === "column";
}

function isDbDependentKey(value: string | null | undefined, kind: string): value is string {
  return matchesDbDependentKind(parseDbStableObjectKey(value)?.kind, kind);
}

function matchesDbDependentKind(stableKind: string | undefined, itemKind: string): boolean {
  return stableKind === itemKind && ["view", "trigger", "routine"].includes(itemKind);
}

function optionalArray(value: string | null): string[] {
  return value ? [value] : [];
}

function dbColumnRef(id: string): { tableKey: string; column: string } | null {
  const ref = columnRefFromNodeId(id);
  return ref ? { tableKey: ref.tableKey, column: ref.columnName } : null;
}

function tableNameFromKey(tableKey: string): string {
  return dbTableNameFromIdentityKey(tableKey);
}
