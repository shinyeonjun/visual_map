import { describe, expect, it } from "vitest";
import type { ApiReadingAnswer, ApiReadingStep, VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import type { ApiConnectionModel } from "./apiConnectionModel";
import { buildApiGraphLayout } from "./apiReadingGraph";

function node(id: string): VisualNode {
  return { id, kind: "function", title: id, layer: "code", source: "code" };
}

function step(id: string, lane: ApiReadingStep["lane"]): ApiReadingStep {
  return {
    id,
    nodeId: id,
    kind: lane,
    title: id,
    detail: id,
    truthClass: "confirmed",
    confidence: "high",
    rank: 1,
    evidence: [],
    depth: 0,
    lane,
    laneBasis: "engine-node",
    incomingEvidence: [],
  };
}

function edge(id: string, from: string, to: string): VisualEdge {
  return { id, from, to, kind: "code_call", confidence: "high", evidence: [] };
}

function answer(steps: ApiReadingStep[]): ApiReadingAnswer {
  return {
    subject: "GET /items",
    method: "GET",
    steps,
    dbCandidates: [],
    unknowns: [],
    recommendedChecks: [],
    hiddenBranches: 0,
    truncated: false,
  };
}

describe("API reading graph layout", () => {
  it("separates same-depth branches so their cards do not overlap", () => {
    const route = step("route", "route");
    const branchA = step("branch-a", "service-function");
    const branchB = step("branch-b", "service-function");
    const map: VisualMap = {
      id: "map",
      workspaceId: "workspace",
      mode: "api-flow",
      focus: "route",
      nodes: [node("route"), node("branch-a"), node("branch-b")],
      edges: [edge("route-a", "route", "branch-a"), edge("route-b", "route", "branch-b")],
      warnings: [],
    };
    const model: ApiConnectionModel = {
      primaryPath: [{ item: route, node: node("route") }],
      primaryEdges: [],
      primaryDatabase: null,
      additionalEdges: map.edges,
      collapsedEdges: [],
      gap: null,
    };

    const layout = buildApiGraphLayout(answer([route, branchA, branchB]), map, model);
    const branches = layout.nodes.filter(({ node: item }) => item.id !== "route");
    expect(branches).toHaveLength(2);
    expect(branches[0].y).toBe(branches[1].y);
    expect(Math.abs(branches[0].x - branches[1].x)).toBeGreaterThanOrEqual(156 + 48);
  });
});
