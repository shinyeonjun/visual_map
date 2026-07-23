import { fireEvent, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory, CodeInventoryItem } from "../../types/workspace";
import type { VisualMap } from "../../types/visual-map";
import { ModePanel } from "./ModePanel";

describe("ModePanel navigation context", () => {
  beforeEach(() => localStorage.clear());

  it("keeps answer modes out of the advanced navigation", () => {
    const { container } = renderMode("atlas", "overview", []);
    const modes = [...container.querySelectorAll<HTMLButtonElement>("[data-mode-id]")]
      .map((button) => button.dataset.modeId);

    expect(modes).toEqual(["atlas", "composition"]);
    expect(container.textContent).not.toContain("API");
    expect(container.textContent).not.toContain("영향");
  });

  it("toggles composition subjects in one stable mixed inventory", () => {
    const toggleCompositionFocus = vi.fn();
    const controls = {
      currentMap: null,
      mode: "composition",
      focusId: null,
      loading: false,
      selectedNode: null,
      selectedEdge: null,
      compositionFocusIds: ["code:route-a"],
      toggleCompositionFocus,
      showMode: vi.fn(),
      selectNode: vi.fn(),
    } as unknown as VisualMapControls;
    const db = {
      inventory: {
        tables: [{
          schema: "public",
          name: "orders",
          columns: [{ name: "id", dataType: "uuid", isPrimaryKey: true, isForeignKey: false }],
        }],
      },
      selectedTableKey: null,
    } as unknown as DbProfileControls;
    const workspace = workspaceControls([route("route-a", "/api/a")]);
    workspace.codeInventory!.functions = Array.from({ length: 140 }, (_, index) => ({
      id: `function-${index}`,
      kind: "function",
      name: `function${index}`,
      filePath: `src/function-${index}.ts`,
      line: index + 1,
      detail: null,
    }));
    workspace.codeInventory!.functions.push({
      id: "builtin-abs",
      kind: "function",
      name: "abs",
      filePath: "<python-builtins>",
      line: 1,
      detail: null,
    });
    const { container } = render(
      <ModePanel
        workspaceControls={workspace}
        dbProfileControls={db}
        visualMapControls={controls}
      />,
    );

    const routeInput = container.querySelector<HTMLInputElement>('[data-context-id="code:route-a"] input')!;
    const tableInput = container.querySelector<HTMLInputElement>('[data-context-id="db:table:public.orders"] input')!;
    expect(routeInput).toBeChecked();
    expect(tableInput).not.toBeChecked();

    fireEvent.click(tableInput);

    expect(toggleCompositionFocus).toHaveBeenCalledWith("db:table:public.orders");
    expect(controls.showMode).not.toHaveBeenCalled();
    expect(container.querySelector('[data-context-id="code:builtin-abs"]')).not.toBeInTheDocument();
  });

  it("allows DB-only composition when a table and column are available", () => {
    const showMode = vi.fn();
    const db = {
      inventory: {
        tables: [{
          schema: "public",
          name: "orders",
          columns: [{ name: "id", dataType: "uuid", isPrimaryKey: true, isForeignKey: false }],
        }],
      },
      selectedTableKey: null,
    } as unknown as DbProfileControls;
    const { container } = render(
      <ModePanel
        workspaceControls={workspaceControls([])}
        dbProfileControls={db}
        visualMapControls={{
          currentMap: null,
          mode: "atlas",
          focusId: null,
          loading: false,
          selectedNode: null,
          selectedEdge: null,
          showMode,
          selectNode: vi.fn(),
        } as unknown as VisualMapControls}
      />,
    );
    const composition = container.querySelector<HTMLButtonElement>('[data-mode-id="composition"]')!;

    expect(composition).not.toHaveClass("locked");
    fireEvent.click(composition);
    expect(showMode).toHaveBeenCalledWith("composition", null);
  });

  it("keeps the atlas context list mounted while a focused atlas view reloads", () => {
    const workspace = workspaceControls([]);
    const db = { inventory: null, selectedTableKey: null } as unknown as DbProfileControls;
    const map: VisualMap = {
      id: "atlas-overview",
      workspaceId: "workspace-1",
      mode: "atlas",
      focus: "overview",
      nodes: [{ id: "group:auth", kind: "group-domain", title: "인증", layer: "group", source: "code" }],
      edges: [],
      warnings: [],
    };
    const controls = {
      currentMap: map,
      mode: "atlas",
      focusId: null,
      loading: false,
      selectedNode: null,
      selectedEdge: null,
      showMode: vi.fn(),
      selectNode: vi.fn(),
    } as unknown as VisualMapControls;
    const { container, rerender } = render(
      <ModePanel workspaceControls={workspace} dbProfileControls={db} visualMapControls={controls} />,
    );

    expect(container.querySelector('[data-context-id="group:auth"]')).toBeInTheDocument();

    rerender(
      <ModePanel
        workspaceControls={workspace}
        dbProfileControls={db}
        visualMapControls={{ ...controls, currentMap: null, focusId: "group:auth", loading: true }}
      />,
    );

    expect(container.querySelector('[data-context-id="group:auth"]')).toBeInTheDocument();
    expect(container.querySelector('[data-context-id="group:auth"]')?.textContent).toContain("현재");
  });
});

function renderMode(
  mode: string,
  focusId: string,
  routes: CodeInventoryItem[],
  showMode = vi.fn(),
) {
  return render(
    <ModePanel
      workspaceControls={workspaceControls(routes)}
      dbProfileControls={{ inventory: null, selectedTableKey: null } as unknown as DbProfileControls}
      visualMapControls={{
        currentMap: null,
        mode,
        focusId,
        loading: false,
        selectedNode: null,
        selectedEdge: null,
        showMode,
        selectNode: vi.fn(),
      } as unknown as VisualMapControls}
    />,
  );
}

function workspaceControls(routes: CodeInventoryItem[]): WorkspaceControls {
  return {
    initialized: true,
    currentWorkspace: {
      id: "workspace-1",
      name: "backend",
      repoPath: "D:/backend",
      repoSource: "local",
      dbProfiles: [],
      createdAt: "2026-07-20T00:00:00Z",
      updatedAt: "2026-07-20T00:00:00Z",
    },
    codeInventory: inventory(routes),
    selectedCodeItem: null,
  } as unknown as WorkspaceControls;
}

function inventory(routes: CodeInventoryItem[]): CodeInventory {
  return {
    project: "backend",
    routes,
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
      routes: routes.length,
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

function route(id: string, name: string): CodeInventoryItem {
  return {
    id,
    kind: "api",
    name,
    filePath: `server/routes/${id}.ts`,
    line: 12,
    detail: null,
  };
}
