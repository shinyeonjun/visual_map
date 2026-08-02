import { describe, expect, it } from "vitest";
import type { VisualMap, VisualNode } from "../../types/visual-map";
import { buildCodeImpactProjection } from "./CodeImpactMap";

function node(id: string, title: string, layer = "code", source = "code"): VisualNode {
  return { id, kind: layer === "api" ? "api" : layer === "db" ? "table" : "function", title, layer, source };
}

describe("CodeImpactMap projection", () => {
  it("keeps callers, callees, and direct DB edges in separate projections", () => {
    const map: VisualMap = {
      id: "map",
      workspaceId: "workspace",
      mode: "search-focus",
      focus: "code:selected",
      nodes: [
        node("api:route", "POST /items", "api"),
        node("code:caller", "controller"),
        node("code:selected", "service"),
        node("code:callee", "repository"),
        node("db:table:items", "items", "db", "db"),
      ],
      edges: [
        { id: "handles", from: "api:route", to: "code:selected", kind: "code_handle", evidence: [{ kind: "handles", text: "route" }] },
        { id: "calls-in", from: "code:caller", to: "code:selected", kind: "code_call", evidence: [{ kind: "calls", text: "caller" }] },
        { id: "calls-out", from: "code:selected", to: "code:callee", kind: "code_call", evidence: [{ kind: "calls", text: "callee" }] },
        { id: "reads", from: "code:selected", to: "db:table:items", kind: "code_db_read", evidence: [{ kind: "sql", text: "SELECT" }] },
      ],
      warnings: [],
    };

    const projection = buildCodeImpactProjection(map, "code:selected");

    expect(projection?.focus.title).toBe("service");
    expect(new Set(projection?.incoming.map((link) => link.node.id))).toEqual(new Set(["api:route", "code:caller"]));
    expect(projection?.outgoing.map((link) => link.node.id)).toEqual(["code:callee"]);
    expect(projection?.database.map((link) => link.node.id)).toEqual(["db:table:items"]);
  });

  it("groups multiple edges to the same node without losing their evidence", () => {
    const map: VisualMap = {
      id: "map",
      workspaceId: "workspace",
      mode: "search-focus",
      focus: "code:selected",
      nodes: [node("code:selected", "service"), node("code:caller", "controller")],
      edges: [
        { id: "call-1", from: "code:caller", to: "code:selected", kind: "code_call", evidence: [] },
        { id: "call-2", from: "code:caller", to: "code:selected", kind: "code_call", evidence: [{ kind: "call", text: "second" }] },
      ],
      warnings: [],
    };

    const link = buildCodeImpactProjection(map, "code:selected")?.incoming[0];

    expect(link?.node.id).toBe("code:caller");
    expect(link?.edges.map((edge) => edge.id)).toEqual(["call-1", "call-2"]);
  });
});
