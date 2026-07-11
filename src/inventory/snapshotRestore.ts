import {
  dbInventoryTableKey,
  type CodeInventory,
  type CodeInventoryItem,
  type DbConstraint,
  type DbForeignKey,
  type DbIndex,
  type DbInventory,
  type DbInventoryTable,
} from "../types/workspace";
import type { InventoryItem, InventorySnapshot, SnapshotLink } from "../types/visual-map";

export function codeInventoryFromSnapshot(snapshot: InventorySnapshot, project: string): CodeInventory {
  const codeItems = snapshot.items.filter((item) => item.source === "code");
  const routes = codeItems.filter((item) => item.layer === "api").map((item) => codeItemFromSnapshot(item, project));
  const codeSymbols = codeItems
    .filter((item) => item.layer === "code" && item.kind !== "file")
    .map((item) => codeItemFromSnapshot(item, project));
  const files = codeItems.filter((item) => item.kind === "file").map((item) => codeItemFromSnapshot(item, project));
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
    summary: {
      routes: routes.length,
      handlers: handlers.length,
      services: services.length,
      repositories: repositories.length,
      functions: functions.length,
      classes: classes.length,
      modules: modules.length,
      files: files.length,
      unknown: unknown.length,
    },
    architecture: snapshot.metadata?.architecture ?? null,
    calls: (snapshot.links ?? [])
      .filter((link) => link.kind === "code_call")
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
  };
}

export function dbInventoryFromSnapshot(snapshot: InventorySnapshot, profileId: string): DbInventory {
  const tables = snapshot.items
    .filter((item) => item.source === "db" && item.kind === "table")
    .map((item): DbInventoryTable => {
      const tableKey = item.id.replace(/^db:table:/, "");
      const constraints = dbConstraintsForTable(snapshot, item);
      const stableTableKey = isDbObjectKey(item.qualifiedName) ? item.qualifiedName : null;
      return {
        key: stableTableKey,
        database: dbObjectKeyParts(stableTableKey)?.database ?? null,
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
      };
    });

  const dbGaps = snapshot.metadata?.gaps?.filter((gap) => gap.kind.startsWith("db-")) ?? [];
  return {
    profileId,
    tables,
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

export function firstTableKey(inventory: DbInventory): string | null {
  return inventory.tables[0] ? dbInventoryTableKey(inventory.tables[0]) : null;
}

function codeItemFromSnapshot(item: InventoryItem, project: string): CodeInventoryItem {
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
  return dbObjectKeyParts(value)?.kind === "table";
}

function isDbColumnKey(value: string | null | undefined): value is string {
  return dbObjectKeyParts(value)?.kind === "column";
}

function dbObjectKeyParts(value: string | null | undefined): { database: string; kind: string } | null {
  const parts = value?.split(":") ?? [];
  if ((parts.length !== 6 && parts.length !== 7) || parts.some((part) => !part)) {
    return null;
  }
  return { database: parts[2], kind: parts[4] };
}

function optionalArray(value: string | null): string[] {
  return value ? [value] : [];
}

function dbColumnRef(id: string): { tableKey: string; column: string } | null {
  if (!id.startsWith("db:column:")) {
    return null;
  }
  const body = id.slice("db:column:".length);
  const splitIndex = body.lastIndexOf(":");
  if (splitIndex <= 0 || splitIndex === body.length - 1) {
    return null;
  }
  return {
    tableKey: body.slice(0, splitIndex),
    column: body.slice(splitIndex + 1),
  };
}

function tableNameFromKey(tableKey: string): string {
  return tableKey.split(".").pop() ?? tableKey;
}
