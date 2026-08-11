import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Inspector } from "./Inspector";
import type { MapTrace, Selection } from "./types";

const orderTrace: MapTrace = {
  id: "t1",
  state: "complete",
  steps: [
    { id: "e1", name: "POST /orders", kind: "HTTP Endpoint", role: "endpoint", definition: null },
    { id: "c1", name: "OrderController.create", kind: "Method", role: "controller", definition: null },
    { id: "s1", name: "OrderService.create", kind: "Method", role: "service", definition: null },
  ],
};

function selection(overrides: Partial<Selection> = {}): Selection {
  return {
    id: "area-0",
    title: "주문",
    role: "주문 생성과 조회를 담당",
    relations: [],
    evidence: [],
    source: null,
    traces: [],
    analysisGaps: { totalCount: 0, items: [], truncatedCount: 0 },
    ...overrides,
  };
}

describe("Inspector", () => {
  it("offers the flow and the code as the two next moves", () => {
    const onOpenTrace = vi.fn();
    const onOpenEvidence = vi.fn();
    render(
      <Inspector
        selection={selection({ traces: [orderTrace], evidence: [{ path: "src/orders/order.service.ts", line: 87 }] })}
        onOpenTrace={onOpenTrace}
        onOpenEvidence={onOpenEvidence}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /흐름 보기/ }));
    fireEvent.click(screen.getByRole("button", { name: /코드 열기/ }));

    expect(onOpenTrace).toHaveBeenCalledOnce();
    expect(onOpenEvidence).toHaveBeenCalledWith({ path: "src/orders/order.service.ts", line: 87 });
  });

  it("does not offer a flow for a selection that has no confirmed path", () => {
    render(<Inspector selection={selection({ traces: [] })} onOpenTrace={vi.fn()} />);
    expect(screen.getByRole("button", { name: /흐름 보기/ })).toBeDisabled();
    expect(screen.getByText("확인된 경로 없음")).toBeInTheDocument();
  });

  it("reads entry APIs off the first step of each path rather than guessing", () => {
    const { container } = render(<Inspector selection={selection({ traces: [orderTrace] })} />);
    const entries = container.querySelector(".entry-list");

    expect(entries).not.toBeNull();
    expect(within(entries as HTMLElement).getByText("POST /orders")).toBeInTheDocument();
    // The handler shown is literally the next confirmed step, not a name match.
    expect(within(entries as HTMLElement).getByText("OrderController.create")).toBeInTheDocument();
  });

  it("lists a path as one line and leaves its steps to the flow view", () => {
    render(<Inspector selection={selection({ traces: [orderTrace] })} onOpenTrace={vi.fn()} />);

    expect(screen.getByText("2홉")).toBeInTheDocument();
    expect(screen.getByText("→ OrderService.create")).toBeInTheDocument();
    // The middle step belongs on the canvas, not in a cramped column here.
    expect(screen.queryByText("Method")).toBeNull();
  });

  it("says why analysis fell short, not only how often", () => {
    const { container } = render(
      <Inspector
        selection={selection({
          analysisGaps: {
            totalCount: 20,
            truncatedCount: 4,
            items: [
              { code: "dynamic_dispatch", capability: "call-graph", message: "런타임에 결정되는 호출이 있습니다." },
            ],
          },
        })}
      />,
    );

    // A bare count only says "something is missing"; the code and the message
    // are what make it actionable.
    expect(screen.getByText("dynamic_dispatch")).toBeInTheDocument();
    expect(screen.getByText("런타임에 결정되는 호출이 있습니다.")).toBeInTheDocument();
    expect(screen.getByText("call-graph")).toBeInTheDocument();
    expect(screen.getByText("+4건 더 있음")).toBeInTheDocument();
    expect(container.querySelector(".gap-section h3 span")?.textContent).toBe("20");
  });

  it("folds identical gap records into one row that counts them", () => {
    const repeated = {
      code: "unresolved_target",
      capability: null,
      message: "Provider records without an exact source range were not promoted.",
    };
    const { container } = render(
      <Inspector
        selection={selection({ analysisGaps: { totalCount: 9, truncatedCount: 0, items: Array(4).fill(repeated) } })}
      />,
    );

    // Four byte-identical rows spend four lines of a narrow panel saying one
    // thing — and give React four children with the same key.
    expect(container.querySelectorAll(".gap-row")).toHaveLength(1);
    expect(screen.getByText("×4")).toBeInTheDocument();
  });

  it("stays quiet about gaps when the engine reported none", () => {
    const { container } = render(<Inspector selection={selection()} />);
    expect(container.querySelector(".gap-section")).toBeNull();
  });

  it("says nothing is picked rather than showing an empty frame", () => {
    render(<Inspector selection={null} />);
    expect(screen.getByText("지도에서 영역이나 항목을 선택하세요.")).toBeInTheDocument();
  });
});
