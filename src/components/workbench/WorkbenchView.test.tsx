import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualMap } from "../../types/visual-map";
import { WorkbenchView } from "./WorkbenchView";

vi.mock("../atlas/AtlasCanvas", () => ({ AtlasCanvas: () => <main data-testid="advanced-surface" /> }));
vi.mock("./AnswerCanvas", () => ({
  AnswerCanvas: ({ onOpenEvidence }: { onOpenEvidence: () => void }) => (
    <main data-testid="answer-surface">
      <button type="button" onClick={onOpenEvidence}>근거 요청</button>
    </main>
  ),
}));
vi.mock("./InspectorPanel", () => ({
  InspectorPanel: ({ onClose }: { onClose?: () => void }) => (
    <div>
      <button type="button" onClick={onClose}>근거 닫기</button>
    </div>
  ),
}));
vi.mock("./ModePanel", () => ({ ModePanel: () => <nav data-testid="advanced-navigation" /> }));
vi.mock("./TargetNavigator", () => ({ TargetNavigator: () => <nav data-testid="answer-navigation" /> }));
vi.mock("./WorkbenchLeftPanel", () => ({ WorkbenchLeftPanel: () => <div /> }));
vi.mock("./WorkbenchStatusBar", () => ({ WorkbenchStatusBar: () => <div /> }));
vi.mock("./WorkbenchTopBar", () => ({
  WorkbenchTopBar: ({ surface, onShowAnswers, onShowAdvanced }: {
    surface?: string;
    onShowAnswers?: () => void;
    onShowAdvanced?: () => void;
  }) => (
    <header>
      <span data-testid="requested-surface">{surface}</span>
      <button type="button" onClick={onShowAnswers}>답 요청</button>
      <button type="button" onClick={onShowAdvanced}>전체 구조 요청</button>
    </header>
  ),
}));

const showMode = vi.fn();

describe("WorkbenchView surface transitions", () => {
  beforeEach(() => showMode.mockClear());

  it("keeps empty target navigation out of the first-run setup", () => {
    const { container } = render(
      <WorkbenchView
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={{
          initialized: true,
          currentWorkspace: null,
          operationStatus: { phase: "idle", label: "준비", message: "준비됨" },
        } as unknown as WorkspaceControls}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={{
          currentMap: null,
          mode: "atlas",
          focusId: null,
          loading: false,
          snapshotStaleReasons: [],
          selectedNode: null,
          selectedEdge: null,
          clearSelection: vi.fn(),
          showMode: vi.fn(),
        } as unknown as VisualMapControls}
        engineRegistry={null}
        engineError={null}
      />,
    );

    expect(screen.getByRole("heading", { name: "프로젝트를 연결하세요" })).toBeInTheDocument();
    expect(screen.queryByTestId("answer-navigation")).not.toBeInTheDocument();
    expect(screen.queryByTestId("advanced-navigation")).not.toBeInTheDocument();
    expect(container.querySelector(".product-workspace")).toHaveClass("is-single-column");
  });

  it.each([
    ["the project opens", true, false],
    ["the saved answer restores", false, true],
  ])("keeps project content hidden while %s", (_phase, opening, restoringSnapshot) => {
    const { container } = render(
      <WorkbenchView
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={{
          initialized: true,
          currentWorkspace: { id: "workspace-1", name: "Orders" },
          opening,
          restoringSnapshot,
          codeInventory: null,
          operationStatus: { phase: "running", label: "저장 결과 확인", message: "저장 결과 확인 진행 중" },
        } as unknown as WorkspaceControls}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={{
          currentMap: null,
          mode: "atlas",
          focusId: null,
          loading: false,
          snapshotStaleReasons: [],
          selectedNode: null,
          selectedEdge: null,
          clearSelection: vi.fn(),
          showMode: vi.fn(),
        } as unknown as VisualMapControls}
        engineRegistry={null}
        engineError={null}
      />,
    );

    expect(screen.getByText("프로젝트 분석을 불러오고 있습니다")).toBeInTheDocument();
    expect(screen.queryByTestId("answer-navigation")).not.toBeInTheDocument();
    expect(screen.queryByTestId("advanced-navigation")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "프로젝트를 연결하세요" })).not.toBeInTheDocument();
    expect(container.querySelector(".product-workspace")).toHaveClass("is-single-column");
  });

  it("keeps the committed layout until the requested map commits", async () => {
    render(<Harness />);

    expect(screen.getByTestId("answer-surface")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("답 준비 완료: GET /orders");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "ready");
    fireEvent.click(screen.getByRole("button", { name: "전체 구조 요청" }));

    expect(showMode).toHaveBeenLastCalledWith("atlas", null);
    expect(screen.getByTestId("requested-surface")).toHaveTextContent("advanced");
    expect(screen.getByTestId("answer-surface")).toBeInTheDocument();
    expect(screen.queryByTestId("advanced-surface")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "idle");

    fireEvent.click(screen.getByRole("button", { name: "전체 구조 커밋" }));
    await waitFor(() => expect(screen.getByTestId("advanced-surface")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "답 요청" }));
    expect(showMode).toHaveBeenLastCalledWith("api-flow", "code:route-orders");
    expect(screen.getByTestId("requested-surface")).toHaveTextContent("answers");
    expect(screen.getByTestId("advanced-surface")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("선택한 대상 분석 중");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "loading");

    fireEvent.click(screen.getByRole("button", { name: "답 커밋" }));
    await waitFor(() => expect(screen.getByTestId("answer-surface")).toBeInTheDocument());
    expect(screen.getByRole("status")).toHaveTextContent("답 준비 완료: GET /orders");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "ready");
  });

  it("opens the evidence drawer without selecting the focused node", () => {
    const answerMap = map("table-usage", "db:table:public.orders");
    const clearSelection = vi.fn();
    const { container } = render(
      <WorkbenchView
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={{
          currentMap: answerMap,
          mode: answerMap.mode,
          focusId: answerMap.focus,
          loading: false,
          snapshotStaleReasons: [],
          selectedNode: null,
          selectedEdge: null,
          clearSelection,
          showMode,
        } as unknown as VisualMapControls}
        engineRegistry={null}
        engineError={null}
      />,
    );

    const workspace = container.querySelector(".product-workspace");
    expect(workspace).not.toHaveClass("inspector-visible");

    fireEvent.click(screen.getByRole("button", { name: "근거 요청" }));
    expect(workspace).toHaveClass("inspector-visible");
    expect(clearSelection).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "근거 닫기" }));
    expect(workspace).not.toHaveClass("inspector-visible");
    expect(clearSelection).toHaveBeenCalledOnce();
  });
});

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
    snapshotSavedAt: null,
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
      <WorkbenchView
        sourceManagerOpen={false}
        setSourceManagerOpen={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={controls}
        engineRegistry={null}
        engineError={null}
      />
      <button type="button" onClick={() => setVisual({ currentMap: advancedMap, mode: "atlas", focusId: null, loading: false })}>
        전체 구조 커밋
      </button>
      <button type="button" onClick={() => setVisual({ currentMap: answerMap, mode: "api-flow", focusId: answerMap.focus, loading: false })}>
        답 커밋
      </button>
    </>
  );
}

function workspaceControls(): WorkspaceControls {
  return {
    initialized: true,
    currentWorkspace: { id: "workspace-1", name: "Orders" },
    codeInventory: null,
    operationStatus: { phase: "idle", label: "준비", message: "준비됨" },
  } as unknown as WorkspaceControls;
}

function map(mode: string, focus: string): VisualMap {
  return {
    id: `${mode}:${focus}`,
    workspaceId: "workspace-1",
    mode,
    focus,
    nodes: [{
      id: focus,
      kind: mode === "api-flow" ? "api" : "target",
      title: focus === "code:route-orders" ? "GET /orders" : focus,
      layer: "code",
      source: "test",
    }],
    edges: [],
    warnings: [],
  };
}
