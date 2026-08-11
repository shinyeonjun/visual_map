import { describe, expect, it } from "vitest";
import { MAP_DETAIL_POLICY, detailForScale } from "./detail";
import { relationsTouching } from "./types";
import type { MapArea, MapView } from "./types";

describe("detailForScale", () => {
  it("gives a fitted repository map a readable middle tier", () => {
    // Fitting a dozen areas lands here, and it used to mean "draw everything"
    // at a size where none of it could be read.
    expect(detailForScale(0.57)).toBe("outline");
    expect(detailForScale(0.4)).toBe("outline");
    expect(detailForScale(0.74)).toBe("outline");
  });

  it("falls back to blocks when far out and shows everything when close", () => {
    expect(detailForScale(0.05)).toBe("overview");
    expect(detailForScale(0.39)).toBe("overview");
    expect(detailForScale(0.89)).toBe("outline");
    expect(detailForScale(0.9)).toBe("full");
    expect(detailForScale(2.2)).toBe("full");
  });

  it("never drops something a wider tier already showed", () => {
    const order = ["overview", "outline", "full"] as const;
    const keys = ["summary", "subareas", "members"] as const;
    for (const key of keys) {
      const shown = order.map((tier) => MAP_DETAIL_POLICY[tier][key]);
      expect(shown, `${key} must only ever turn on as the reader zooms in`).toEqual([...shown].sort());
    }
  });
});

describe("relationsTouching", () => {
  const view = fixture();

  it("answers for nothing when nothing is pointed at", () => {
    expect(relationsTouching(view, null)).toBeNull();
  });

  it("keeps only the relations that reach the pointed-at area", () => {
    expect(relationsTouching(view, "area-a")).toEqual(new Set(["rel-ab"]));
    expect(relationsTouching(view, "area-c")).toEqual(new Set(["rel-bc"]));
  });

  it("resolves a nested area or a single member up to its owning area", () => {
    // The reader clicks what they can see; relations only run between the
    // top-level areas, so a member click has to answer for its owner.
    expect(relationsTouching(view, "child-b")).toEqual(new Set(["rel-ab", "rel-bc"]));
    expect(relationsTouching(view, "node-b1")).toEqual(new Set(["rel-ab", "rel-bc"]));
  });

  it("treats an id the map does not hold as pointing at nothing", () => {
    expect(relationsTouching(view, "not-on-the-map")).toBeNull();
  });
});

function fixture(): MapView {
  return {
    areas: [
      area("area-a", []),
      { ...area("area-b", []), areas: [{ ...area("child-b", ["node-b1"]), depth: 1 }] },
      area("area-c", []),
    ],
    relations: [
      { id: "rel-ab", from: "area-a", to: "area-b", truth: "verified", label: "호출", count: 3 },
      { id: "rel-bc", from: "area-b", to: "area-c", truth: "structural", label: "구조", count: 1 },
    ],
    unattributedAnalysisGapCount: 0,
  };
}

function area(id: string, nodeIds: string[]): MapArea {
  return {
    id,
    name: id,
    summary: "",
    category: "domain",
    labelSource: "semantic",
    fallbackReason: null,
    depth: 0,
    areas: [],
    nodes: nodeIds.map((nodeId) => ({
      id: nodeId,
      name: nodeId,
      kind: "Function",
      role: "code" as const,
      definition: null,
    })),
    hiddenNodeCount: 0,
    boundaryRelationCounts: { verified: 0, structural: 0, candidate: 0 },
    affectingAnalysisGapCount: 0,
  };
}
