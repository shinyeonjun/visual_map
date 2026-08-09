/**
 * What the canvas draws.
 *
 * This is the read model, not the fact graph. The engine owns nodes, edges
 * and evidence; this is the shape those become once they have been folded to
 * something a person can look at — areas instead of files, one relation
 * instead of the ninety fact edges it stands for.
 *
 * Nothing here may be invented by the view. Every count comes from the
 * engine's own accounting, and every relation carries the class the engine
 * assigned it. The canvas decides where things sit, never what is true.
 */

/**
 * How far the engine got with a relation.
 *
 * `verified` — resolved to an exact target with source evidence.
 * `structural` — a containment or import fact, not an execution claim.
 * `candidate` — proposed, evidence present, target not resolved.
 *
 * There is deliberately no member for "unknown": a relation the engine could
 * not classify is not drawn at all, so it never reaches this type.
 */
export type TruthClass = "verified" | "structural" | "candidate";

/** Drives the icon only. The engine's own `kind` string is what gets shown. */
export type NodeRole = "endpoint" | "controller" | "service" | "repository" | "table" | "event" | "code";

export interface MapNode {
  id: string;
  name: string;
  /** The engine's wording — "HTTP Endpoint", "Controller", "PostgreSQL Table". */
  kind: string;
  role: NodeRole;
}

/** How the deterministic static walk ended; never an AI confidence score. */
export type TraceState = "complete" | "partial" | "gap" | "cycle" | "depth-limited";

export interface MapTraceMeta {
  id: string;
  state: TraceState;
  stepCount: number;
}

export interface MapTrace {
  id: string;
  state: TraceState;
  steps: MapNode[];
}

export interface MapArea {
  id: string;
  name: string;
  /** The original identifier, kept beside a translated name so both are visible. */
  originalName?: string | null;
  summary: string;
  /** 0 for a top-level area, 1 for one nested inside it. */
  depth: number;
  areas: MapArea[];
  nodes: MapNode[];
  /**
   * Members left off the canvas. Shown as a count so nothing disappears
   * silently — a box that quietly drops half its contents is a lie.
   */
  hiddenNodeCount: number;
  /** Where the reader put it. Top-level only; nested areas lay out on their own. */
  position?: { x: number; y: number } | null;
  width?: number | null;
  /** Present only when `nodes` are one verified static execution path. */
  trace?: MapTraceMeta | null;
}

export interface MapRelation {
  id: string;
  from: string;
  to: string;
  truth: TruthClass;
  /** "호출", "사용", "구성" — the reader's word for the relation. */
  label: string;
  /** How many fact edges this one line stands for. */
  count: number;
}

export interface MapView {
  areas: MapArea[];
  relations: MapRelation[];
}

/** One line of the relation breakdown in the inspector. */
export interface RelationTally {
  label: string;
  truth: TruthClass;
  count: number;
}

export interface EvidenceRef {
  path: string;
  line?: number | null;
}

export interface SourceExcerpt {
  path: string;
  /** Line number of the first entry in `lines`. */
  startLine: number;
  lines: string[];
  /** The line the evidence points at. */
  hitLine: number;
}

export interface Selection {
  id: string;
  title: string;
  role: string;
  relations: RelationTally[];
  evidence: EvidenceRef[];
  source: SourceExcerpt | null;
  /** Bounded static paths that start at, or are fully contained by, the selection. */
  traces?: MapTrace[];
}

/** Every area on the map, flattened, parents before children. */
export function flattenAreas(areas: MapArea[]): MapArea[] {
  const out: MapArea[] = [];
  const walk = (list: MapArea[]) => {
    for (const area of list) {
      out.push(area);
      walk(area.areas);
    }
  };
  walk(areas);
  return out;
}

/**
 * Relations that both end on something currently drawn.
 *
 * A relation pointing at an area that is not on the canvas is dropped rather
 * than stretched to its parent here — the engine folds relations to the level
 * it publishes, so re-folding in the view would double-count.
 */
export function drawableRelations(view: MapView): MapRelation[] {
  const present = new Set(flattenAreas(view.areas).map((area) => area.id));
  return view.relations.filter(
    (relation) => relation.from !== relation.to && present.has(relation.from) && present.has(relation.to),
  );
}
