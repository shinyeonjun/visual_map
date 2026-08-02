import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory } from "../../types/workspace";
import { ProjectExplorer } from "./ProjectExplorer";

describe("ProjectExplorer", () => {
  it("opens a target and exposes only the two product goals", () => {
    const showMode = vi.fn();
    const onShowAdvanced = vi.fn();
    const onShowAnswers = vi.fn();
    render(
      <ProjectExplorer
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(showMode)}
        onOpenDatabase={vi.fn()}
        onShowAnswers={onShowAnswers}
        onShowAdvanced={onShowAdvanced}
      />,
    );

    expect(screen.getByRole("tab", { name: /API 1/ })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /API/ }));
    fireEvent.click(screen.getByRole("button", { name: /GET \/orders/ }));
    expect(showMode).toHaveBeenCalledWith("api-flow", "code:route-orders");

    fireEvent.click(screen.getByRole("button", { name: "이해하기" }));
    expect(onShowAdvanced).toHaveBeenCalledWith("atlas");
    fireEvent.click(screen.getByRole("button", { name: "영향 보기" }));
    expect(onShowAnswers).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "관계 연결" })).not.toBeInTheDocument();
  });

  it("renders code targets under source folders and keeps selection behavior", () => {
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
      <ProjectExplorer
        workspaceControls={{ ...workspaceControls(), codeInventory } as WorkspaceControls}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(showMode)}
        onOpenDatabase={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("tab", { name: /코드/ }));
    expect(screen.getByRole("button", { name: "src, 1개 코드 항목" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "service.ts, 1개 코드 항목" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /loadOrders/ }));
    expect(showMode).toHaveBeenCalledWith("search-focus", "code:function-load-orders");
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
