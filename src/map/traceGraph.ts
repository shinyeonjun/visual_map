/**
 * The shape a set of execution paths makes when they are drawn together.
 *
 * The engine publishes each `TracePath` as its own ordered list of steps, and
 * several paths out of one entrypoint usually begin identically before they
 * diverge. Drawing them as separate rows repeats that shared beginning once
 * per path; merging them on their common prefix draws it once and lets the
 * differences be the thing that shows. That is the whole trick behind a line
 * map: one trunk, branches where the paths actually part.
 *
 * Merging is by *prefix*, never by name alone. Two paths collapse only while
 * every step so far has been identical, so this can never claim that two
 * different routes through the code are the same route.
 *
 * Nothing here invents a step or an order. Position comes from the index the
 * engine already assigned, and a walk that ended without reaching the end
 * becomes a visible stub rather than a line that quietly continues.
 */

import type { MapNode, MapTrace, MapTraceHop, NodeRole, TraceState } from "./types";

/** How a walk ended when it did not reach the end of the path. */
export type UnfinishedTraceState = Exclude<TraceState, "complete">;

export interface TraceGraphNode {
  /** Identifies the step *and* the route taken to reach it. */
  key: string;
  node: MapNode;
  column: number;
  lane: number;
  traceIds: string[];
}

export interface TraceGraphEdge {
  key: string;
  fromKey: string;
  toKey: string;
  traceIds: string[];
  /*
    The engine's record of this exact move: how sure it is of the target, where
    the call is written, and whether the source guards or repeats it. Absent
    when the path was published without per-hop detail.
  */
  hop: MapTraceHop | null;
}

/** A walk that stopped early, hung off the last step it did confirm. */
export interface TraceGraphTerminal {
  key: string;
  nodeKey: string;
  state: UnfinishedTraceState;
  column: number;
  lane: number;
}

export interface TraceGraphColumn {
  index: number;
  /** Null when the paths disagree about what this position is. */
  role: NodeRole | null;
}

export interface TraceGraph {
  nodes: TraceGraphNode[];
  edges: TraceGraphEdge[];
  terminals: TraceGraphTerminal[];
  columns: TraceGraphColumn[];
  columnCount: number;
  laneCount: number;
}

/** Separates path components in a key; forbidden inside an engine id. */
const KEY_SEPARATOR = "\0";

interface MutableNode extends TraceGraphNode {
  lane: number;
}

export function buildTraceGraph(traces: MapTrace[]): TraceGraph {
  const nodes = new Map<string, MutableNode>();
  const edges = new Map<string, TraceGraphEdge>();
  const children = new Map<string, string[]>();
  const roots: string[] = [];
  const unfinished = new Map<string, UnfinishedTraceState>();

  for (const trace of traces) {
    let parentKey: string | null = null;
    let key = "";
    for (const [column, node] of trace.steps.entries()) {
      key = key.length > 0 ? `${key}${KEY_SEPARATOR}${node.id}` : node.id;
      const existing = nodes.get(key);
      if (existing) {
        addTrace(existing.traceIds, trace.id);
      } else {
        nodes.set(key, { key, node, column, lane: 0, traceIds: [trace.id] });
        if (parentKey === null) roots.push(key);
        else childListOf(children, parentKey).push(key);
      }
      if (parentKey !== null) {
        const edgeKey = `${parentKey}${KEY_SEPARATOR}>${key}`;
        const edge = edges.get(edgeKey);
        // Hops are indexed by the move they describe, so the hop for the step
        // arriving at `column` is the one at `column - 1`.
        const hop = trace.hops?.[column - 1] ?? null;
        if (edge) {
          addTrace(edge.traceIds, trace.id);
          edge.hop = edge.hop ?? hop;
        } else {
          edges.set(edgeKey, { key: edgeKey, fromKey: parentKey, toKey: key, traceIds: [trace.id], hop });
        }
      }
      parentKey = key;
    }
    // One stub per end, however many walks stopped in the same place.
    if (parentKey !== null && trace.state !== "complete") unfinished.set(parentKey, trace.state);
  }

  const laneCount = assignLanes(nodes, children, roots);
  const terminals = placeTerminals(nodes, children, unfinished, laneCount);

  const ordered = [...nodes.values()].sort(byColumnThenLane);
  return {
    nodes: ordered,
    edges: [...edges.values()],
    terminals,
    columns: describeColumns(ordered),
    columnCount: ordered.reduce((max, item) => Math.max(max, item.column + 1), 0),
    laneCount: Math.max(laneCount + terminals.filter(needsOwnLane(children)).length, ordered.length > 0 ? 1 : 0),
  };
}

/**
 * Straight trunk, branches below it.
 *
 * A step keeps the lane of its first continuation, so the busiest route runs
 * flat across the map and every divergence reads as a departure from it.
 */
function assignLanes(nodes: Map<string, MutableNode>, children: Map<string, string[]>, roots: string[]): number {
  let nextLane = 0;
  const walk = (key: string): number => {
    const item = nodes.get(key);
    if (!item) return 0;
    const kids = children.get(key) ?? [];
    if (kids.length === 0) {
      item.lane = nextLane++;
      return item.lane;
    }
    const lanes = kids.map(walk);
    item.lane = lanes[0];
    return item.lane;
  };
  for (const root of roots) walk(root);
  return nextLane;
}

function placeTerminals(
  nodes: Map<string, MutableNode>,
  children: Map<string, string[]>,
  unfinished: Map<string, UnfinishedTraceState>,
  laneCount: number,
): TraceGraphTerminal[] {
  let spare = laneCount;
  const terminals: TraceGraphTerminal[] = [];
  for (const [nodeKey, state] of unfinished) {
    const owner = nodes.get(nodeKey);
    if (!owner) continue;
    // A step other walks continue through cannot host the stub on its own
    // lane without the stub sitting on top of a confirmed line.
    const branching = (children.get(nodeKey) ?? []).length > 0;
    terminals.push({
      key: `${nodeKey}${KEY_SEPARATOR}~${state}`,
      nodeKey,
      state,
      column: owner.column + 1,
      lane: branching ? spare++ : owner.lane,
    });
  }
  return terminals;
}

function needsOwnLane(children: Map<string, string[]>) {
  return (terminal: TraceGraphTerminal) => (children.get(terminal.nodeKey) ?? []).length > 0;
}

/**
 * What each position along the paths is.
 *
 * The engine classifies every step, so a column can be named when the paths
 * agree about it. They will not always agree — one route can reach a service
 * in three hops where another takes four — and an invented name for a mixed
 * column would be a claim the facts do not support.
 */
function describeColumns(nodes: TraceGraphNode[]): TraceGraphColumn[] {
  const roles = new Map<number, Set<NodeRole>>();
  for (const item of nodes) {
    const seen = roles.get(item.column) ?? new Set<NodeRole>();
    seen.add(item.node.role);
    roles.set(item.column, seen);
  }
  return [...roles.entries()]
    .sort(([left], [right]) => left - right)
    .map(([index, seen]) => ({ index, role: seen.size === 1 ? [...seen][0] : null }));
}

function childListOf(children: Map<string, string[]>, key: string): string[] {
  const existing = children.get(key);
  if (existing) return existing;
  const created: string[] = [];
  children.set(key, created);
  return created;
}

function addTrace(traceIds: string[], id: string) {
  if (!traceIds.includes(id)) traceIds.push(id);
}

function byColumnThenLane(left: TraceGraphNode, right: TraceGraphNode): number {
  return left.column - right.column || left.lane - right.lane;
}
