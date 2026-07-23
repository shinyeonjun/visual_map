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
        onOpenDatabase={vi.fn()}
        onOpenRelations={vi.fn()}
      />,
    );

    const apiTarget = screen.getByRole("button", { name: /\/api\/orders/ });
    expect(screen.getByRole("tab", { name: /API/ })).toHaveAttribute("data-target-kind", "api");
    expect(apiTarget).toHaveAttribute("data-target-id", "code:route-orders");
    fireEvent.click(apiTarget);

    expect(onSelectTarget).toHaveBeenCalledOnce();
    expect(showMode).toHaveBeenCalledWith("api-flow", "code:route-orders");
  });

  it("does not turn the current target into a hidden evidence action", () => {
    const showMode = vi.fn();
    const selectNode = vi.fn();
    const onSelectTarget = vi.fn();
    render(
      <TargetNavigator
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={{
          ...visualControls(showMode),
          currentMap: {
            mode: "api-flow",
            focus: "code:route-orders",
            nodes: [{ id: "code:route-orders", kind: "api", title: "/api/orders", layer: "api", source: "code" }],
          },
          focusId: "code:route-orders",
          selectNode,
        } as unknown as VisualMapControls}
        onSelectTarget={onSelectTarget}
        onOpenDatabase={vi.fn()}
        onOpenRelations={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /\/api\/orders/ }));

    expect(onSelectTarget).toHaveBeenCalledOnce();
    expect(showMode).not.toHaveBeenCalled();
    expect(selectNode).not.toHaveBeenCalled();
  });

  it("opens multi-target relationships from one explicit action", () => {
    const onOpenRelations = vi.fn();
    render(
      <TargetNavigator
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn())}
        onSelectTarget={vi.fn()}
        onOpenDatabase={vi.fn()}
        onOpenRelations={onOpenRelations}
      />,
    );

    expect(screen.queryByRole("button", { name: "전체 구조" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "여러 대상 관계" }));
    expect(onOpenRelations).toHaveBeenCalledOnce();
  });

  it("moves target kinds with the tab keyboard convention", () => {
    render(
      <TargetNavigator
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn())}
        onSelectTarget={vi.fn()}
        onOpenDatabase={vi.fn()}
        onOpenRelations={vi.fn()}
      />,
    );

    const api = screen.getByRole("tab", { name: /API/ });
    const code = screen.getByRole("tab", { name: /코드/ });
    const column = screen.getByRole("tab", { name: /컬럼/ });
    Object.defineProperty(screen.getByRole("tabpanel"), "scrollTo", { value: vi.fn() });
    api.focus();

    fireEvent.keyDown(api, { key: "ArrowRight" });
    expect(code).toHaveFocus();
    expect(code).toHaveAttribute("aria-selected", "true");
    expect(code).toHaveAttribute("tabindex", "0");

    fireEvent.keyDown(code, { key: "End" });
    expect(column).toHaveFocus();
    expect(column).toHaveAttribute("aria-selected", "true");

    fireEvent.keyDown(column, { key: "Home" });
    expect(api).toHaveFocus();
    expect(api).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveAttribute("aria-labelledby", "target-kind-api");
  });

  it("keeps one target in the tab order and moves focus without opening an answer", () => {
    const workspace = workspaceControls();
    workspace.codeInventory!.functions = codeItems("function", 3);
    const showMode = vi.fn();
    const { container } = render(
      <TargetNavigator
        workspaceControls={workspace}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(showMode, "search-focus")}
        onSelectTarget={vi.fn()}
        onOpenDatabase={vi.fn()}
        onOpenRelations={vi.fn()}
      />,
    );

    const targets = Array.from(
      container.querySelectorAll<HTMLButtonElement>(".target-list button[data-target-id]"),
    );
    expect(targets).toHaveLength(3);
    expect(targets.filter((target) => target.tabIndex === 0)).toEqual([targets[0]]);

    targets[0]?.focus();
    fireEvent.keyDown(targets[0]!, { key: "ArrowDown" });
    expect(targets[1]).toHaveFocus();
    expect(targets.filter((target) => target.tabIndex === 0)).toEqual([targets[1]]);

    fireEvent.keyDown(targets[1]!, { key: "End" });
    expect(targets[2]).toHaveFocus();
    fireEvent.keyDown(targets[2]!, { key: "Home" });
    expect(targets[0]).toHaveFocus();
    expect(showMode).not.toHaveBeenCalled();
  });

  it("bounds each code role by default but searches the full inventory", () => {
    const workspace = workspaceControls();
    workspace.codeInventory!.handlers = codeItems("handler", 20);
    workspace.codeInventory!.functions = codeItems("function", 20);
    workspace.codeInventory!.partial = true;
    const { container } = render(
      <TargetNavigator
        workspaceControls={workspace}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn(), "search-focus")}
        onSelectTarget={vi.fn()}
        onOpenDatabase={vi.fn()}
        onOpenRelations={vi.fn()}
      />,
    );

    expect(container.querySelectorAll(".target-list button")).toHaveLength(24);
    expect(screen.getByText("24/40개 표시 · 전체는 상단 검색")).toBeInTheDocument();
    Object.defineProperty(container.querySelector(".target-list"), "scrollTo", { value: vi.fn() });
    fireEvent.change(screen.getByLabelText("코드 목록 필터"), { target: { value: "handler19" } });
    expect(screen.getByRole("button", { name: /handler19/ })).toBeInTheDocument();
  });

  it("hides engine-only builtins while disclosing the excluded count", () => {
    const workspace = workspaceControls();
    workspace.codeInventory!.functions = [
      ...codeItems("function", 1),
      { ...codeItems("function", 1)[0], id: "builtin-len", name: "len", filePath: "<python-builtins>" },
    ];
    render(
      <TargetNavigator
        workspaceControls={workspace}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn(), "search-focus")}
        onSelectTarget={vi.fn()}
        onOpenDatabase={vi.fn()}
        onOpenRelations={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: /len/ })).not.toBeInTheDocument();
    expect(screen.getByText("1개")).toBeInTheDocument();
    expect(screen.getByText("내장 심볼 1개 제외")).toBeInTheDocument();
  });

  it("opens database setup from an empty DB target list", () => {
    const onOpenDatabase = vi.fn();
    render(
      <TargetNavigator
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualControls(vi.fn())}
        onSelectTarget={vi.fn()}
        onOpenDatabase={onOpenDatabase}
        onOpenRelations={vi.fn()}
      />,
    );

    Object.defineProperty(document.querySelector(".target-list"), "scrollTo", { value: vi.fn() });
    fireEvent.click(screen.getByRole("tab", { name: /테이블/ }));
    expect(screen.getByText("DB를 연결하면 테이블 사용 위치를 볼 수 있습니다.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "DB 연결" }));

    expect(onOpenDatabase).toHaveBeenCalledOnce();
  });

  it("chooses the first available target kind when inventory arrives", () => {
    const emptyWorkspace = workspaceControls();
    emptyWorkspace.codeInventory = null;
    const props = {
      dbProfileControls: { inventory: null } as DbProfileControls,
      visualMapControls: visualControls(vi.fn(), "atlas"),
      onSelectTarget: vi.fn(),
      onOpenDatabase: vi.fn(),
      onOpenRelations: vi.fn(),
    };
    const { rerender } = render(<TargetNavigator workspaceControls={emptyWorkspace} {...props} />);

    const codeWorkspace = workspaceControls();
    codeWorkspace.codeInventory!.routes = [];
    codeWorkspace.codeInventory!.handlers = codeItems("handler", 1);
    rerender(<TargetNavigator workspaceControls={codeWorkspace} {...props} />);

    expect(screen.getByRole("tab", { name: /코드/ })).toHaveAttribute("aria-selected", "true");
  });
});

function visualControls(showMode: ReturnType<typeof vi.fn>, mode = "api-flow"): VisualMapControls {
  return {
    currentMap: null,
    mode,
    focusId: null,
    loading: false,
    selectedNode: null,
    selectedEdge: null,
    showMode,
    selectNode: vi.fn(),
  } as unknown as VisualMapControls;
}

function codeItems(kind: string, count: number) {
  return Array.from({ length: count }, (_, index) => ({
    id: `${kind}-${index}`,
    kind,
    name: `${kind}${index}`,
    filePath: `src/${kind}${index}.ts`,
    line: 1,
    detail: null,
  }));
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
