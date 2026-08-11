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

/** Semantic responsibility class approved by the meaning compiler. */
export type AreaCategory = "domain" | "shared" | "infrastructure" | "integration" | "structural";

/** Whether the displayed area label was semantically derived or structurally retained. */
export type LabelSource = "semantic" | "structural";

/** Why the verifier required a structural label instead of a semantic one. */
export type SemanticFallbackReason = "insufficient-semantic-signal" | "mixed-responsibility";

/** Fact-relation records crossing one semantic area's member boundary. */
export interface MapTruthCounts {
  verified: number;
  structural: number;
  candidate: number;
}

/** Drives the icon only. The engine's own `kind` string is what gets shown. */
export type NodeRole =
  "endpoint" | "controller" | "service" | "repository" | "table" | "table-reference" | "event" | "code";

export interface MapNode {
  id: string;
  name: string;
  /** The engine's wording — "HTTP Endpoint", "Controller", "PostgreSQL Table". */
  kind: string;
  role: NodeRole;
  /** Exact definition location; never substituted with an incoming call site. */
  definition: EvidenceRef | null;
}

/** How the deterministic static walk ended; never an AI confidence score. */
export type TraceState = "complete" | "partial" | "gap" | "cycle" | "depth-limited";

export interface MapTraceMeta {
  id: string;
  state: TraceState;
  stepCount: number;
}

/**
 * How certain the engine is about *which* target a call reaches.
 *
 * `direct` — the target is fixed at the call site.
 * `virtual` / `interface` — the receiver is chosen at runtime among the
 *   implementations of a type the engine did not enumerate.
 * `dynamic` — the language allows the target to change at runtime, so static
 *   analysis cannot close the question at all.
 * `unknown` — the provider resolved a target but lost the classification.
 * `not-applicable` / `unreported` — the relation is not a call.
 *
 * A line on the map may not present these as the same thing: "we know" and
 * "we found one possibility" are different claims.
 */
export type DispatchKind = "direct" | "virtual" | "interface" | "dynamic" | "unknown" | "not-applicable" | "unreported";

/**
 * Where a call is written, and what the source says about whether it runs.
 *
 * These are lexical facts about the written code, never observations of a
 * running program: `guarded` means the call sits inside a branch, not that it
 * was skipped, and the ordinal is source order rather than execution order.
 */
export interface MapExecutionOccurrence {
  callSiteEvidenceId: string;
  callSite: EvidenceRef | null;
  lexicalOrdinal: number;
  guarded: boolean;
  repeated: boolean;
  deferred: boolean;
  awaited: boolean;
}

/** One step of a path, and the evidence for that step specifically. */
export interface MapTraceHop {
  id: string;
  from: string;
  to: string;
  kind: string;
  truth: TruthClass;
  dispatch: DispatchKind;
  evidence: EvidenceRef[];
  execution: MapExecutionOccurrence | null;
}

export interface MapTrace {
  id: string;
  state: TraceState;
  steps: MapNode[];
  /** One shorter than `steps`: the move between each pair. */
  hops?: MapTraceHop[];
}

export interface MapArea {
  id: string;
  name: string;
  /** The original identifier, kept beside a translated name so both are visible. */
  originalName?: string | null;
  summary: string;
  /** AI-proposed and verifier-approved category; never inferred by the UI. */
  category: AreaCategory;
  labelSource: LabelSource;
  fallbackReason: SemanticFallbackReason | null;
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
  /** Fact relations with exactly one endpoint in this area's effective members. */
  boundaryRelationCounts: MapTruthCounts;
  /** Canonical gap records whose declared scope overlaps this area. Not additive across parent/child areas. */
  affectingAnalysisGapCount: number;
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
  /** How those edges split across dispatch precision. Preview data omits it. */
  dispatches?: MapDispatchTally[];
}

export interface MapView {
  areas: MapArea[];
  relations: MapRelation[];
  /** Workspace-wide or otherwise unassignable canonical analysis gaps. */
  unattributedAnalysisGapCount: number;
}

/** One line of the relation breakdown in the inspector. */
export interface RelationTally {
  label: string;
  truth: TruthClass;
  count: number;
  /*
    Absent only in the development preview, which has no basis for inventing a
    dispatch classification. The published read model always reports one.
  */
  dispatch?: DispatchKind;
}

export interface MapDispatchTally {
  dispatch: DispatchKind;
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

export interface AnalysisGapItem {
  code: string;
  capability: string | null;
  message: string;
}

/** Bounded canonical gaps that affect the current selection. */
export interface AnalysisGapSummary {
  totalCount: number;
  items: AnalysisGapItem[];
  truncatedCount: number;
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
  analysisGaps: AnalysisGapSummary;
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

/**
 * The relations that touch what the reader is pointing at, or `null` when they
 * are pointing at nothing.
 *
 * Relations run between top-level areas, but a reader clicks whatever they can
 * see — a nested area, or one member of one. Resolving that back to the area
 * it belongs to is what lets "show me this one's connections" answer from any
 * depth rather than only from an area header.
 */
export function relationsTouching(view: MapView, focusId: string | null): Set<string> | null {
  const owner = owningAreaId(view, focusId);
  if (!owner) return null;
  return new Set(
    drawableRelations(view)
      .filter((relation) => relation.from === owner || relation.to === owner)
      .map((relation) => relation.id),
  );
}

/**
 * The top-level area an id belongs to, whether it is the area itself, a nested
 * area, or one member of one.
 *
 * Relations and execution paths are published per top-level area, so anything
 * the reader can click has to be resolved back to its owner before either can
 * answer for it.
 */
export function owningAreaId(view: MapView, id: string | null): string | null {
  if (!id) return null;
  return view.areas.find((area) => areaContains(area, id))?.id ?? null;
}

function areaContains(area: MapArea, id: string): boolean {
  return (
    area.id === id || area.nodes.some((node) => node.id === id) || area.areas.some((child) => areaContains(child, id))
  );
}
