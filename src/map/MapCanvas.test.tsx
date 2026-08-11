import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MapCanvas } from "./MapCanvas";
import type { TraceView } from "./MapCanvas";
import type { MapView } from "./types";

const view: MapView = {
  areas: [
    {
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
      boundaryRelationCounts: { verified: 0, structural: 0, candidate: 0 },
      affectingAnalysisGapCount: 0,
    },
  ],
  relations: [],
  unattributedAnalysisGapCount: 0,
};

function draw(traceView: TraceView | null) {
  return render(
    <MapCanvas
      view={view}
      selectedId={null}
      onSelect={() => undefined}
      traceView={traceView}
      onOpenTrace={() => undefined}
      onCloseTrace={() => undefined}
    />,
  );
}

describe("map canvas view routing", () => {
  it("draws the field of areas until a flow is opened", () => {
    const { container } = draw(null);
    expect(container.querySelector(".map-stage")).not.toBeNull();
    expect(container.querySelector(".trace-canvas")).toBeNull();
  });

  it("hands over to the flow view for the area that was opened", () => {
    const { container } = draw({
      areaId: "area-0",
      title: "POST /orders",
      summary: "주문 생성",
      traces: [
        {
          id: "t1",
          state: "complete",
          steps: [
            { id: "n1", name: "POST /orders", kind: "HTTP Endpoint", role: "endpoint", definition: null },
            { id: "n2", name: "OrderService.create", kind: "Method", role: "service", definition: null },
          ],
        },
      ],
    });

    expect(container.querySelector(".map-stage")).toBeNull();
    expect(screen.getByRole("heading", { name: "POST /orders" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /OrderService\.create/ })).toBeInTheDocument();
  });

  it("says so plainly when an opened area has no confirmed path", () => {
    draw({ areaId: "area-0", title: "주문", summary: "", traces: [] });
    expect(screen.getByText("확인된 실행 경로가 없습니다.")).toBeInTheDocument();
  });
});
