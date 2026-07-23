import type { CodeInventory, DbInventory } from "../../types/workspace";
import {
  codeInventoryCodeItems,
  codeKindChip,
  codeRouteMethod,
  dbInventoryTableKey,
  isProjectCodeItem,
} from "../../types/workspace";
import {
  dbColumnNodeId,
  dbTableIdentityLabel,
  dbTableNodeId,
} from "../../visual/nodeIds";

export type TargetKind = "api" | "code" | "table" | "column";

type TargetItem = {
  id: string;
  kind: TargetKind;
  badge: string;
  title: string;
  meta: string;
  group?: string;
  focusId: string;
  mode: "api-flow" | "search-focus" | "table-usage" | "column-impact";
};

type TargetCatalog = Record<TargetKind, TargetItem[]>;

const CODE_TARGET_RANK: Record<string, number> = {
  handler: 0,
  controller: 0,
  service: 1,
  repository: 2,
  function: 3,
  method: 3,
  class: 4,
  module: 5,
  unknown: 6,
  file: 7,
};

const CODE_TARGET_GROUP: Record<string, string> = {
  handler: "핸들러",
  controller: "핸들러",
  service: "서비스",
  repository: "리포지토리",
  function: "함수",
  method: "함수",
  class: "클래스",
  module: "모듈",
  unknown: "확인 필요",
  file: "파일",
};

export function buildTargetCatalog(
  codeInventory: CodeInventory | null,
  dbInventory: DbInventory | null,
): TargetCatalog {
  const tables = dbInventory?.tables ?? [];

  return {
    api: (codeInventory?.routes ?? []).filter(isProjectCodeItem).map((route) => ({
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
    ].filter(isProjectCodeItem).sort(compareCodeTargets).map((item) => ({
      id: `code:${item.id}`,
      kind: "code",
      badge: codeKindChip(item.kind),
      title: item.name,
      meta: sourceLocation(item.filePath, item.line),
      group: CODE_TARGET_GROUP[item.kind.toLowerCase()] ?? "기타",
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

function compareCodeTargets(left: CodeInventory["functions"][number], right: CodeInventory["functions"][number]): number {
  const rank = (CODE_TARGET_RANK[left.kind.toLowerCase()] ?? 6) - (CODE_TARGET_RANK[right.kind.toLowerCase()] ?? 6);
  return rank || left.name.localeCompare(right.name) || (left.filePath ?? "").localeCompare(right.filePath ?? "");
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
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  const compactPath = parts.length > 3
    ? `${parts[0]}/…/${parts.slice(-2).join("/")}`
    : parts.join("/");
  return `${compactPath}${line ? `:${line}` : ""}`;
}
