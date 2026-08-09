import "@testing-library/jest-dom/vitest";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { MapArea, MapView } from "./types";
import { MapCanvas } from "./MapCanvas";

function area(index: number): MapArea {
  return {
    id: `area-${index}`,
    name: `영역 ${index}`,
    summary: `${index}번째 책임 영역`,
    depth: 0,
    areas: [],
    nodes: [{ id: `node-${index}`, name: `symbol${index}`, kind: "Code", role: "code" }],
    hiddenNodeCount: 0,
    position: null,
    width: index % 7 === 0 ? 520 : 232,
  };
}

describe("large map canvas", () => {
  it("expands its world for every unplaced area instead of clipping after a fixed number of rows", () => {
    const view: MapView = { areas: Array.from({ length: 40 }, (_, index) => area(index)), relations: [] };
    const { container } = render(<MapCanvas view={view} selectedId={null} onSelect={() => undefined} />);

    expect(container.querySelectorAll("[data-area-id]")).toHaveLength(40);
    const stage = container.querySelector<HTMLElement>(".map-stage");
    expect(stage).not.toBeNull();
    expect(Number.parseFloat(stage?.style.height ?? "0")).toBeGreaterThan(1080);
    expect(Number.parseFloat(stage?.style.width ?? "0")).toBeGreaterThan(1440);
  });

  it("includes a distant stored position in the world bounds", () => {
    const distant = area(0);
    distant.position = { x: 2_400, y: 3_200 };
    const { container } = render(
      <MapCanvas view={{ areas: [distant], relations: [] }} selectedId={null} onSelect={() => undefined} />,
    );

    const stage = container.querySelector<HTMLElement>(".map-stage");
    expect(Number.parseFloat(stage?.style.width ?? "0")).toBeGreaterThan(2_900);
    expect(Number.parseFloat(stage?.style.height ?? "0")).toBeGreaterThan(3_300);
  });
});
