import type { SearchResult, SearchResultGroup } from "../types/controls";
import { codeInventoryCodeItems, codeRouteMethod, dbInventoryTableKey } from "../types/workspace";
import type {
  CodeInventory,
  CodeInventoryItem,
  DbInventory,
  DbInventoryColumn,
  DbInventoryTable,
} from "../types/workspace";

const SEARCH_GROUPS = [
  ["api:", "API"],
  ["code:", "코드"],
  ["file:", "파일"],
  ["table:", "테이블"],
  ["column:", "컬럼"],
] as const;

const SEARCH_GROUP_LIMIT = 4;

type RankedSearchResult = { score: number; result: SearchResult };

type IndexedCodeItem = {
  item: CodeInventoryItem;
  name: string;
  qualifiedName: string;
  path: string;
};

type IndexedDbColumn = {
  column: DbInventoryColumn;
  name: string;
  qualifiedName: string;
};

type IndexedDbTable = {
  table: DbInventoryTable;
  tableKey: string;
  qualifiedName: string;
  normalizedName: string;
  normalizedQualifiedName: string;
  normalizedTableKey: string;
  columns: IndexedDbColumn[];
};

type CodeSearchIndex = {
  routes: IndexedCodeItem[];
  code: IndexedCodeItem[];
  files: IndexedCodeItem[];
};

const codeSearchIndexes = new WeakMap<CodeInventory, CodeSearchIndex>();
const dbSearchIndexes = new WeakMap<DbInventory, IndexedDbTable[]>();
const EMPTY_CODE_INDEX: CodeSearchIndex = { routes: [], code: [], files: [] };

export type SearchCollection = {
  results: SearchResult[];
  total: number;
  counts: number[];
  truncated: boolean;
};

export function prepareSearchIndex(codeInventory: CodeInventory | null, dbInventory: DbInventory | null) {
  codeSearchIndex(codeInventory);
  dbSearchIndex(dbInventory);
}

export function collectSearchResults(
  query: string,
  codeInventory: CodeInventory | null,
  dbInventory: DbInventory | null,
): SearchCollection {
  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery.length < 2) {
    return emptySearchCollection();
  }
  const rankedGroups: RankedSearchResult[][] = SEARCH_GROUPS.map(() => []);
  const counts = SEARCH_GROUPS.map(() => 0);
  const codeIndex = codeSearchIndex(codeInventory);
  for (const indexed of codeIndex.routes) {
    const { item } = indexed;
    const score = indexedCodeItemSearchScore(indexed, normalizedQuery);
    if (score > 0) {
      const method = codeRouteMethod(item) ?? "ANY?";
      addSearchResult(rankedGroups, counts, 0, score, {
        id: `api:${item.id}`,
        title: `${method} ${item.name}`,
        subtitle: routeSearchIdentity(item),
        focusId: `code:${item.id}`,
        codeItem: item,
      });
    }
  }
  for (const indexed of codeIndex.code) {
    const { item } = indexed;
    const score = indexedCodeItemSearchScore(indexed, normalizedQuery);
    if (score > 0) {
      addSearchResult(rankedGroups, counts, 1, score, {
        id: `code:${item.id}`,
        title: item.name,
        subtitle: item.filePath ?? (item.kind.toLowerCase() === "unknown" ? "근거 미확인" : item.kind),
        focusId: `code:${item.id}`,
        codeItem: item,
      });
    }
  }
  for (const indexed of codeIndex.files) {
    const { item } = indexed;
    const score = indexedCodeItemSearchScore(indexed, normalizedQuery);
    if (score > 0) {
      addSearchResult(rankedGroups, counts, 2, score, {
        id: `file:${item.id}`,
        title: item.name,
        subtitle: item.filePath ?? "파일",
        focusId: `code:${item.id}`,
        codeItem: item,
      });
    }
  }
  for (const indexed of dbSearchIndex(dbInventory)) {
    const { table, tableKey } = indexed;
    const tableScore = searchScore(
      indexed.normalizedName,
      indexed.normalizedQualifiedName,
      indexed.normalizedTableKey,
      normalizedQuery,
    );
    if (tableScore > 0) {
      addSearchResult(rankedGroups, counts, 3, tableScore, {
        id: `table:${tableKey}`,
        title: table.name,
        subtitle: table.schema ?? "테이블",
        focusId: `db:table:${tableKey}`,
        tableKey,
      });
    }
    for (const indexedColumn of indexed.columns) {
      const { column } = indexedColumn;
      const columnScore = searchScore(
        indexedColumn.name,
        indexedColumn.qualifiedName,
        indexed.normalizedTableKey,
        normalizedQuery,
      );
      if (columnScore > 0) {
        addSearchResult(rankedGroups, counts, 4, columnScore, {
          id: `column:${tableKey}:${column.name}`,
          title: `${table.name}.${column.name}`,
          subtitle: `${tableKey}${column.dataType ? ` · ${column.dataType}` : ""}`,
          focusId: `db:column:${tableKey}:${column.name}`,
          tableKey,
        });
      }
    }
  }
  const results = rankedGroups.flatMap((group) => group.map(({ result }) => result));
  const total = counts.reduce((sum, count) => sum + count, 0);
  return { results, total, counts, truncated: total > results.length };
}

function routeSearchIdentity(item: CodeInventoryItem): string {
  if (item.filePath) {
    return item.line ? `${item.filePath}:L${item.line}` : item.filePath;
  }
  return item.qualifiedName ?? item.id;
}

export function groupSearchResults(results: SearchResult[]): SearchResultGroup[] {
  return SEARCH_GROUPS.map(([prefix, title]) => ({
    title,
    results: results.filter((result) => result.id.startsWith(prefix)).slice(0, 4),
  })).filter((group) => group.results.length > 0);
}

export function searchSummaryText(collection: SearchCollection): string {
  if (collection.total === 0) {
    return "일치하는 대상이 없습니다.";
  }
  const counts = SEARCH_GROUPS.map(([, title], index) => [title, collection.counts[index]] as const)
    .filter(([, count]) => count > 0)
    .map(([title, count]) => `${title} ${count}`)
    .join(" · ");
  return `찾은 대상 ${collection.total}개 · ${counts}`;
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

function indexedCodeItemSearchScore(item: IndexedCodeItem, query: string): number {
  return searchScore(item.name, item.qualifiedName, item.path, query);
}

function codeSearchIndex(inventory: CodeInventory | null): CodeSearchIndex {
  if (!inventory) {
    return EMPTY_CODE_INDEX;
  }
  const cached = codeSearchIndexes.get(inventory);
  if (cached) {
    return cached;
  }
  const index = {
    routes: inventory.routes.map(indexCodeItem),
    code: codeInventoryCodeItems(inventory).map(indexCodeItem),
    files: inventory.files.map(indexCodeItem),
  };
  codeSearchIndexes.set(inventory, index);
  return index;
}

function indexCodeItem(item: CodeInventoryItem): IndexedCodeItem {
  return {
    item,
    name: item.name.toLowerCase(),
    qualifiedName: (item.qualifiedName ?? "").toLowerCase(),
    path: (item.filePath ?? "").toLowerCase(),
  };
}

function dbSearchIndex(inventory: DbInventory | null): IndexedDbTable[] {
  if (!inventory) {
    return [];
  }
  const cached = dbSearchIndexes.get(inventory);
  if (cached) {
    return cached;
  }
  const index = inventory.tables.map((table) => {
    const tableKey = dbInventoryTableKey(table);
    const qualifiedName = table.schema ? `${table.schema}.${table.name}` : table.name;
    return {
      table,
      tableKey,
      qualifiedName,
      normalizedName: table.name.toLowerCase(),
      normalizedQualifiedName: qualifiedName.toLowerCase(),
      normalizedTableKey: tableKey.toLowerCase(),
      columns: table.columns.map((column) => ({
        column,
        name: column.name.toLowerCase(),
        qualifiedName: `${qualifiedName}.${column.name}`.toLowerCase(),
      })),
    };
  });
  dbSearchIndexes.set(inventory, index);
  return index;
}

function addSearchResult(
  groups: RankedSearchResult[][],
  counts: number[],
  group: number,
  score: number,
  result: SearchResult,
) {
  counts[group] += 1;
  const ranked = groups[group];
  ranked.push({ score, result });
  ranked.sort(compareRankedSearchResult);
  if (ranked.length > SEARCH_GROUP_LIMIT) {
    ranked.pop();
  }
}

function compareRankedSearchResult(left: RankedSearchResult, right: RankedSearchResult): number {
  return (
    right.score - left.score ||
    compareText(left.result.title, right.result.title) ||
    compareText(left.result.id, right.result.id)
  );
}

function emptySearchCollection(): SearchCollection {
  return {
    results: [],
    total: 0,
    counts: SEARCH_GROUPS.map(() => 0),
    truncated: false,
  };
}

function searchScore(name: string, qualifiedName: string, path: string, query: string): number {
  const basename = path.split(/[\\/]/).pop() ?? "";
  if (name === query) return 1_000;
  if (name.startsWith(query)) return 900;
  if (qualifiedName === query) return 850;
  if (qualifiedName.startsWith(query)) return 800;
  if (basename === query) return 700;
  if (basename.startsWith(query)) return 650;
  if (name.includes(query)) return 500;
  if (qualifiedName.includes(query)) return 400;
  if (path.includes(query)) return 300;
  return 0;
}

function compareText(left: string, right: string): number {
  const normalizedLeft = left.toLowerCase();
  const normalizedRight = right.toLowerCase();
  return normalizedLeft < normalizedRight ? -1 : normalizedLeft > normalizedRight ? 1 : 0;
}
