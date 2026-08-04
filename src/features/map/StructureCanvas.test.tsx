import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StructureCanvas } from "./StructureCanvas";
import type { VisualMap, VisualNode } from "../../types/visual-map";

function area(id: string, title: string): VisualNode {
  return {
    id,
    title,
    kind: "group-domain",
    layer: "mixed",
    source: "projection",
    metrics: { apiCount: 1, codeCount: 2, dbCount: 0, topApi: [], topCode: [], topDb: [] },
  };
}

function member(id: string, title: string, path: string, layer = "api"): VisualNode {
  return {
    id,
    title,
    kind: layer === "api" ? "route" : "function",
    layer,
    source: "code",
    location: { path, line: 1 },
  };
}

const areas = [area("group:order", "주문"), area("group:catalog", "카탈로그"), area("group:billing", "결제")];

function mapWith(nodes: VisualNode[], focus = "group:order"): VisualMap {
  return { mode: "atlas", focus, nodes, edges: [] } as unknown as VisualMap;
}

function boxNamed(title: string): HTMLElement {
  return screen.getByRole("button", { name: new RegExp(`^${title} `) });
}

describe("StructureCanvas", () => {
  it("draws every area as a box on one canvas", () => {
    const { container } = render(<StructureCanvas areas={areas} onExpandArea={vi.fn()} />);

    expect(container.querySelectorAll("[data-flow-id]")).toHaveLength(3);
    for (const title of ["주문", "카탈로그", "결제"]) {
      expect(boxNamed(title)).toBeInTheDocument();
    }
  });

  it("keeps the other boxes on the canvas when one is opened", () => {
    // The whole point of the view: opening a box must not lift its neighbours
    // into a strip somewhere else, and must not replace the screen.
    const { container, rerender } = render(<StructureCanvas areas={areas} onExpandArea={vi.fn()} />);
    fireEvent.click(boxNamed("주문"));
    rerender(
      <StructureCanvas
        areas={areas}
        openId="group:order"
        map={mapWith([member("api:a", "GET /a", "src/order/a.py")])}
        onExpandArea={vi.fn()}
      />,
    );

    expect(container.querySelector(".flow-box.is-open")).toHaveAttribute("data-flow-id", "group:order");
    expect(boxNamed("카탈로그")).toBeInTheDocument();
    expect(boxNamed("결제")).toBeInTheDocument();
  });

  it("nests the members inside the box that was opened", () => {
    const { container, rerender } = render(<StructureCanvas areas={areas} onExpandArea={vi.fn()} />);
    fireEvent.click(boxNamed("주문"));
    rerender(
      <StructureCanvas
        areas={areas}
        openId="group:order"
        map={mapWith([member("api:a", "GET /a", "src/order/a.py")])}
        onExpandArea={vi.fn()}
      />,
    );

    const opened = container.querySelector(".flow-box.is-open");
    expect(within(opened as HTMLElement).getByText("GET /a")).toBeInTheDocument();
  });

  it("selects a leaf without collapsing or requesting another map", () => {
    const onExpandNode = vi.fn();
    const onSelectNode = vi.fn();
    const detail = mapWith([member("api:a", "GET /a", "src/order/a.py")]);
    const { container } = render(
      <StructureCanvas
        areas={areas}
        openId="group:order"
        map={detail}
        onExpandArea={vi.fn()}
        onExpandNode={onExpandNode}
        onSelectNode={onSelectNode}
      />,
    );

    fireEvent.click(boxNamed("GET /a"));

    expect(onSelectNode).toHaveBeenCalledWith(expect.objectContaining({ id: "api:a" }));
    expect(onExpandNode).not.toHaveBeenCalled();
    expect(container.querySelector(".flow-box.is-open")).toHaveAttribute("data-flow-id", "group:order");
  });

  it("puts a module level between a package and its members when there are many", () => {
    // Without this a package with 35 routes opens onto 35 boxes at once. The
    // engine computes these boundaries; until the desktop layer reads them,
    // they are derived from the member paths.
    const many = [
      ...Array.from({ length: 5 }, (_, i) =>
        member(`auth:${i}`, `GET /auth/${i}`, `src/plane/authentication/v${i}.py`),
      ),
      ...Array.from({ length: 5 }, (_, i) => member(`utils:${i}`, `helper${i}`, `src/plane/utils/h${i}.py`, "code")),
    ];
    const { rerender } = render(<StructureCanvas areas={areas} onExpandArea={vi.fn()} />);
    fireEvent.click(boxNamed("주문"));
    rerender(<StructureCanvas areas={areas} openId="group:order" map={mapWith(many)} onExpandArea={vi.fn()} />);

    expect(boxNamed("authentication")).toBeInTheDocument();
    expect(boxNamed("utils")).toBeInTheDocument();
    // The routes themselves are one level deeper, not on the package.
    expect(screen.queryByRole("button", { name: /^GET \/auth\/0 / })).not.toBeInTheDocument();
  });

  it("does not ask the map to focus a module the canvas invented", () => {
    // The derived id is not a node the map knows. Handing it over made the map
    // drop its focus, which collapsed the canvas the reader had just opened.
    const onExpandNode = vi.fn();
    const many = [
      ...Array.from({ length: 5 }, (_, i) =>
        member(`auth:${i}`, `GET /auth/${i}`, `src/plane/authentication/v${i}.py`),
      ),
      ...Array.from({ length: 5 }, (_, i) => member(`utils:${i}`, `helper${i}`, `src/plane/utils/h${i}.py`, "code")),
    ];
    const { container, rerender } = render(
      <StructureCanvas areas={areas} onExpandArea={vi.fn()} onExpandNode={onExpandNode} />,
    );
    fireEvent.click(boxNamed("주문"));
    rerender(
      <StructureCanvas
        areas={areas}
        openId="group:order"
        map={mapWith(many)}
        onExpandArea={vi.fn()}
        onExpandNode={onExpandNode}
      />,
    );

    fireEvent.click(boxNamed("authentication"));
    expect(onExpandNode).not.toHaveBeenCalled();
    expect(container.querySelectorAll(".flow-box.is-open")).toHaveLength(2);
    expect(boxNamed("GET /auth/0")).toBeInTheDocument();
  });

  it("walks back out through the breadcrumb", () => {
    const onCollapse = vi.fn();
    const { container, rerender } = render(
      <StructureCanvas areas={areas} onExpandArea={vi.fn()} onCollapse={onCollapse} />,
    );
    fireEvent.click(boxNamed("주문"));
    rerender(
      <StructureCanvas
        areas={areas}
        openId="group:order"
        map={mapWith([member("api:a", "GET /a", "src/order/a.py")])}
        onExpandArea={vi.fn()}
        onCollapse={onCollapse}
      />,
    );

    expect(screen.getByRole("button", { name: "주문" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /전체/ }));
    expect(onCollapse).toHaveBeenCalled();
    expect(container.querySelector(".flow-box.is-open")).toBeNull();
  });

  it("lands a relation on the closed box that owns its endpoint", () => {
    // Closing a box must not delete the relations that pointed inside it.
    const { container } = render(
      <StructureCanvas
        areas={areas}
        edges={[
          {
            id: "e1",
            from: "group:order",
            to: "group:catalog",
            kind: "group_calls",
            evidence: [{ kind: "engine", text: "call" }],
          },
        ]}
        onExpandArea={vi.fn()}
      />,
    );

    expect(container.querySelectorAll(".flow-wire")).toHaveLength(1);
  });

  it("never prints the packed subtitle transport string on a box", () => {
    const packed: VisualNode = {
      id: "group:packed",
      title: "묶음",
      kind: "group-domain",
      layer: "mixed",
      source: "projection",
      subtitle: "API 3 · 코드 4 · DB 0|GET /a|handler.py|",
    };
    render(<StructureCanvas areas={[packed]} onExpandArea={vi.fn()} />);

    const box = boxNamed("묶음");
    expect(box.textContent).not.toContain("|");
    expect(within(box).getByText("GET /a · handler.py")).toBeInTheDocument();
  });
});
