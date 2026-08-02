import { describe, expect, it } from "vitest";
import type { ApiReadingAnswer, ApiReadingStep, VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import type { ApiConnectionModel } from "./apiConnectionModel";
import {
  API_GRAPH_NODE_GAP,
  API_GRAPH_NODE_HEIGHT,
  API_GRAPH_NODE_WIDTH,
  apiGraphEdgeAnchors,
  apiGraphEdgePath,
  buildApiGraphLayout,
} from "./apiReadingGraph";

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
    expect(Math.abs(branches[0].x - branches[1].x)).toBeGreaterThanOrEqual(API_GRAPH_NODE_WIDTH + API_GRAPH_NODE_GAP);
  });

  it("keeps every depth row collision-free when branches continue from different parents", () => {
    const route = step("route", "route");
    const handler = step("handler", "handler");
    const left = step("left", "service-function");
    const right = step("right", "service-function");
    const leftNext = step("left-next", "repository-query");
    const rightNext = step("right-next", "repository-query");
    const map: VisualMap = {
      id: "map",
      workspaceId: "workspace",
      mode: "api-flow",
      focus: "route",
      nodes: [route, handler, left, right, leftNext, rightNext].map(({ id }) => node(id)),
      edges: [
        edge("route-handler", "route", "handler"),
        edge("handler-left", "handler", "left"),
        edge("handler-right", "handler", "right"),
        edge("left-next", "left", "left-next"),
        edge("right-next", "right", "right-next"),
      ],
      warnings: [],
    };
    const model: ApiConnectionModel = {
      primaryPath: [{ item: route, node: node("route") }, { item: handler, node: node("handler") }],
      primaryEdges: [edge("route-handler", "route", "handler")],
      primaryDatabase: null,
      additionalEdges: map.edges.slice(1),
      collapsedEdges: [],
      gap: null,
    };

    const layout = buildApiGraphLayout(answer([route, handler, left, right, leftNext, rightNext]), map, model);
    const rows = new Map<number, Array<{ x: number; y: number }>>();
    for (const item of layout.nodes) {
      const row = rows.get(item.y) ?? [];
      row.push(item);
      rows.set(item.y, row);
    }
    for (const row of rows.values()) {
      const ordered = row.sort((leftItem, rightItem) => leftItem.x - rightItem.x);
      for (let index = 1; index < ordered.length; index += 1) {
        expect(ordered[index].x - ordered[index - 1].x).toBeGreaterThanOrEqual(API_GRAPH_NODE_WIDTH + API_GRAPH_NODE_GAP);
      }
    }
  });

  it("anchors every edge on a card boundary instead of entering card content", () => {
    const route = step("route", "route");
    const handler = step("handler", "handler");
    const service = step("service", "service-function");
    const map: VisualMap = {
      id: "map",
      workspaceId: "workspace",
      mode: "api-flow",
      focus: "route",
      nodes: [route, handler, service].map(({ id }) => node(id)),
      edges: [edge("route-handler", "route", "handler"), edge("handler-service", "handler", "service")],
      warnings: [],
    };
    const model: ApiConnectionModel = {
      primaryPath: [{ item: route, node: node("route") }, { item: handler, node: node("handler") }],
      primaryEdges: [edge("route-handler", "route", "handler")],
      primaryDatabase: null,
      additionalEdges: [edge("handler-service", "handler", "service")],
      collapsedEdges: [],
      gap: null,
    };

    const layout = buildApiGraphLayout(answer([route, handler, service]), map, model);
    for (const connection of layout.edges) {
      const anchors = apiGraphEdgeAnchors(connection, null);
      const path = apiGraphEdgePath(connection, null);
      expect(path.startsWith(`M ${anchors.startX} ${anchors.startY}`)).toBe(true);
      expect(path.endsWith(`${anchors.endX} ${anchors.endY}`)).toBe(true);
      if (anchors.kind === "cross-row") {
        expect(anchors.startY).toBe(connection.from.y + API_GRAPH_NODE_HEIGHT);
        expect(anchors.endY).toBe(connection.to.y);
      } else if (anchors.kind === "same-row-arc") {
        expect(anchors.startY).toBe(connection.from.y + API_GRAPH_NODE_HEIGHT);
        expect(anchors.endY).toBe(connection.to.y + API_GRAPH_NODE_HEIGHT);
      } else {
        expect(anchors.startY).toBe(connection.from.y + API_GRAPH_NODE_HEIGHT / 2);
        expect(anchors.endY).toBe(connection.to.y + API_GRAPH_NODE_HEIGHT / 2);
      }
    }
  });
});
