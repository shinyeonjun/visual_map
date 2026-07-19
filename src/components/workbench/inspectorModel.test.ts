import { describe, expect, it } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  edgeTrustLabel,
  edgeTrustTone,
  nodeEvidenceSummary,
} from "./inspectorModel";

const node: VisualNode = {
  id: "db:table:public.orders",
  kind: "table",
  title: "orders",
  subtitle: "public",
  layer: "data",
  source: "db",
};

function mapWith(edge: VisualEdge): VisualMap {
  return {
    id: "map",
    workspaceId: "workspace",
    mode: "table-usage",
    focus: node.id,
    nodes: [node],
    edges: [edge],
    warnings: [],
  };
}

describe("inspector trust model", () => {
  it("never presents a strong name match as a confirmed relationship", () => {
    const candidate: VisualEdge = {
      id: "candidate",
      from: "code:repository",
      to: node.id,
      kind: "candidate_table",
      confidence: "high",
      evidence: [{ kind: "name-match", text: "orders identifier match" }],
    };

    expect(edgeTrustLabel(candidate)).toBe("후보 단서 강함");
    expect(edgeTrustTone(candidate)).toBe("amber");
    expect(nodeEvidenceSummary(node, mapWith(candidate))).toMatchObject({
      confidence: "후보 단서 강함",
      badgeTone: "amber",
      connectionSummary: "직접 0 · 구조 0 · 후보 1 · 이름 단서 0",
    });
  });

  it("labels containment without evidence as structural, not direct", () => {
    const structural: VisualEdge = {
      id: "structural",
      from: "group:package:app",
      to: node.id,
      kind: "group_contains",
      confidence: null,
      evidence: [],
    };

    expect(edgeTrustLabel(structural)).toBe("구조 근거");
    expect(edgeTrustTone(structural)).toBe("gray");
  });
});
