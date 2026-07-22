import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory } from "../../types/workspace";
import { TargetNavigator } from "./TargetNavigator";

describe("TargetNavigator", () => {
  it("opens an API in its automatic answer mode", () => {
    const showMode = vi.fn();
    const onSelectTarget = vi.fn();
    render(
      <TargetNavigator
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(showMode)}
        onSelectTarget={onSelectTarget}
        onOpenAdvanced={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /\/api\/orders/ }));

    expect(onSelectTarget).toHaveBeenCalledOnce();
    expect(showMode).toHaveBeenCalledWith("api-flow", "code:route-orders");
  });

  it("keeps the full atlas behind an explicit advanced action", () => {
    const onOpenAdvanced = vi.fn();
    render(
      <TargetNavigator
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn())}
        onSelectTarget={vi.fn()}
        onOpenAdvanced={onOpenAdvanced}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "전체 구조" }));
    expect(onOpenAdvanced).toHaveBeenCalledWith("atlas");
  });
});

function visualControls(showMode: ReturnType<typeof vi.fn>): VisualMapControls {
  return {
    currentMap: null,
    mode: "api-flow",
    focusId: null,
    loading: false,
    selectedNode: null,
    selectedEdge: null,
    showMode,
    selectNode: vi.fn(),
  } as unknown as VisualMapControls;
}

function workspaceControls(): WorkspaceControls {
  return {
    initialized: true,
    currentWorkspace: { id: "workspace-1", name: "Orders" },
    codeInventory: codeInventory(),
  } as unknown as WorkspaceControls;
}

function codeInventory(): CodeInventory {
  return {
    project: "orders",
    routes: [{ id: "route-orders", kind: "api", name: "/api/orders", filePath: "src/routes.ts", line: 12, detail: null }],
    services: [],
    handlers: [],
    repositories: [],
    functions: [],
    classes: [],
    modules: [],
    unknown: [],
    files: [],
    calls: [],
    summary: { routes: 1, handlers: 0, services: 0, repositories: 0, functions: 0, classes: 0, modules: 0, files: 0, unknown: 0 },
  };
}
