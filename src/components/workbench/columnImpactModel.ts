import type { ImpactReviewBoard, ImpactReviewItem } from "../../types/visual-map";

const TIER_LIMIT = 4;

type CascadeTierId = "structure" | "code" | "api";

export type CascadeTier = {
  id: CascadeTierId;
  title: string;
  description: string;
  items: ImpactReviewItem[];
  hidden: number;
};

export type ColumnImpactCascadeModel = {
  subject: string;
  tiers: CascadeTier[];
  total: number;
};

/**
 * Groups the confirmed/structural direct-impact items of a column into a
 * left-to-right cascade: DB structure facts → code that uses the column →
 * API routes that reach that code. This is a regrouping of the engine's
 * direct lane only — candidates and unknowns stay in their own lists.
 */
export function buildColumnImpactCascade(board: ImpactReviewBoard): ColumnImpactCascadeModel | null {
  const direct = board.lanes.find((lane) => lane.id === "direct") ?? board.lanes[0];
  const items = direct?.items ?? [];
  if (items.length === 0) return null;

  const structure: ImpactReviewItem[] = [];
  const code: ImpactReviewItem[] = [];
  const api: ImpactReviewItem[] = [];
  for (const item of items) {
    const tier = classify(item);
    if (tier === "api") api.push(item);
    else if (tier === "structure") structure.push(item);
    else code.push(item);
  }

  const tiers: CascadeTier[] = ([
    {
      id: "structure" as const,
      title: "DB 구조",
      description: "제약·인덱스·뷰 등 DB가 직접 아는 사실",
      items: structure.slice(0, TIER_LIMIT),
      hidden: Math.max(0, structure.length - TIER_LIMIT),
    },
    {
      id: "code" as const,
      title: "코드 사용",
      description: "이 컬럼을 읽거나 바꾸는 코드",
      items: code.slice(0, TIER_LIMIT),
      hidden: Math.max(0, code.length - TIER_LIMIT),
    },
    {
      id: "api" as const,
      title: "API 도달",
      description: "그 코드가 처리하는 API",
      items: api.slice(0, TIER_LIMIT),
      hidden: Math.max(0, api.length - TIER_LIMIT),
    },
  ] satisfies CascadeTier[]).filter((tier) => tier.items.length > 0);

  return tiers.length > 0 ? { subject: board.subject, tiers, total: items.length } : null;
}

function classify(item: ImpactReviewItem): CascadeTierId {
  const kind = item.kind.toLowerCase();
  if (kind.includes("route") || kind === "api") return "api";
  if (item.nodeId?.startsWith("code:route")) return "api";
  if (item.nodeId?.startsWith("db:")) return "structure";
  if (["constraint", "index", "view", "trigger", "fk", "foreign", "column", "table", "sequence"].some((token) => kind.includes(token))) {
    return "structure";
  }
  return "code";
}
