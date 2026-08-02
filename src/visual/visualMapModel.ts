import type { SearchResult } from "../types/controls";
import type { AnalysisCoverage, ChangeIntent, InventorySnapshot, VisualMap } from "../types/visual-map";

export type RelationView = "connections" | "calls" | "data" | "impact";

const VISUAL_MAP_CACHE_LIMIT = 32;

export function compositionSearchResultIsSupported(result: SearchResult): boolean {
  return !result.id.startsWith("db-object:");
}

export function sourceSummary(snapshot: InventorySnapshot): string | null {
  const code = snapshot.metadata?.code;
  const codeLabel = code?.sourceRevisionLabel
    ? code.sourceType === "github-clone"
      ? `${code.sourceRevisionLabel} · 원격 최신 여부 미확인`
      : `${code.sourceRevisionLabel} · 로컬 상태 확인`
    : null;
  const labels = [codeLabel, snapshot.metadata?.db?.sourceRevisionLabel].filter((label): label is string =>
    Boolean(label),
  );
  return labels.length > 0 ? labels.join(" · ") : null;
}

export function coverageFromSnapshot(snapshot: InventorySnapshot): AnalysisCoverage {
  const code = snapshot.metadata?.code;
  const db = snapshot.metadata?.db;
  const gaps = snapshot.metadata?.gaps ?? [];
  return {
    code: {
      available: Boolean(code),
      observed: code?.resultCount ?? null,
      total: null,
      limit: code?.limitApplied ?? null,
      truncated: Boolean(code?.truncated || code?.limitClamped),
    },
    db: {
      available: Boolean(db),
      observed: db?.resultCount ?? null,
      total: db?.totalTables ?? null,
      limit: db?.limitApplied ?? null,
      truncated: Boolean(db?.truncated || db?.limitClamped),
    },
    gaps: gaps.filter((gap) => gap.kind !== "db-capability").length,
    capabilities: gaps.filter((gap) => gap.kind === "db-capability").length,
    reindexRequired: Boolean(snapshot.metadata?.migration?.reindexRequired),
  };
}

export function mapRequestKey(
  workspaceId: string,
  mode: string,
  focusId?: string | null,
  changeIntent?: ChangeIntent | null,
  focusIds: string[] = [],
  relationView: RelationView = "connections",
): string {
  return `${workspaceId}\u0000${mode}\u0000${focusId ?? ""}\u0000${changeIntent?.kind ?? ""}\u0000${changeIntent?.value ?? ""}\u0000${focusIds.join("\u001f")}\u0000${relationView}`;
}

export function visualMapCacheKey(kind: "base" | "enriched", targetKey: string, generation: number): string {
  return `${generation}\u0000${kind}\u0000${targetKey}`;
}

export function rememberVisualMap(cache: Map<string, VisualMap>, key: string, map: VisualMap): void {
  cache.delete(key);
  cache.set(key, map);
  while (cache.size > VISUAL_MAP_CACHE_LIMIT) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

export function mapAnswersMode(map: VisualMap | null, mode: string): boolean {
  if (!map) return false;
  if (mode === "api-flow") return Boolean(map.apiReading);
  if (mode === "table-usage" || mode === "column-impact") return Boolean(map.reviewBoard);
  if (mode === "composition") return map.mode === "composition" && map.nodes.length >= 2;
  return map.nodes.length > 0;
}

export function selectedSearchSummary(result: SearchResult): string {
  const target = result.subtitle ? `${result.title} · ${result.subtitle}` : result.title;
  return `${searchModeLabelForResult(result)} · ${target}`;
}

export function searchModeForResult(result: SearchResult): string {
  if (result.id.startsWith("api:")) return "api-flow";
  if (result.id.startsWith("table:")) return "table-usage";
  if (result.id.startsWith("column:")) return "column-impact";
  return "search-focus";
}

function searchModeLabelForResult(result: SearchResult): string {
  if (result.id.startsWith("api:")) return "API가 닿는 코드";
  if (result.id.startsWith("table:")) return "테이블 연결";
  if (result.id.startsWith("column:")) return "컬럼 변경 범위";
  return "대상 주변 근거";
}
