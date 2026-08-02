import { describe, expect, it } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  buildCodeCallGraphLayout,
  buildCodeCallGraphModel,
  CALL_NODE_HEIGHT,
  CALL_NODE_WIDTH,
  callGraphDataPath,
  callGraphSidePath,
} from "./codeCallGraphModel";

function node(id: string, overrides: Partial<VisualNode> = {}): VisualNode {
  return { id, kind: "function", title: id, layer: "code", source: "code", ...overrides };
}

function edge(id: string, from: string, to: string, kind = "code_call"): VisualEdge {
  return { id, from, to, kind, confidence: "high", evidence: [{ kind: "engine", text: id }] };
}

function mapWith(nodes: VisualNode[], edges: VisualEdge[]): VisualMap {
  return { id: "map", workspaceId: "w", mode: "search-focus", focus: "focus", nodes, edges, warnings: [] };
}

describe("code call graph model", () => {
  it("splits callers, callees, and data targets around the focus", () => {
    const map = mapWith(
      [
        node("focus"),
        node("caller-route", { kind: "route", layer: "api" }),
        node("callee-repo", { kind: "repository" }),
        node("orders", { kind: "table", layer: "db" }),
      ],
      [
        edge("in", "caller-route", "focus", "code_handle"),
        edge("out", "focus", "callee-repo"),
        edge("db", "focus", "orders", "code_db_read"),
      ],
    );

    const model = buildCodeCallGraphModel("focus", map)!;
    expect(model.callers.map(({ node: item }) => item.id)).toEqual(["caller-route"]);
    expect(model.callees.map(({ node: item }) => item.id)).toEqual(["callee-repo"]);
    expect(model.dataTargets.map(({ node: item }) => item.id)).toEqual(["orders"]);
    expect(model.totalConnections).toBe(3);
  });

  it("returns null when the focus has no connections", () => {
    expect(buildCodeCallGraphModel("focus", mapWith([node("focus")], []))).toBeNull();
  });

  it("routes a candidate table edge to the data row, not the callee column", () => {
    const map = mapWith(
      [node("focus"), node("db:table:orders", { kind: "table", layer: "database" })],
      [edge("cand", "focus", "db:table:orders", "candidate_table")],
    );
    const model = buildCodeCallGraphModel("focus", map)!;
    expect(model.callees).toHaveLength(0);
    expect(model.dataTargets).toHaveLength(1);
  });

  it("caps each side and reports hidden counts", () => {
    const callers = Array.from({ length: 8 }, (_, index) => node(`caller-${index}`));
    const map = mapWith(
      [node("focus"), ...callers],
      callers.map((caller, index) => edge(`in-${index}`, caller.id, "focus")),
    );
    const model = buildCodeCallGraphModel("focus", map)!;
    expect(model.callers).toHaveLength(6);
    expect(model.hiddenCallers).toBe(2);
  });

  it("keeps every layout card inside the canvas without overlap", () => {
    const map = mapWith(
      [
        node("focus"),
        node("a"), node("b"), node("c"),
        node("x"), node("y"),
        node("t", { kind: "table", layer: "db" }),
      ],
      [
        edge("ia", "a", "focus"), edge("ib", "b", "focus"), edge("ic", "c", "focus"),
        edge("ox", "focus", "x"), edge("oy", "focus", "y"),
        edge("dt", "focus", "t", "code_db_write"),
      ],
    );
    const layout = buildCodeCallGraphLayout(buildCodeCallGraphModel("focus", map)!);
    const placed = [layout.focus, ...[...layout.callers, ...layout.callees, ...layout.dataTargets].map(({ x, y }) => ({ x, y }))];
    for (const { x, y } of placed) {
      expect(x).toBeGreaterThanOrEqual(0);
      expect(y).toBeGreaterThanOrEqual(0);
      expect(x + CALL_NODE_WIDTH).toBeLessThanOrEqual(layout.width);
      expect(y + CALL_NODE_HEIGHT).toBeLessThanOrEqual(layout.height);
    }
    const callerColumn = layout.callers.map(({ y }) => y).sort((left, right) => left - right);
    for (let index = 1; index < callerColumn.length; index += 1) {
      expect(callerColumn[index] - callerColumn[index - 1]).toBeGreaterThanOrEqual(CALL_NODE_HEIGHT);
    }
  });

  it("anchors side paths on card boundaries", () => {
    const map = mapWith(
      [node("focus"), node("a")],
      [edge("ia", "a", "focus")],
    );
    const layout = buildCodeCallGraphLayout(buildCodeCallGraphModel("focus", map)!);
    const path = callGraphSidePath("caller", layout.callers[0], layout.focus);
    expect(path.startsWith(`M ${layout.callers[0].x + CALL_NODE_WIDTH} `)).toBe(true);
    expect(path.endsWith(` ${layout.focus.x} ${layout.focus.y + CALL_NODE_HEIGHT / 2}`)).toBe(true);
  });

  it("drops data paths from the focus bottom into the table top", () => {
    const map = mapWith(
      [node("focus"), node("t", { kind: "table", layer: "db" })],
      [edge("dt", "focus", "t", "code_db_read")],
    );
    const layout = buildCodeCallGraphLayout(buildCodeCallGraphModel("focus", map)!);
    const target = layout.dataTargets[0];
    const path = callGraphDataPath(layout.focus, target);
    expect(path).toContain(` ${layout.focus.y + CALL_NODE_HEIGHT} `);
    expect(path.endsWith(` ${target.x + CALL_NODE_WIDTH / 2} ${target.y}`)).toBe(true);
  });
});
