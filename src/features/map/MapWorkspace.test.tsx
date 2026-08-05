import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualMap } from "../../types/visual-map";
import { MapWorkspace } from "./MapWorkspace";

vi.mock("./MapInspector", () => ({
  MapInspector: ({ onClose }: { onClose?: () => void }) => (
    <div>
      <button type="button" onClick={onClose}>
        근거 닫기
      </button>
    </div>
  ),
}));
vi.mock("./MapSourcePanel", () => ({ MapSourcePanel: () => <div /> }));
vi.mock("./MapStatusBar", () => ({ MapStatusBar: () => <div /> }));
vi.mock("./MapTopBar", () => ({
  MapTopBar: () => <header data-testid="topbar" />,
}));

const showMode = vi.fn();

describe("MapWorkspace canvas transitions", () => {
  beforeEach(() => showMode.mockClear());

  it("keeps empty target navigation out of the first-run setup", () => {
    const { container } = render(
      <MapWorkspace
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={
          {
            initialized: true,
            currentWorkspace: null,
            operationStatus: { phase: "idle", label: "준비", message: "준비됨" },
          } as unknown as WorkspaceControls
        }
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={
          {
            currentMap: null,
            mode: "atlas",
            focusId: null,
            loading: false,
            snapshotStaleReasons: [],
            snapshotSavedAt: "test",
            selectedNode: null,
            selectedEdge: null,
            clearSelection: vi.fn(),
            showMode: vi.fn(),
          } as unknown as VisualMapControls
        }
        engineRegistry={null}
        engineError={null}
      />,
    );

    expect(screen.getByRole("heading", { name: "프로젝트를 지도에 올리세요" })).toBeInTheDocument();
    expect(screen.queryByTestId("answer-navigation")).not.toBeInTheDocument();
    expect(screen.queryByTestId("advanced-navigation")).not.toBeInTheDocument();
    expect(container.querySelector(".map-canvas-layer")).toBeInTheDocument();
    expect(container.querySelector(".map-app-shell")).toHaveAttribute("data-left-panel", "closed");
  });

  it.each([
    ["the project opens", true, false],
    ["the saved answer restores", false, true],
  ])("keeps the shell visible while %s", (_phase, opening, restoringSnapshot) => {
    const { container } = render(
      <MapWorkspace
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={
          {
            initialized: true,
            currentWorkspace: { id: "workspace-1", name: "Orders" },
            opening,
            restoringSnapshot,
            codeInventory: null,
            operationStatus: { phase: "running", label: "저장 결과 확인", message: "저장 결과 확인 진행 중" },
          } as unknown as WorkspaceControls
        }
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={
          {
            currentMap: null,
            mode: "atlas",
            focusId: null,
            loading: false,
            snapshotStaleReasons: [],
            selectedNode: null,
            selectedEdge: null,
            clearSelection: vi.fn(),
            showMode: vi.fn(),
          } as unknown as VisualMapControls
        }
        engineRegistry={null}
        engineError={null}
      />,
    );

    expect(screen.getByRole("region", { name: /구조 흐름/ })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "캔버스 도구" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "레이어 탐색기" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "프로젝트를 지도에 올리세요" })).not.toBeInTheDocument();
    expect(container.querySelector(".map-app-shell")).toHaveAttribute("data-left-panel", "closed");
  });

  it("keeps the committed layout until the requested map commits", async () => {
    render(<Harness />);

    // Landing on an answer: breadcrumb shows root › target, root is clickable.
    expect(screen.getByRole("region", { name: /구조 흐름/ })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "구조 위치" })).toHaveTextContent("전체 프로젝트");
    expect(screen.getByRole("status")).toHaveTextContent("답 준비 완료: GET /orders");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "ready");
    fireEvent.click(screen.getByRole("button", { name: "전체 프로젝트" }));

    expect(showMode).toHaveBeenLastCalledWith("atlas", null);
    expect(screen.getByRole("region", { name: /구조 흐름/ })).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "idle");

    fireEvent.click(screen.getByRole("button", { name: "전체 구조 커밋" }));
    await waitFor(() => expect(screen.getByRole("region", { name: /구조 흐름/ })).toBeInTheDocument());
    expect(screen.getByRole("region", { name: /전체 프로젝트 구조 흐름/ })).toBeInTheDocument();

    // Descending again happens by picking a target, not by a mode switch.
    fireEvent.click(screen.getByRole("button", { name: "대상 선택" }));
    expect(showMode).toHaveBeenLastCalledWith("api-flow", "code:route-orders");
    expect(screen.getByRole("region", { name: /구조 흐름/ })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("선택한 대상 분석 중");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "loading");

    fireEvent.click(screen.getByRole("button", { name: "답 커밋" }));
    await waitFor(() => expect(screen.getByRole("region", { name: /구조 흐름/ })).toBeInTheDocument());
    expect(screen.getByRole("status")).toHaveTextContent("답 준비 완료: GET /orders");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "ready");
  });

  it("warns that the map is partial before the canvas is read", () => {
    const answerMap = map("table-usage", "db:table:public.orders");
    const { container } = render(
      <MapWorkspace
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={workspaceControls(partiallyIndexedInventory())}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={
          {
            currentMap: answerMap,
            mode: answerMap.mode,
            focusId: answerMap.focus,
            loading: false,
            snapshotStaleReasons: [],
            snapshotSavedAt: "test",
            selectedNode: null,
            selectedEdge: null,
            clearSelection: vi.fn(),
            showMode,
          } as unknown as VisualMapControls
        }
        engineRegistry={null}
        engineError={null}
      />,
    );

    const notice = screen.getByRole("alert");
    expect(notice).toHaveTextContent("이 지도는 프로젝트 전체가 아닙니다");
    expect(notice).toHaveTextContent("3,863개 중 0개 분석");
    // The warning has to precede the canvas in reading order, not sit beside it.
    expect(notice.compareDocumentPosition(container.querySelector(".flow-canvas")!)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
  });

  it("selects a flow box without clearing the current selection", () => {
    const answerMap = map("table-usage", "db:table:public.orders");
    const clearSelection = vi.fn();
    const selectNode = vi.fn();
    const { container } = render(
      <MapWorkspace
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={
          {
            currentMap: answerMap,
            mode: answerMap.mode,
            focusId: answerMap.focus,
            loading: false,
            snapshotStaleReasons: [],
            snapshotSavedAt: "test",
            selectedNode: null,
            selectedEdge: null,
            clearSelection,
            selectNode,
            showMode,
          } as unknown as VisualMapControls
        }
        engineRegistry={null}
        engineError={null}
      />,
    );

    const shell = container.querySelector(".map-app-shell");
    expect(shell).toHaveAttribute("data-inspector", "closed");

    fireEvent.click(screen.getByRole("button", { name: /^db:table:public\.orders / }));
    expect(selectNode).toHaveBeenCalledWith(expect.objectContaining({ id: answerMap.focus }));
    expect(shell).toHaveAttribute("data-inspector", "closed");
    expect(clearSelection).not.toHaveBeenCalled();
  });

  it("opens an inventory target through its package and module on the same canvas", async () => {
    const { container } = render(<TargetDrillHarness />);

    fireEvent.click(screen.getByRole("button", { name: "레이어 열기" }));
    fireEvent.click(screen.getByRole("button", { name: "/api/orders" }));

    await waitFor(() => {
      expect(showMode).toHaveBeenCalledWith("atlas", "group:package:plane");
      expect(showMode).toHaveBeenCalledWith("atlas", "group:module:plane:orders");
      expect(container.querySelector(".flow-canvas-shell")).toHaveAttribute("data-depth", "2");
      expect(container.querySelector(".map-app-shell")).toHaveAttribute("data-inspector", "open");
    });
    expect(screen.getByRole("navigation", { name: "구조 위치" })).toHaveTextContent("전체 프로젝트Planeorders");
  });
});

function TargetDrillHarness() {
  const areaId = "group:package:plane";
  const moduleId = "group:module:plane:orders";
  const routeId = "code:route-orders";
  const area = {
    id: areaId,
    kind: "group-domain",
    title: "Plane",
    layer: "mixed",
    source: "architecture",
    parentId: null,
    depth: 0,
    metrics: { apiCount: 1, codeCount: 0, dbCount: 0, topApi: ["/api/orders"], topCode: [], topDb: [] },
  };
  const module = {
    id: moduleId,
    kind: "group-domain",
    title: "orders",
    layer: "mixed",
    source: "architecture",
    parentId: areaId,
    depth: 1,
    location: { path: "apps/api/plane/orders", line: null },
  };
  const route = {
    id: routeId,
    kind: "route",
    title: "/api/orders",
    layer: "api",
    source: "code",
    location: { path: "apps/api/plane/orders/routes.ts", line: 12 },
  };
  const overview = { ...map("atlas", "overview"), nodes: [area] } as VisualMap;
  const areaMap = { ...overview, focus: areaId, nodes: [area, module] } as VisualMap;
  const moduleMap = { ...overview, focus: moduleId, nodes: [module, route] } as VisualMap;
  const [currentMap, setCurrentMap] = useState(overview);
  const [selectedNode, setSelectedNode] = useState<typeof route | null>(null);
  const controls = {
    currentMap,
    mode: "atlas",
    focusId: currentMap.focus,
    loading: false,
    snapshotStaleReasons: [],
    snapshotSavedAt: "test",
    selectedNode,
    selectedEdge: null,
    clearSelection: () => setSelectedNode(null),
    selectNode: (node: typeof route) => setSelectedNode(node),
    showMode: (mode: string, focusId?: string | null) => {
      showMode(mode, focusId);
      setCurrentMap(focusId === areaId ? areaMap : focusId === moduleId ? moduleMap : overview);
    },
  } as unknown as VisualMapControls;

  return (
    <MapWorkspace
      sourceManagerOpen={false}
      setSourceManagerOpen={vi.fn()}
      workspaceControls={workspaceControls({
        project: "orders",
        routes: [
          {
            id: "route-orders",
            kind: "api",
            name: "/api/orders",
            filePath: "apps/api/plane/orders/routes.ts",
            line: 12,
            detail: null,
          },
        ],
        services: [],
        handlers: [],
        repositories: [],
        functions: [],
        classes: [],
        modules: [],
        unknown: [],
        files: [],
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
      })}
      dbProfileControls={{ inventory: null } as DbProfileControls}
      visualMapControls={controls}
      engineRegistry={null}
      engineError={null}
    />
  );
}

function Harness() {
  const answerMap = map("api-flow", "code:route-orders");
  const advancedMap = map("atlas", "overview");
  const [visual, setVisual] = useState<{
    currentMap: VisualMap;
    mode: string;
    focusId: string | null;
    loading: boolean;
  }>({ currentMap: answerMap, mode: "api-flow", focusId: answerMap.focus, loading: false });
  const controls = {
    ...visual,
    snapshotStaleReasons: [],
    snapshotSavedAt: "test",
    selectedNode: null,
    selectedEdge: null,
    showMode: (mode: string, focusId?: string | null) => {
      showMode(mode, focusId);
      setVisual((current) => ({ ...current, mode, focusId: focusId ?? null, loading: true }));
    },
    clearSelection: vi.fn(),
  } as unknown as VisualMapControls;

  return (
    <>
      <MapWorkspace
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={controls}
        engineRegistry={null}
        engineError={null}
      />
      <button
        type="button"
        onClick={() => setVisual({ currentMap: advancedMap, mode: "atlas", focusId: null, loading: false })}
      >
        전체 구조 커밋
      </button>
      {/* Stands in for clicking a node on the atlas canvas. */}
      <button type="button" onClick={() => controls.showMode("api-flow", answerMap.focus)}>
        대상 선택
      </button>
      <button
        type="button"
        onClick={() => setVisual({ currentMap: answerMap, mode: "api-flow", focusId: answerMap.focus, loading: false })}
      >
        답 커밋
      </button>
    </>
  );
}

function workspaceControls(codeInventory: unknown = null): WorkspaceControls {
  return {
    initialized: true,
    currentWorkspace: { id: "workspace-1", name: "Orders" },
    codeInventory,
    operationStatus: { phase: "idle", label: "준비", message: "준비됨" },
  } as unknown as WorkspaceControls;
}

/** A Kafka-shaped index: the Java side never reached the map, the tooling scripts did. */
function partiallyIndexedInventory(): unknown {
  return {
    project: "kafka",
    routes: [],
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
      routes: 0,
      handlers: 0,
      services: 0,
      repositories: 0,
      functions: 21,
      classes: 0,
      modules: 0,
      files: 21,
      unknown: 0,
    },
    architecture: {
      languages: [
        {
          id: "java",
          name: "Java",
          provider: "native-lsp",
          files_found: 3863,
          files_indexed: 0,
          files_excluded: 3863,
          files_missing: 0,
          status: "excluded",
        },
        {
          id: "python",
          name: "Python",
          provider: "native-lsp",
          files_found: 21,
          files_indexed: 21,
          files_excluded: 0,
          files_missing: 0,
          status: "indexed",
        },
      ],
    },
  };
}

function map(mode: string, focus: string): VisualMap {
  return {
    id: `${mode}:${focus}`,
    workspaceId: "workspace-1",
    mode,
    focus,
    nodes: [
      {
        id: focus,
        kind: mode === "api-flow" ? "api" : "target",
        title: focus === "code:route-orders" ? "GET /orders" : focus,
        layer: "code",
        source: "test",
      },
    ],
    edges: [],
    warnings: [],
  };
}
