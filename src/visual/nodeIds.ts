import {
  dbColumnNodeId,
  dbTableIdentityLabel,
  dbTableNodeId,
  decodeDbIdentityComponent,
} from "../inventory/dbIdentity";

const DB_TABLE_PREFIX = "db:table:";
const DB_COLUMN_PREFIX = "db:column:";

export { dbColumnNodeId, dbTableIdentityLabel, dbTableNodeId };

export type DbColumnNodeRef = {
  tableKey: string;
  columnName: string;
};

export function columnRefFromNodeId(nodeId: string): DbColumnNodeRef | null {
  if (!nodeId.startsWith(DB_COLUMN_PREFIX)) {
    return null;
  }
  const body = nodeId.slice(DB_COLUMN_PREFIX.length);
  const splitIndex = body.lastIndexOf(":");
  if (splitIndex <= 0 || splitIndex >= body.length - 1) {
    return null;
  }
  const columnName = decodeDbIdentityComponent(body.slice(splitIndex + 1));
  return columnName === null ? null : { tableKey: body.slice(0, splitIndex), columnName };
}

export function tableKeyFromDbNodeId(nodeId: string): string | null {
  if (nodeId.startsWith(DB_TABLE_PREFIX)) {
    return nodeId.slice(DB_TABLE_PREFIX.length) || null;
  }
  return columnRefFromNodeId(nodeId)?.tableKey ?? null;
}

export function columnLabelFromNodeId(nodeId: string): string | null {
  const column = columnRefFromNodeId(nodeId);
  return column ? `${dbTableIdentityLabel(column.tableKey)}.${column.columnName}` : null;
}
