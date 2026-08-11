import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AreaBox } from "./AreaBox";
import type { AreaCategory, MapArea } from "./types";

function area(overrides: Partial<MapArea> = {}): MapArea {
  return {
    id: "area-0",
    name: "주문",
    summary: "주문 생성과 조회",
    category: "domain",
    labelSource: "semantic",
    fallbackReason: null,
    depth: 0,
    areas: [],
    nodes: [],
    hiddenNodeCount: 0,
    boundaryRelationCounts: { verified: 12, structural: 3, candidate: 1 },
    affectingAnalysisGapCount: 0,
    ...overrides,
  };
}

function draw(overrides: Partial<MapArea> = {}) {
  return render(
    <AreaBox
      area={area(overrides)}
      fallbackPosition={{ x: 0, y: 0 }}
      detail="outline"
      selectedId={null}
      onSelect={() => undefined}
      onHover={() => undefined}
      onOpenTrace={() => undefined}
    />,
  );
}

describe("AreaBox", () => {
  it("names the category in words, never by colour alone", () => {
    // Only two category hues survived validation against the truth palette,
    // so the text is what actually distinguishes all five.
    const cases: Array<[AreaCategory, string]> = [
      ["domain", "도메인"],
      ["integration", "연동"],
      ["shared", "공통"],
      ["infrastructure", "인프라"],
      ["structural", "구조"],
    ];
    for (const [category, label] of cases) {
      const { container, unmount } = draw({ category });
      expect(screen.getByText(label)).toBeInTheDocument();
      expect(container.querySelector(`.category-${category}`)).not.toBeNull();
      unmount();
    }
  });

  it("reports boundary relations split by how far the engine got", () => {
    const { container } = draw();
    const boundary = container.querySelector(".map-area-boundary");

    expect(boundary?.querySelector("b.verified")?.textContent).toBe("12");
    expect(boundary?.querySelector("b.structural")?.textContent).toBe("3");
    expect(boundary?.querySelector("b.candidate")?.textContent).toBe("1");
  });

  it("shows analysis gaps when there are any and stays quiet when there are none", () => {
    expect(draw({ affectingAnalysisGapCount: 4 }).container.querySelector(".map-area-boundary em")?.textContent).toBe(
      "공백 4",
    );
    expect(draw({ affectingAnalysisGapCount: 0 }).container.querySelector(".map-area-boundary em")).toBeNull();
  });

  it("marks a name the analysis copied instead of derived, and says why", () => {
    const { container } = draw({ labelSource: "structural", fallbackReason: "insufficient-semantic-signal" });
    const name = container.querySelector(".map-area-name");

    // Reading a copied label the same as an evidence-derived one is how a
    // reader ends up trusting a name nothing proved.
    expect(name).toHaveClass("structural");
    expect(name).toHaveAttribute("title", expect.stringContaining("근거가 부족해"));
  });

  it("leaves a derived name unmarked", () => {
    const name = draw({ labelSource: "semantic", fallbackReason: null }).container.querySelector(".map-area-name");
    expect(name).not.toHaveClass("structural");
    expect(name).not.toHaveAttribute("title");
  });

  it("keeps the member count apart from the boundary count", () => {
    // One says how big the area is; the other says how much of its edge is
    // known. Folding them into a single number would answer neither.
    const { container } = draw({
      nodes: [{ id: "n1", name: "createOrder", kind: "Method", role: "service", definition: null }],
      hiddenNodeCount: 40,
    });
    expect(container.querySelector(".map-area-boundary")?.textContent).not.toContain("41");
  });
});
