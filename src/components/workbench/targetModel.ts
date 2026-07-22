import type { CodeInventory, DbInventory } from "../../types/workspace";
import {
  codeInventoryCodeItems,
  codeKindChip,
  codeRouteMethod,
  dbInventoryTableKey,
} from "../../types/workspace";
import {
  dbColumnNodeId,
  dbTableIdentityLabel,
  dbTableNodeId,
} from "../../visual/nodeIds";

export type TargetKind = "api" | "code" | "table" | "column";

export type TargetItem = {
  id: string;
  kind: TargetKind;
  badge: string;
  title: string;
  meta: string;
  group?: string;
  focusId: string;
  mode: "api-flow" | "search-focus" | "table-usage" | "column-impact";
};

export type TargetCatalog = Record<TargetKind, TargetItem[]>;

export function buildTargetCatalog(
  codeInventory: CodeInventory | null,
  dbInventory: DbInventory | null,
): TargetCatalog {
  const tables = dbInventory?.tables ?? [];

  return {
    api: (codeInventory?.routes ?? []).map((route) => ({
      id: `api:${route.id}`,
      kind: "api",
      badge: codeRouteMethod(route) ?? "API",
      title: route.name,
      meta: sourceLocation(route.filePath, route.line),
      focusId: `code:${route.id}`,
      mode: "api-flow",
    })),
    code: [
      ...codeInventoryCodeItems(codeInventory),
      ...(codeInventory?.files ?? []),
    ].map((item) => ({
      id: `code:${item.id}`,
      kind: "code",
      badge: codeKindChip(item.kind),
      title: item.name,
      meta: sourceLocation(item.filePath, item.line),
      focusId: `code:${item.id}`,
      mode: "search-focus",
    })),
    table: tables.map((table) => {
      const tableKey = dbInventoryTableKey(table);
      return {
        id: `table:${tableKey}`,
        kind: "table",
        badge: "TABLE",
        title: dbTableIdentityLabel(tableKey),
        meta: `컬럼 ${table.columns.length.toLocaleString("ko-KR")}개`,
        focusId: dbTableNodeId(tableKey),
        mode: "table-usage",
      };
    }),
    column: tables.flatMap((table) => {
      const tableKey = dbInventoryTableKey(table);
      return table.columns.map((column) => ({
        id: `column:${tableKey}:${column.name}`,
        kind: "column" as const,
        badge: column.isPrimaryKey ? "PK" : column.isForeignKey ? "FK" : "COL",
        title: column.name,
        meta: column.dataType ?? "타입 정보 없음",
        group: dbTableIdentityLabel(tableKey),
        focusId: dbColumnNodeId(tableKey, column.name),
        mode: "column-impact" as const,
      }));
    }),
  };
}

export function targetKindForMode(mode: string): TargetKind | null {
  if (mode === "api-flow") return "api";
  if (mode === "search-focus") return "code";
  if (mode === "table-usage") return "table";
  if (mode === "column-impact") return "column";
  return null;
}

export function firstAvailableTargetKind(catalog: TargetCatalog): TargetKind {
  return (["api", "code", "table", "column"] as const).find((kind) => catalog[kind].length > 0) ?? "api";
}

function sourceLocation(path: string | null | undefined, line: number | null | undefined): string {
  if (!path) return line ? `L${line}` : "소스 위치 없음";
  const compactPath = path.replace(/\\/g, "/").split("/").filter(Boolean).slice(-2).join("/");
  return `${compactPath}${line ? `:${line}` : ""}`;
}
