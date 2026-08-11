import { describe, expect, it } from "vitest";
import { buildTraceGraph } from "./traceGraph";
import type { DispatchKind, MapNode, MapTrace, MapTraceHop, NodeRole, TraceState } from "./types";

describe("buildTraceGraph", () => {
  it("draws a shared beginning once instead of once per path", () => {
    const graph = buildTraceGraph([
      trace("t1", "complete", ["POST /orders:endpoint", "Controller:controller", "OrderService:service"]),
      trace("t2", "complete", ["POST /orders:endpoint", "Controller:controller", "StockService:service"]),
    ]);

    // Two paths, five steps written, four drawn: the entrypoint and the
    // controller are the same route in both.
    expect(graph.nodes).toHaveLength(4);
    expect(graph.nodes.filter((item) => item.column === 0)).toHaveLength(1);
    expect(graph.nodes.filter((item) => item.column === 2)).toHaveLength(2);
  });

  it("keeps the shared trunk flat and sends the branch below it", () => {
    const graph = buildTraceGraph([
      trace("t1", "complete", ["A:endpoint", "B:controller", "C:service"]),
      trace("t2", "complete", ["A:endpoint", "B:controller", "D:service"]),
    ]);

    expect(laneOf(graph, "A")).toBe(0);
    expect(laneOf(graph, "B")).toBe(0);
    expect(laneOf(graph, "C")).toBe(0);
    expect(laneOf(graph, "D")).toBe(1);
    expect(graph.laneCount).toBe(2);
  });

  it("never merges two different routes that happen to reach the same step", () => {
    const graph = buildTraceGraph([
      trace("t1", "complete", ["A:endpoint", "Shared:service"]),
      trace("t2", "complete", ["B:endpoint", "Shared:service"]),
    ]);

    // Reaching `Shared` by way of A is not the same fact as reaching it by
    // way of B, so collapsing them would invent a route neither path took.
    const shared = graph.nodes.filter((item) => item.node.id === "Shared");
    expect(shared).toHaveLength(2);
    expect(graph.edges).toHaveLength(2);
  });

  it("records which paths run over each step and each line", () => {
    const graph = buildTraceGraph([
      trace("t1", "complete", ["A:endpoint", "B:service"]),
      trace("t2", "complete", ["A:endpoint", "B:service"]),
    ]);

    expect(graph.nodes[0].traceIds).toEqual(["t1", "t2"]);
    expect(graph.edges[0].traceIds).toEqual(["t1", "t2"]);
  });

  it("hangs a visible stub where a walk stopped early", () => {
    const graph = buildTraceGraph([trace("t1", "gap", ["A:endpoint", "B:service"])]);

    // A path that hit an analysis gap must not read as one that finished.
    expect(graph.terminals).toEqual([
      { key: expect.any(String), nodeKey: expect.any(String), state: "gap", column: 2, lane: 0 },
    ]);
  });

  it("draws no stub for a path that reached its end", () => {
    const graph = buildTraceGraph([trace("t1", "complete", ["A:endpoint", "B:service"])]);
    expect(graph.terminals).toEqual([]);
  });

  it("gives a stub its own lane when other paths continue through that step", () => {
    const graph = buildTraceGraph([
      trace("t1", "complete", ["A:endpoint", "B:service", "C:repository"]),
      trace("t2", "depth-limited", ["A:endpoint", "B:service"]),
    ]);

    const [terminal] = graph.terminals;
    // Sitting on B's own lane would lay the broken stub straight over the
    // confirmed line running B → C.
    expect(terminal.lane).not.toBe(laneOf(graph, "B"));
    expect(graph.laneCount).toBeGreaterThan(1);
  });

  it("attaches each hop to the move it describes", () => {
    const graph = buildTraceGraph([
      {
        id: "t1",
        state: "complete",
        steps: [step("A:endpoint"), step("B:controller"), step("C:service")],
        // One shorter than the steps: hop[0] is A→B, hop[1] is B→C.
        hops: [hop("h-ab", "direct"), hop("h-bc", "dynamic")],
      },
    ]);

    // Resolved through the nodes rather than by picking the key apart: how a
    // key is encoded is this module's business, not the test's.
    const stepOf = new Map(graph.nodes.map((item) => [item.key, item.node.id]));
    const byTarget = new Map(graph.edges.map((edge) => [stepOf.get(edge.toKey), edge.hop?.id]));
    expect(byTarget.get("B")).toBe("h-ab");
    expect(byTarget.get("C")).toBe("h-bc");
  });

  it("carries dispatch through so a dynamic call cannot be drawn as a certain one", () => {
    const graph = buildTraceGraph([
      { id: "t1", state: "complete", steps: [step("A:endpoint"), step("B:service")], hops: [hop("h", "dynamic")] },
    ]);
    expect(graph.edges[0].hop?.dispatch).toBe("dynamic");
  });

  it("still builds when the engine published a path without hop detail", () => {
    const graph = buildTraceGraph([trace("t1", "complete", ["A:endpoint", "B:service"])]);
    expect(graph.edges[0].hop).toBeNull();
    expect(graph.nodes).toHaveLength(2);
  });

  it("names a column only when every path agrees what it is", () => {
    const graph = buildTraceGraph([
      trace("t1", "complete", ["A:endpoint", "B:service"]),
      trace("t2", "complete", ["C:endpoint", "D:repository"]),
    ]);

    expect(graph.columns).toEqual([
      { index: 0, role: "endpoint" },
      { index: 1, role: null },
    ]);
  });

  it("is stable and empty-safe", () => {
    expect(buildTraceGraph([])).toMatchObject({ nodes: [], edges: [], terminals: [], columnCount: 0, laneCount: 0 });

    const twice = [0, 1].map(() =>
      JSON.stringify(
        buildTraceGraph([
          trace("t1", "complete", ["A:endpoint", "B:service", "C:repository"]),
          trace("t2", "gap", ["A:endpoint", "B:service", "D:service"]),
        ]),
      ),
    );
    expect(twice[0]).toBe(twice[1]);
  });
});

function laneOf(graph: ReturnType<typeof buildTraceGraph>, nodeId: string): number {
  const found = graph.nodes.find((item) => item.node.id === nodeId);
  if (!found) throw new Error(`${nodeId} is not on the graph`);
  return found.lane;
}

/** `"name:role"` per step, so a path reads as one line in the test. */
function trace(id: string, state: TraceState, steps: string[]): MapTrace {
  return { id, state, steps: steps.map(step) };
}

function hop(id: string, dispatch: DispatchKind): MapTraceHop {
  return { id, from: "", to: "", kind: "Calls", truth: "verified", dispatch, evidence: [], execution: null };
}

function step(spec: string): MapNode {
  const [name, role] = spec.split(":");
  return { id: name, name, kind: role, role: role as NodeRole, definition: null };
}

describe("node definition", () => {
  it("carries the exact definition through to the drawn step", () => {
    const defined: MapNode = {
      id: "s1",
      name: "OrderService.create",
      kind: "Method",
      role: "service",
      definition: { path: "src/orders/order.service.ts", line: 87 },
    };
    const graph = buildTraceGraph([{ id: "t1", state: "complete", steps: [step("A:endpoint"), defined] }]);

    // The step's own location, never the line the caller sits on.
    expect(graph.nodes[1].node.definition).toEqual({ path: "src/orders/order.service.ts", line: 87 });
  });
});
