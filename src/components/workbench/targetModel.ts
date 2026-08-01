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

export type TargetItem = {
  id: string;
  kind: TargetKind;
  badge: string;
  title: string;
  meta: string;
  sourcePath?: string | null;
  group?: string;
  focusId: string;
  mode: "api-flow" | "search-focus" | "table-usage" | "column-impact";
};

export type TargetCatalog = Record<TargetKind, TargetItem[]>;

export type ApiTreeNode = {
  key: string;
  label: string;
  children: ApiTreeNode[];
  items: TargetItem[];
};

export type CodeTreeNode = {
  key: string;
  label: string;
  children: CodeTreeNode[];
  items: TargetItem[];
  isFile: boolean;
};

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
  const tables = [...(dbInventory?.tables ?? [])]
    .sort((left, right) => dbInventoryTableKey(left).localeCompare(dbInventoryTableKey(right)));

  return {
    api: [...(codeInventory?.routes ?? [])].filter(isProjectCodeItem).sort(compareApiTargets).map((route) => ({
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
      sourcePath: item.filePath,
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

export function buildApiTree(items: TargetItem[]): ApiTreeNode {
  const root: ApiTreeNode = { key: "root", label: "", children: [], items: [] };

  for (const item of items) {
    let node = root;
    for (const segment of apiPathSegments(item.title)) {
      let child = node.children.find((candidate) => candidate.label === segment);
      if (!child) {
        child = { key: `${node.key}/${segment}`, label: segment, children: [], items: [] };
        node.children.push(child);
      }
      node = child;
    }
    node.items.push(item);
  }

  sortApiTree(root);
  return root;
}

export function buildCodeTree(items: TargetItem[]): CodeTreeNode {
  const root: CodeTreeNode = { key: "root", label: "", children: [], items: [], isFile: false };

  for (const item of items) {
    let node = root;
    const segments = codePathSegments(item.sourcePath);
    segments.forEach((segment, index) => {
      let child = node.children.find((candidate) => candidate.label === segment);
      if (!child) {
        child = {
          key: `${node.key}/${segment}`,
          label: segment,
          children: [],
          items: [],
          isFile: index === segments.length - 1,
        };
        node.children.push(child);
      }
      node = child;
    });
    node.items.push(item);
  }

  sortCodeTree(root);
  return root;
}

export function codePathSegments(filePath: string | null | undefined): string[] {
  const value = filePath?.trim().replace(/\\/g, "/") ?? "";
  if (!value) return ["소스 위치 없음"];
  const segments = value.split("/").filter((segment) => segment && segment !== ".");
  if (/^[A-Za-z]:$/.test(segments[0] ?? "")) segments.shift();
  return segments.length > 0 ? segments : ["소스 위치 없음"];
}

export function countCodeTreeItems(node: CodeTreeNode): number {
  return node.items.length + node.children.reduce((total, child) => total + countCodeTreeItems(child), 0);
}

export function apiPathSegments(title: string): string[] {
  const value = title.trim();
  const slashIndex = value.indexOf("/");
  const path = slashIndex >= 0 ? value.slice(slashIndex) : value;
  const segments = path.split("/").filter(Boolean);
  return segments.length > 0 ? segments : [path || title.trim() || "/"];
}

function sortApiTree(node: ApiTreeNode): void {
  node.children.sort((left, right) => left.label.localeCompare(right.label, "ko-KR", { numeric: true }));
  node.items.sort((left, right) => left.title.localeCompare(right.title) || left.id.localeCompare(right.id));
  for (const child of node.children) sortApiTree(child);
}

function sortCodeTree(node: CodeTreeNode): void {
  node.children.sort((left, right) => Number(left.isFile) - Number(right.isFile)
    || left.label.localeCompare(right.label, "ko-KR", { numeric: true }));
  node.items.sort((left, right) => left.title.localeCompare(right.title, "ko-KR", { numeric: true })
    || left.meta.localeCompare(right.meta, "ko-KR", { numeric: true })
    || left.id.localeCompare(right.id));
  for (const child of node.children) sortCodeTree(child);
}

function compareApiTargets(left: CodeInventory["routes"][number], right: CodeInventory["routes"][number]): number {
  return left.name.localeCompare(right.name)
    || (codeRouteMethod(left) ?? "").localeCompare(codeRouteMethod(right) ?? "")
    || (left.filePath ?? "").localeCompare(right.filePath ?? "")
    || (left.line ?? 0) - (right.line ?? 0)
    || left.id.localeCompare(right.id);
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
