import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualMap } from "../../types/visual-map";
import { WorkbenchView } from "./WorkbenchView";

vi.mock("../atlas/AtlasCanvas", () => ({ AtlasCanvas: () => <main data-testid="advanced-surface" /> }));
vi.mock("./AnswerCanvas", () => ({ AnswerCanvas: () => <main data-testid="answer-surface" /> }));
vi.mock("./InspectorPanel", () => ({ InspectorPanel: () => <div /> }));
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
    expect(container.querySelector(".product-workspace")).toHaveClass("is-onboarding");
  });

  it("keeps the committed layout until the requested map commits", async () => {
    render(<Harness />);

    expect(screen.getByTestId("answer-surface")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "전체 구조 요청" }));

    expect(showMode).toHaveBeenLastCalledWith("atlas", null);
    expect(screen.getByTestId("requested-surface")).toHaveTextContent("advanced");
    expect(screen.getByTestId("answer-surface")).toBeInTheDocument();
    expect(screen.queryByTestId("advanced-surface")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "전체 구조 커밋" }));
    await waitFor(() => expect(screen.getByTestId("advanced-surface")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "답 요청" }));
    expect(showMode).toHaveBeenLastCalledWith("api-flow", "code:route-orders");
    expect(screen.getByTestId("requested-surface")).toHaveTextContent("answers");
    expect(screen.getByTestId("advanced-surface")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "답 커밋" }));
    await waitFor(() => expect(screen.getByTestId("answer-surface")).toBeInTheDocument());
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
    nodes: [],
    edges: [],
    warnings: [],
  };
}
