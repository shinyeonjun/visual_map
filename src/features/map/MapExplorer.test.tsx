import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory } from "../../types/workspace";
import { MapExplorer } from "./MapExplorer";

describe("MapExplorer", () => {
  it("reveals a target's layer without navigating the central canvas", () => {
    const showMode = vi.fn();
    const onSelectTarget = vi.fn();
    render(
      <MapExplorer
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(showMode)}
        onOpenDatabase={vi.fn()}
        onSelectTarget={onSelectTarget}
      />,
    );

    // Every kind is a stacked, already-open section — no tabs hiding the rest.
    expect(screen.queryAllByRole("tab")).toHaveLength(0);
    expect(screen.getByRole("button", { name: /API 라우트/ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "데이터베이스0" })).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(screen.getByRole("button", { name: /GET \/orders/ }));
    expect(showMode).not.toHaveBeenCalled();
    expect(onSelectTarget).toHaveBeenCalledWith(expect.objectContaining({ title: "GET /orders" }));
    expect(screen.getByRole("button", { name: /API 라우트/ })).toHaveAttribute("data-revealed", "true");

    // Navigation is one axis: picking a target descends, the breadcrumb ascends.
    // The explorer itself carries no mode switch.
    expect(screen.queryByRole("button", { name: "이해하기" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "영향 보기" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "관계 연결" })).not.toBeInTheDocument();
  });

  it("renders a compact code preview and reveals code without navigation", () => {
    const showMode = vi.fn();
    const codeInventory = inventory();
    codeInventory.functions = [
      {
        id: "function-load-orders",
        kind: "function",
        name: "loadOrders",
        filePath: "src/orders/service.ts",
        line: 23,
        detail: null,
      },
    ];
    codeInventory.summary.functions = 1;

    render(
      <MapExplorer
        workspaceControls={{ ...workspaceControls(), codeInventory } as WorkspaceControls}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(showMode)}
        onOpenDatabase={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /loadOrders/ }));
    expect(showMode).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "코드1" })).toHaveAttribute("data-revealed", "true");
  });

  it("shows exact totals while previewing a bounded inventory", () => {
    const codeInventory = inventory();
    codeInventory.partial = true;
    codeInventory.summary.routes = 649;
    codeInventory.summary.functions = 71_902;
    codeInventory.functions = [
      { id: "function-visible", kind: "function", name: "visible", filePath: "src/app.ts", detail: null },
    ];

    render(
      <MapExplorer
        workspaceControls={{ ...workspaceControls(), codeInventory } as WorkspaceControls}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn())}
        onOpenDatabase={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "API 라우트649" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "코드71902" })).toBeInTheDocument();
    expect(screen.getByText("… 645개 더보기 · 검색으로 좁히기")).toBeInTheDocument();
    expect(screen.getByText("… 71898개 더보기 · 검색으로 좁히기")).toBeInTheDocument();
  });
});

function workspaceControls(): WorkspaceControls {
  return {
    initialized: true,
    currentWorkspace: { id: "workspace-1", name: "Shop" },
    codeInventory: inventory(),
  } as unknown as WorkspaceControls;
}

function visualControls(showMode: ReturnType<typeof vi.fn>): VisualMapControls {
  return {
    currentMap: null,
    mode: "atlas",
    focusId: null,
    compositionFocusIds: [],
    relationView: "connections",
    searchQuery: "",
    searchPopoverOpen: false,
    searchSummary: null,
    searchGroups: [],
    showMode,
    toggleCompositionFocus: vi.fn(),
    openSearchPopover: vi.fn(),
    closeSearchPopover: vi.fn(),
    setSearchQuery: vi.fn(),
    runSearch: vi.fn(),
    selectSearchResult: vi.fn(),
  } as unknown as VisualMapControls;
}

function inventory(): CodeInventory {
  return {
    project: "shop",
    routes: [
      { id: "route-orders", kind: "route", name: "GET /orders", filePath: "src/routes.ts", line: 12, detail: null },
    ],
    services: [],
    files: [],
    handlers: [],
    repositories: [],
    functions: [],
    classes: [],
    modules: [],
    unknown: [],
    calls: [],
    summary: {
      routes: 1,
      handlers: 0,
      services: 0,
      repositories: 0,
      functions: 0,
      classes: 0,
      modules: 0,
      files: 0,
      unknown: 0,
    },
  };
}
