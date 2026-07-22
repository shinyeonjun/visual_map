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
  const loadedDbItemCount = snapshot.items.filter((item) => item.source === "db").length;
  const tables = snapshot.items
    .filter((item) => item.source === "db" && item.kind === "table")
    .map((item): DbInventoryTable => {
      const tableKey = item.id.replace(/^db:table:/, "");
      const constraints = dbConstraintsForTable(snapshot, item);
      const stableTableKey = isDbObjectKey(item.qualifiedName) ? item.qualifiedName : null;
      return {
        key: stableTableKey,
        database: parseDbStableObjectKey(stableTableKey)?.database ?? null,
        schema: item.path ?? null,
        name: item.name,
        columns: snapshot.items
          .filter((column) => column.source === "db" && column.kind === "column" && column.parentId === item.id)
          .map((column) => ({
            key: isDbColumnKey(column.qualifiedName) ? column.qualifiedName : null,
            tableKey: stableTableKey,
            name: column.name,
            dataType: column.path ?? null,
            nullable: column.nullable ?? null,
            isPrimaryKey: Boolean(column.isPrimaryKey),
            isForeignKey: Boolean(column.isForeignKey),
          })),
        foreignKeys: foreignKeysForTable(snapshot, tableKey, "outbound"),
        inboundForeignKeys: foreignKeysForTable(snapshot, tableKey, "inbound"),
        constraints,
        indexes: dbIndexesForTable(snapshot, item),
        dependents: dbDependentsForTable(snapshot, item),
      };
    });

  const dbGaps = snapshot.metadata?.gaps?.filter((gap) => gap.kind.startsWith("db-")) ?? [];
  return {
    profileId,
    tables,
    partial: Boolean(inventorySummary?.sources.db && inventorySummary.sources.db.total > loadedDbItemCount),
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
  snapshot: InventorySnapshot,
  tableKey: string,
  direction: "outbound" | "inbound",
): DbForeignKey[] {
  const grouped = new Map<string, DbForeignKey>();
  for (const link of snapshot.links ?? []) {
    if (link.kind !== "db_fk") {
      continue;
    }
    const from = dbColumnRef(link.from);
    const to = dbColumnRef(link.to);
    if (!from || !to || (direction === "outbound" ? from.tableKey : to.tableKey) !== tableKey) {
      continue;
    }
    const sourceTable = snapshot.items.find((item) => item.id === `db:table:${from.tableKey}`);
    const referencedTable = snapshot.items.find((item) => item.id === `db:table:${to.tableKey}`);
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

function dbConstraintsForTable(snapshot: InventorySnapshot, table: InventoryItem): DbConstraint[] {
  return snapshot.items
    .filter((item) => item.source === "db" && item.kind === "constraint" && item.parentId === table.id)
    .map((item) => {
      const evidence = dbObjectEvidence(snapshot, table.id, item.id);
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

function dbIndexesForTable(snapshot: InventorySnapshot, table: InventoryItem): DbIndex[] {
  return snapshot.items
    .filter((item) => item.source === "db" && item.kind === "index" && item.parentId === table.id)
    .map((item) => {
      const evidence = dbObjectEvidence(snapshot, table.id, item.id);
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

function dbDependentsForTable(snapshot: InventorySnapshot, table: InventoryItem): DbDependentObject[] {
  const columnIds = new Set(
    snapshot.items
      .filter((item) => item.source === "db" && item.kind === "column" && item.parentId === table.id)
      .map((item) => item.id),
  );
  const itemById = new Map(snapshot.items.map((item) => [item.id, item]));
  const grouped = new Map<string, DbDependentObject>();

  for (const link of snapshot.links ?? []) {
    const trigger = link.kind === "db_trigger" && link.from === table.id;
    const dependency = link.kind === "db_dependency" && (link.to === table.id || columnIds.has(link.to));
    if (!trigger && !dependency) continue;

    const object = itemById.get(trigger ? link.to : link.from);
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

function dbObjectEvidence(snapshot: InventorySnapshot, tableId: string, objectId: string): SnapshotLink | undefined {
  return (snapshot.links ?? []).find(
    (link) => link.kind === "contains" && link.from === tableId && link.to === objectId,
  );
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
