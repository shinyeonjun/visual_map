import { describe, expect, it } from "vitest";
import type { ApiReadingAnswer, ApiReadingStep, VisualMap, VisualNode } from "../../types/visual-map";
import { buildApiConnectionModel } from "./apiConnectionModel";

describe("buildApiConnectionModel", () => {
  it("keeps the primary path on real confirmed edges and attaches the DB candidate to its real source", () => {
    const model = buildApiConnectionModel(answer, map);

    expect(model.primaryPath.map(({ node }) => node.id)).toEqual([
      "code:route",
      "code:handler",
      "code:service",
      "code:repository",
    ]);
    expect(model.primaryEdges.map((edge) => edge.id)).toEqual(["handles", "calls-service", "calls-repository"]);
    expect(model.primaryCandidate?.node.id).toBe("db:table:public.sessions");
    expect(model.primaryCandidate?.edge.from).toBe("code:repository");
    expect(model.additionalEdges.map((edge) => edge.id)).toEqual(["calls-side"]);
  });
});

const nodes: VisualNode[] = [
  node("code:route", "GET /api/v1/sessions", "api"),
  node("code:handler", "listSessions", "handler"),
  node("code:service", "sessionService.list", "function"),
  node("code:repository", "findActiveSessions", "function"),
  node("code:side", "auditSessionRead", "function"),
  { id: "db:table:public.sessions", kind: "table", title: "sessions", layer: "db", source: "db" },
];

const map: VisualMap = {
  id: "api-map",
  workspaceId: "workspace",
  mode: "api-flow",
  focus: "code:route",
  nodes,
  edges: [
    edge("handles", "code:route", "code:handler", "code_handle"),
    edge("calls-service", "code:handler", "code:service", "code_call"),
    edge("calls-repository", "code:service", "code:repository", "code_call"),
    edge("calls-side", "code:handler", "code:side", "code_call"),
    edge("candidate-db", "code:repository", "db:table:public.sessions", "candidate_uses"),
  ],
  warnings: [],
};

const answer: ApiReadingAnswer = {
  subject: "/api/v1/sessions",
  steps: [
    step("route", "code:route", "route", 1, 0),
    step("handler", "code:handler", "handler", 2, 1),
    step("service", "code:service", "service-function", 3, 2),
    step("repository", "code:repository", "repository-query", 4, 3),
    step("side", "code:side", "service-function", 5, 2),
  ],
  dbCandidates: [{
    id: "candidate",
    nodeId: "db:table:public.sessions",
    kind: "db-candidate",
    title: "sessions",
    detail: "repository에서 sessions 사용 가능성을 확인해야 합니다.",
    truthClass: "candidate",
    confidence: "high",
    rank: 1,
    evidence: [{ kind: "name-match", text: "sessions" }],
  }],
  unknowns: [],
  recommendedChecks: [],
  hiddenBranches: 0,
  truncated: false,
};

function node(id: string, title: string, kind: string): VisualNode {
  return { id, kind, title, layer: kind === "api" ? "api" : "code", source: "code" };
}

function edge(id: string, from: string, to: string, kind: string) {
  return { id, from, to, kind, evidence: [{ kind, text: `${from} -> ${to}` }] };
}

function step(
  id: string,
  nodeId: string,
  lane: ApiReadingStep["lane"],
  rank: number,
  depth: number,
): ApiReadingStep {
  return {
    id,
    nodeId,
    kind: "function",
    title: nodes.find((item) => item.id === nodeId)?.title ?? nodeId,
    detail: nodeId,
    truthClass: lane === "route" ? "structural" : "confirmed",
    rank,
    evidence: [],
    depth,
    lane,
    laneBasis: lane === "handler" ? "confirmed-handles" : lane === "route" ? "engine-node" : "name-inferred",
    incomingEvidence: [],
  };
}
