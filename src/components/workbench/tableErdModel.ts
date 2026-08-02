import type { VisualMap } from "../../types/visual-map";
import type { DbForeignKey, DbInventoryColumn, DbInventoryTable } from "../../types/workspace";
import { dbInventoryTableKey } from "../../types/workspace";
import { dbTableIdentityKey } from "../../inventory/dbIdentity";
import { dbTableIdentityLabel, dbTableNodeId, tableKeyFromDbNodeId } from "../../visual/nodeIds";

const NEIGHBOR_LIMIT = 4;
const COLUMN_LIMIT = 6;

export type ErdNeighbor = {
  id: string;
  direction: "inbound" | "outbound";
  label: string;
  tableKey: string | null;
  nodeId: string | null;
  viaLabel: string;
};

export type TableErdModel = {
  tableLabel: string;
  columns: DbInventoryColumn[];
  hiddenColumns: number;
  inbound: ErdNeighbor[];
  outbound: ErdNeighbor[];
  hiddenInbound: number;
  hiddenOutbound: number;
  reads: number;
  writes: number;
  candidateUses: number;
};

/**
 * Mini ERD around the focused table: FK neighbors only, plus how code touches
 * the table (READS/WRITES counts from confirmed static-SQL edges in the map).
 * Neighbors come straight from DB metadata; nothing is inferred.
 */
export function buildTableErdModel(map: VisualMap, tables: DbInventoryTable[]): TableErdModel | null {
  const tableKey = tableKeyFromDbNodeId(map.focus);
  if (!tableKey) return null;
  const table = tables.find((candidate) => dbInventoryTableKey(candidate) === tableKey);
  if (!table) return null;

  const columns = [...table.columns].sort(compareColumns);
  const outboundAll = (table.foreignKeys ?? []).map((fk, index) => outboundNeighbor(fk, index));
  const inboundAll = (table.inboundForeignKeys ?? []).map((fk, index) => inboundNeighbor(fk, index));

  const reads = map.edges.filter((edge) => edge.to === map.focus && edge.kind === "code_db_read").length;
  const writes = map.edges.filter((edge) => edge.to === map.focus && edge.kind === "code_db_write").length;
  const candidateUses = map.edges.filter(
    (edge) => edge.to === map.focus && (edge.kind.startsWith("candidate") || edge.confidence === "candidate"),
  ).length;

  return {
    tableLabel: dbTableIdentityLabel(tableKey),
    columns: columns.slice(0, COLUMN_LIMIT),
    hiddenColumns: Math.max(0, columns.length - COLUMN_LIMIT),
    inbound: inboundAll.slice(0, NEIGHBOR_LIMIT),
    outbound: outboundAll.slice(0, NEIGHBOR_LIMIT),
    hiddenInbound: Math.max(0, inboundAll.length - NEIGHBOR_LIMIT),
    hiddenOutbound: Math.max(0, outboundAll.length - NEIGHBOR_LIMIT),
    reads,
    writes,
    candidateUses,
  };
}

function outboundNeighbor(fk: DbForeignKey, index: number): ErdNeighbor {
  const tableKey = fk.referencedTableKey
    ?? (fk.referencedTable ? dbTableIdentityKey(fk.referencedSchema, fk.referencedTable) : null);
  return {
    id: fk.key ?? fk.name ?? `outbound-${index}`,
    direction: "outbound",
    label: tableKey ? dbTableIdentityLabel(tableKey) : fk.referencedTable,
    tableKey,
    nodeId: tableKey ? dbTableNodeId(tableKey) : null,
    viaLabel: `${fk.columns.join(", ")} → ${fk.referencedColumns.join(", ")}`,
  };
}

function inboundNeighbor(fk: DbForeignKey, index: number): ErdNeighbor {
  const tableKey = fk.tableKey
    ?? (fk.table ? dbTableIdentityKey(fk.tableSchema, fk.table) : null);
  return {
    id: fk.key ?? fk.name ?? `inbound-${index}`,
    direction: "inbound",
    label: tableKey ? dbTableIdentityLabel(tableKey) : fk.table ?? "알 수 없는 테이블",
    tableKey,
    nodeId: tableKey ? dbTableNodeId(tableKey) : null,
    viaLabel: `${fk.columns.join(", ")} → ${fk.referencedColumns.join(", ")}`,
  };
}

function compareColumns(left: DbInventoryColumn, right: DbInventoryColumn): number {
  const rank = (column: DbInventoryColumn): number =>
    column.isPrimaryKey ? 0 : column.isForeignKey ? 1 : 2;
  return rank(left) - rank(right) || left.name.localeCompare(right.name);
}
