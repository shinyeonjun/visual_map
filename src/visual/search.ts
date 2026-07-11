import type { SearchResult, SearchResultGroup } from "../types/controls";
import { codeInventoryCodeItems, dbInventoryTableKey } from "../types/workspace";
import type { CodeInventory, CodeInventoryItem, DbInventory } from "../types/workspace";

const SEARCH_GROUPS = [
  ["api:", "API"],
  ["code:", "코드"],
  ["file:", "파일"],
  ["table:", "테이블"],
  ["column:", "컬럼"],
] as const;

export function collectSearchResults(
  query: string,
  codeInventory: CodeInventory | null,
  dbInventory: DbInventory | null,
): SearchResult[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery.length < 2) {
    return [];
  }
  const results: Array<{ group: number; score: number; result: SearchResult }> = [];
  for (const item of codeInventory?.routes ?? []) {
    const score = codeItemSearchScore(item, normalizedQuery);
    if (score > 0) {
      results.push({ group: 0, score, result: {
        id: `api:${item.id}`,
        title: item.name,
        subtitle: item.filePath ?? "API",
        focusId: `code:${item.id}`,
        codeItem: item,
      } });
    }
  }
  for (const item of codeInventoryCodeItems(codeInventory)) {
    const score = codeItemSearchScore(item, normalizedQuery);
    if (score > 0) {
      results.push({ group: 1, score, result: {
        id: `code:${item.id}`,
        title: item.name,
        subtitle: item.filePath ?? item.kind,
        focusId: `code:${item.id}`,
        codeItem: item,
      } });
    }
  }
  for (const item of codeInventory?.files ?? []) {
    const score = codeItemSearchScore(item, normalizedQuery);
    if (score > 0) {
      results.push({ group: 2, score, result: {
        id: `file:${item.id}`,
        title: item.name,
        subtitle: item.filePath ?? "파일",
        focusId: `code:${item.id}`,
        codeItem: item,
      } });
    }
  }
  for (const table of dbInventory?.tables ?? []) {
    const tableKey = dbInventoryTableKey(table);
    const qualifiedTable = table.schema ? `${table.schema}.${table.name}` : table.name;
    const tableScore = searchScore(table.name, qualifiedTable, tableKey, normalizedQuery);
    if (tableScore > 0) {
      results.push({ group: 3, score: tableScore, result: {
        id: `table:${tableKey}`,
        title: table.name,
        subtitle: table.schema ?? "테이블",
        focusId: `db:table:${tableKey}`,
        tableKey,
      } });
    }
    for (const column of table.columns) {
      const qualifiedColumn = `${qualifiedTable}.${column.name}`;
      const columnScore = searchScore(column.name, qualifiedColumn, tableKey, normalizedQuery);
      if (columnScore > 0) {
        results.push({ group: 4, score: columnScore, result: {
          id: `column:${tableKey}:${column.name}`,
          title: `${table.name}.${column.name}`,
          subtitle: `${tableKey}${column.dataType ? ` · ${column.dataType}` : ""}`,
          focusId: `db:column:${tableKey}:${column.name}`,
          tableKey,
        } });
      }
    }
  }
  results.sort(
    (left, right) =>
      left.group - right.group ||
      right.score - left.score ||
      compareText(left.result.title, right.result.title) ||
      compareText(left.result.id, right.result.id),
  );
  return results.map(({ result }) => result);
}

export function groupSearchResults(results: SearchResult[]): SearchResultGroup[] {
  return SEARCH_GROUPS.map(([prefix, title]) => ({
    title,
    results: results.filter((result) => result.id.startsWith(prefix)).slice(0, 4),
  })).filter((group) => group.results.length > 0);
}

export function searchSummaryText(results: SearchResult[]): string {
  if (results.length === 0) {
    return "일치하는 대상이 없습니다.";
  }
  const counts = SEARCH_GROUPS.map(
    ([prefix, title]) => [title, results.filter((result) => result.id.startsWith(prefix)).length] as const,
  )
    .filter(([, count]) => count > 0)
    .map(([title, count]) => `${title} ${count}`)
    .join(" · ");
  return `찾은 대상 ${results.length}개 · ${counts}`;
}

export function searchScopeText(codeInventory: CodeInventory | null, dbInventory: DbInventory | null): string {
  const scopes = [
    codeInventory?.routes.length ? "API" : null,
    codeInventoryCodeItems(codeInventory).length ? "코드" : null,
    codeInventory?.files.length ? "파일" : null,
    dbInventory?.tables.length ? "테이블" : null,
    dbInventory?.tables.some((table) => table.columns.length > 0) ? "컬럼" : null,
  ].filter(Boolean);
  return scopes.length > 0 ? scopes.join(" · ") : "항목";
}

function codeItemSearchScore(item: CodeInventoryItem, query: string): number {
  return searchScore(item.name, item.qualifiedName ?? "", item.filePath ?? "", query);
}

function searchScore(name: string, qualifiedName: string, path: string, query: string): number {
  const normalizedName = name.toLowerCase();
  const normalizedQualifiedName = qualifiedName.toLowerCase();
  const normalizedPath = path.toLowerCase();
  const basename = normalizedPath.split(/[\\/]/).pop() ?? "";
  if (normalizedName === query) return 1_000;
  if (normalizedName.startsWith(query)) return 900;
  if (normalizedQualifiedName === query) return 850;
  if (normalizedQualifiedName.startsWith(query)) return 800;
  if (basename === query) return 700;
  if (basename.startsWith(query)) return 650;
  if (normalizedName.includes(query)) return 500;
  if (normalizedQualifiedName.includes(query)) return 400;
  if (normalizedPath.includes(query)) return 300;
  return 0;
}

function compareText(left: string, right: string): number {
  const normalizedLeft = left.toLowerCase();
  const normalizedRight = right.toLowerCase();
  return normalizedLeft < normalizedRight ? -1 : normalizedLeft > normalizedRight ? 1 : 0;
}
