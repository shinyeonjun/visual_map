const DB_TABLE_PREFIX = "db:table:";
const DB_COLUMN_PREFIX = "db:column:";

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
  return splitIndex > 0 && splitIndex < body.length - 1
    ? { tableKey: body.slice(0, splitIndex), columnName: body.slice(splitIndex + 1) }
    : null;
}

export function tableKeyFromDbNodeId(nodeId: string): string | null {
  if (nodeId.startsWith(DB_TABLE_PREFIX)) {
    return nodeId.slice(DB_TABLE_PREFIX.length) || null;
  }
  return columnRefFromNodeId(nodeId)?.tableKey ?? null;
}

export function columnLabelFromNodeId(nodeId: string): string | null {
  const column = columnRefFromNodeId(nodeId);
  return column ? `${column.tableKey}.${column.columnName}` : null;
}
