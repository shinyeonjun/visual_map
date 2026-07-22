import { fireEvent, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory, CodeInventoryItem } from "../../types/workspace";
import type { VisualMap } from "../../types/visual-map";
import { ModePanel } from "./ModePanel";

describe("ModePanel navigation context", () => {
  beforeEach(() => localStorage.clear());

  it("keeps the API route order stable and marks the current target in place", () => {
    const routes = [route("route-a", "/api/a"), route("route-current", "/api/current")];
    const { container } = renderMode("api-flow", "code:route-current", routes);

    const items = container.querySelectorAll<HTMLButtonElement>(".product-context-list > button");
    expect(items[0]?.dataset.contextId).toBe("route-a");
    expect(items[1]?.dataset.contextId).toBe("route-current");
    expect(items[1]?.textContent).toContain("현재");
    expect(container.querySelector(".product-context-current")).not.toBeInTheDocument();
  });

  it("keeps routes available when the user switches to the code view", () => {
    const routes = [route("route-current", "/api/current")];
    const { container } = renderMode("search-focus", "code:route-current", routes);

    const first = container.querySelector<HTMLButtonElement>(".product-context-list > button");
    expect(first?.dataset.contextId).toBe("route-current");
    expect(first?.textContent).toContain("/api/current");
  });

  it("keeps the previous analysis criterion until a same-mode refresh commits", () => {
    const routes = [route("route-old", "/api/old"), route("route-next", "/api/next")];
    const workspace = workspaceControls(routes);
    const currentMap: VisualMap = {
      id: "api-old",
      workspaceId: "workspace-1",
      mode: "api-flow",
      focus: "code:route-old",
      nodes: [],
      edges: [],
      warnings: [],
    };
    const controls = {
      currentMap,
      mode: "api-flow",
      focusId: "code:route-next",
      loading: true,
      selectedNode: null,
      selectedEdge: null,
      showMode: vi.fn(),
      selectNode: vi.fn(),
    } as unknown as VisualMapControls;
    const { container } = render(
      <ModePanel
        workspaceControls={workspace}
        dbProfileControls={{ inventory: null, selectedTableKey: null } as unknown as DbProfileControls}
        visualMapControls={controls}
      />,
    );

    expect(container.querySelector('[data-context-id="route-old"]')?.textContent).toContain("현재");
    expect(container.querySelector('[data-context-id="route-next"]')?.getAttribute("aria-current")).toBeNull();
  });

  it("replaces a pending target request instead of selecting stale evidence", () => {
    const routes = [route("route-old", "/api/old"), route("route-next", "/api/next")];
    const showMode = vi.fn();
    const selectNode = vi.fn();
    const currentMap: VisualMap = {
      id: "api-old",
      workspaceId: "workspace-1",
      mode: "api-flow",
      focus: "code:route-old",
      nodes: [{ id: "code:route-old", kind: "api", title: "/api/old", layer: "api", source: "code" }],
      edges: [],
      warnings: [],
    };
    const { container } = render(
      <ModePanel
        workspaceControls={workspaceControls(routes)}
        dbProfileControls={{ inventory: null, selectedTableKey: null } as unknown as DbProfileControls}
        visualMapControls={{
          currentMap,
          mode: "api-flow",
          focusId: "code:route-next",
          loading: true,
          selectedNode: null,
          selectedEdge: null,
          showMode,
          selectNode,
        } as unknown as VisualMapControls}
      />,
    );

    fireEvent.click(container.querySelector<HTMLButtonElement>('[data-context-id="route-old"]')!);

    expect(showMode).toHaveBeenCalledWith("api-flow", "code:route-old");
    expect(selectNode).not.toHaveBeenCalled();
  });

  it("keeps the committed navigation visible while another mode is loading", () => {
    const workspace = workspaceControls([route("route-old", "/api/old")]);
    const controls = {
      currentMap: {
        id: "api-old",
        workspaceId: "workspace-1",
        mode: "api-flow",
        focus: "code:route-old",
        nodes: [],
        edges: [],
        warnings: [],
      },
      mode: "search-focus",
      focusId: null,
      loading: true,
      selectedNode: null,
      selectedEdge: null,
      showMode: vi.fn(),
      selectNode: vi.fn(),
    } as unknown as VisualMapControls;
    const { container } = render(
      <ModePanel
        workspaceControls={workspace}
        dbProfileControls={{ inventory: null, selectedTableKey: null } as unknown as DbProfileControls}
        visualMapControls={controls}
      />,
    );

    expect(container.querySelector('[data-mode-id="api"]')).toHaveAttribute("aria-current", "page");
    expect(container.querySelector('[data-mode-id="search"]')).toHaveAttribute("aria-busy", "true");
    expect(container.querySelector(".product-context-browser")?.getAttribute("aria-label")).toBe("API 라우트 탐색");
  });

  it("opens compact context only from the explicit item button", async () => {
    const showMode = vi.fn();
    const { container } = renderMode("api-flow", "code:route-current", [route("route-current", "/api/current")], showMode);

    fireEvent.click(container.querySelector<HTMLButtonElement>('[data-mode-id="api"]')!);

    expect(showMode).not.toHaveBeenCalled();
    expect(container.querySelector(".product-context-browser")?.classList.contains("compact-open")).toBe(false);

    fireEvent.click(container.querySelector<HTMLButtonElement>(".product-context-toggle")!);
    expect(container.querySelector(".product-context-browser")?.classList.contains("compact-open")).toBe(true);
    await waitFor(() => expect(container.querySelector<HTMLInputElement>(".product-context-filter input")).toHaveFocus());

    fireEvent.keyDown(container.querySelector(".product-context-browser")!, { key: "Escape" });
    expect(container.querySelector(".product-context-browser")?.classList.contains("compact-open")).toBe(false);
    await waitFor(() => expect(container.querySelector<HTMLButtonElement>(".product-context-toggle")).toHaveFocus());
  });

  it("opens an unvisited mode without silently choosing its first item", () => {
    const showMode = vi.fn();
    const routes = [route("route-a", "/api/a")];
    const { container } = renderMode("api-flow", "code:route-a", routes, showMode);

    fireEvent.click(container.querySelector<HTMLButtonElement>('[data-mode-id="search"]')!);

    expect(showMode).toHaveBeenCalledWith("search-focus", null);
  });

  it("restores the context list scroll position independently for each mode", () => {
    const workspace = workspaceControls([route("route-a", "/api/a")]);
    const db = { inventory: null, selectedTableKey: null } as unknown as DbProfileControls;
    const controls = (mode: string) => ({
      currentMap: null,
      mode,
      focusId: null,
      loading: false,
      selectedNode: null,
      selectedEdge: null,
      showMode: vi.fn(),
      selectNode: vi.fn(),
    }) as unknown as VisualMapControls;
    const { container, rerender } = render(
      <ModePanel workspaceControls={workspace} dbProfileControls={db} visualMapControls={controls("api-flow")} />,
    );
    const list = container.querySelector<HTMLDivElement>(".product-context-list")!;
    list.scrollTop = 72;
    fireEvent.scroll(list);

    rerender(<ModePanel workspaceControls={workspace} dbProfileControls={db} visualMapControls={controls("search-focus")} />);
    expect(list.scrollTop).toBe(0);

    list.scrollTop = 34;
    fireEvent.scroll(list);
    rerender(<ModePanel workspaceControls={workspace} dbProfileControls={db} visualMapControls={controls("api-flow")} />);
    expect(list.scrollTop).toBe(72);
  });

  it("keeps change-impact columns grouped by table instead of choosing a table implicitly", () => {
    const db = {
      inventory: {
        tables: [
          { schema: "public", name: "users", columns: [{ name: "id", dataType: "uuid", isPrimaryKey: true, isForeignKey: false }] },
          { schema: "public", name: "orders", columns: [{ name: "user_id", dataType: "uuid", isPrimaryKey: false, isForeignKey: true }] },
        ],
      },
      selectedTableKey: null,
      openColumn: vi.fn(),
    } as unknown as DbProfileControls;
    const controls = {
      currentMap: null,
      mode: "column-impact",
      focusId: null,
      loading: false,
      selectedNode: null,
      selectedEdge: null,
      showMode: vi.fn(),
      selectNode: vi.fn(),
    } as unknown as VisualMapControls;
    const { container } = render(
      <ModePanel workspaceControls={workspaceControls([])} dbProfileControls={db} visualMapControls={controls} />,
    );

    expect([...container.querySelectorAll(".product-context-list > h3")].map((item) => item.textContent)).toEqual([
      "public.users",
      "public.orders",
    ]);
    expect(container.querySelectorAll(".product-context-list > button")).toHaveLength(2);
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
