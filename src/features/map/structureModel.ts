import type { VisualEdge, VisualNode } from "../../types/visual-map";

export type FlowTone = "api" | "code" | "db" | "package" | "module" | "external";

export type StructureNode = {
  id: string;
  title: string;
  /** "패키지", "모듈", "API", "코드", "DB" — what the box is, not what it holds. */
  kindLabel: string;
  tone: FlowTone;
  depth: number;
  /** Path or location under the title. */
  meta: string;
  /** Composition line in the foot, empty for a leaf. */
  metric: string;
  children: StructureNode[];
  /** False for a leaf the reader cannot open any further. */
  expandable: boolean;
  node: VisualNode;
};

/**
 * The containment tree behind the canvas.
 *
 * Membership is single-parent: a file lives in exactly one module, a module in
 * exactly one package. Call relations are deliberately not in here — those are
 * a graph, drawn as arrows over this tree, and a service reached from three
 * routes has three callers but still only one home.
 */
export function buildStructureTree(
  roots: VisualNode[],
  childrenOf: (id: string) => VisualNode[],
  expandedPath: string[],
): StructureNode[] {
  const openAtDepth = new Map(expandedPath.map((id, depth) => [depth, id]));

  const build = (node: VisualNode, depth: number): StructureNode => {
    const isOpen = openAtDepth.get(depth) === node.id;
    const rawChildren = isOpen ? childrenOf(node.id) : [];
    return {
      id: node.id,
      title: node.title,
      kindLabel: kindLabelOf(node, depth),
      tone: toneOf(node, depth),
      depth,
      meta: metaOf(node),
      metric: metricOf(node),
      children: rawChildren.map((child) => build(child, depth + 1)),
      // Source locations belong in the inspector. Only containment opens a box.
      expandable: isGroup(node) || rawChildren.length > 0,
      node,
    };
  };

  return roots.map((node) => build(node, 0));
}

/** Every box currently on the canvas, in render order. */
export function flattenStructure(nodes: StructureNode[]): StructureNode[] {
  const out: StructureNode[] = [];
  const walk = (list: StructureNode[]) => {
    for (const node of list) {
      out.push(node);
      walk(node.children);
    }
  };
  walk(nodes);
  return out;
}

/**
 * Relations, redirected to the boxes that are actually on screen.
 *
 * An edge into a collapsed package must not vanish — it is drawn to the package
 * itself. Without this, closing a box silently deletes every relation that
 * pointed inside it and the canvas looks less connected than the project is.
 */
export function resolveConnectors(
  edges: VisualEdge[],
  visible: StructureNode[],
  ownerOf: (nodeId: string) => string | null,
): Array<{ id: string; from: string; to: string; tone: string; label: string; edge: VisualEdge }> {
  const onScreen = new Set(visible.map((node) => node.id));
  const resolve = (nodeId: string): string | null => {
    if (onScreen.has(nodeId)) return nodeId;
    let owner = ownerOf(nodeId);
    const guard = new Set<string>();
    while (owner && !guard.has(owner)) {
      if (onScreen.has(owner)) return owner;
      guard.add(owner);
      owner = ownerOf(owner);
    }
    return null;
  };

  const seen = new Set<string>();
  const connectors = [];
  for (const edge of edges) {
    const from = resolve(edge.from);
    const to = resolve(edge.to);
    if (!from || !to || from === to) continue;
    // Many member-level relations collapse onto the same pair of boxes.
    const key = `${from}->${to}`;
    if (seen.has(key)) continue;
    seen.add(key);
    connectors.push({ id: edge.id, from, to, tone: edgeTone(edge), label: edgeLabel(edge), edge });
  }
  return connectors;
}

/** Below this a package reads fine as a flat list and a module level is noise. */
const MODULE_THRESHOLD = 8;

/** `source` on a box the canvas invented; the map cannot be asked to focus it. */
const DERIVED_SOURCE = "derived-module";

export function isDerived(node: VisualNode): boolean {
  return node.source === DERIVED_SOURCE;
}

/**
 * The module level, derived from member paths.
 *
 * Old snapshots can predate the engine's MODULE contract. Grouping by the
 * first path segment below the package's common prefix reconstructs a useful
 * compatibility boundary for those snapshots:
 * `apps/api/plane/authentication/views.py` and `.../authentication/urls.py`
 * become one `authentication` module.
 *
 * Current snapshots use the real parentId/depth groups and never enter here.
 */
export function deriveModules(
  areaId: string,
  members: VisualNode[],
): { modules: VisualNode[]; membersOf: Map<string, VisualNode[]> } {
  const empty = { modules: [], membersOf: new Map<string, VisualNode[]>() };
  if (members.length < MODULE_THRESHOLD) return empty;

  const segmentsOf = (node: VisualNode): string[] => {
    const path = node.location?.path;
    if (!path) return [];
    const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
    return parts.slice(0, -1); // drop the file name
  };

  const all = members.map(segmentsOf);
  if (all.filter((parts) => parts.length > 0).length < MODULE_THRESHOLD) return empty;

  // Strip what every member shares; what differs first is the module boundary.
  const withPath = all.filter((parts) => parts.length > 0);
  let common = 0;
  for (;;) {
    const head = withPath[0]?.[common];
    if (head === undefined) break;
    if (!withPath.every((parts) => parts[common] === head)) break;
    common += 1;
  }

  const membersOf = new Map<string, VisualNode[]>();
  const titles = new Map<string, string>();
  members.forEach((member, index) => {
    const parts = all[index];
    const key = parts[common] ?? "root";
    const id = `module:${areaId}:${key}`;
    titles.set(id, key);
    const bucket = membersOf.get(id);
    if (bucket) bucket.push(member);
    else membersOf.set(id, [member]);
  });

  // One bucket means the split found nothing; two levels saying the same thing
  // is worse than one.
  if (membersOf.size < 2) return empty;

  const modules = [...membersOf.entries()].map(([id, owned]) => {
    const prefix = [...withPath[0].slice(0, common), titles.get(id) ?? ""].filter(Boolean);
    const api = owned.filter((node) => normalizeLayer(node) === "api").length;
    const db = owned.filter((node) => normalizeLayer(node) === "db").length;
    return {
      id,
      kind: "MODULE",
      title: titles.get(id) ?? "root",
      layer: "mixed",
      // Marks a box the map has never heard of. Asking the map to focus a
      // derived id made it drop its focus, which collapsed the canvas.
      source: DERIVED_SOURCE,
      subtitle: `API ${api} · 코드 ${owned.length - api - db} · DB ${db}`,
      location: { path: prefix.join("/"), line: null },
    } satisfies VisualNode;
  });

  modules.sort((left, right) => {
    const size = (membersOf.get(right.id)?.length ?? 0) - (membersOf.get(left.id)?.length ?? 0);
    return size !== 0 ? size : left.title.localeCompare(right.title, "ko-KR");
  });
  return { modules, membersOf };
}

function isGroup(node: VisualNode): boolean {
  return node.kind === "group-domain" || node.id.startsWith("group:") || node.kind === "MODULE";
}

function kindLabelOf(node: VisualNode, depth: number): string {
  if (isGroup(node)) return depth === 0 ? "패키지" : "모듈";
  const tone = toneOf(node, depth);
  if (tone === "api") return "API";
  if (tone === "db") return "DB";
  if (tone === "external") return "외부";
  return "코드";
}

function toneOf(node: VisualNode, depth = 0): FlowTone {
  if (isGroup(node)) return depth === 0 ? "package" : "module";
  if (node.kind === "external") return "external";
  const layer = normalizeLayer(node);
  if (layer === "db") return "db";
  if (layer === "api") return "api";
  return "code";
}

/**
 * One spelling for each lane.
 *
 * `layer` arrives as "db", "database" or "data" depending on which part of the
 * pipeline produced the node, and consumers each checked a different subset —
 * a node marked "db" only reached the data lane through the `kind` fallback.
 */
function normalizeLayer(node: VisualNode): "api" | "code" | "db" {
  const layer = (node.layer ?? "").toLowerCase();
  if (layer === "db" || layer === "database" || layer === "data") return "db";
  if (node.source === "db" || node.kind === "table" || node.kind === "column") return "db";
  if (layer === "api") return "api";
  if (node.kind === "route" || node.kind === "endpoint") return "api";
  return "code";
}

function metaOf(node: VisualNode): string {
  if (node.location?.path) {
    return `${node.location.path}${node.location.line ? `:${node.location.line}` : ""}`;
  }
  const { samples, counts } = subtitleParts(node.subtitle);
  return samples.length > 0 ? samples.join(" · ") : (counts ?? "");
}

function metricOf(node: VisualNode): string {
  const metrics = node.metrics;
  if (metrics) return `API ${metrics.apiCount} · 코드 ${metrics.codeCount} · DB ${metrics.dbCount}`;
  const { counts } = subtitleParts(node.subtitle);
  return counts?.startsWith("API ") ? counts : "";
}

/**
 * An area node's `subtitle` packs its counts and three representative-item
 * lists into one string separated by "|". It is transport, never display text.
 */
function subtitleParts(subtitle?: string | null): { counts: string | null; samples: string[] } {
  if (!subtitle) return { counts: null, samples: [] };
  const [counts = "", ...rest] = subtitle.split("|");
  return { counts: counts.trim() || null, samples: rest.map((part) => part.trim()).filter(Boolean) };
}

function edgeTone(edge: VisualEdge): string {
  const kind = edge.kind ?? "";
  if (kind.startsWith("candidate")) return "candidate";
  if (edge.evidence?.length) return "confirmed";
  return "inferred";
}

function edgeLabel(edge: VisualEdge): string {
  if (edge.kind === "code_handle") return "HANDLES";
  if (edge.kind === "code_call") return "CALLS";
  return edge.kind || "관계";
}
