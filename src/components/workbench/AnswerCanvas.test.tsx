import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory } from "../../types/workspace";
import type { VisualMap } from "../../types/visual-map";
import { AnswerCanvas } from "./AnswerCanvas";

describe("AnswerCanvas", () => {
  it("shows the confirmed API path before collapsed candidates", () => {
    const map = apiMap();
    const { container } = renderAnswer(map, "code:route-orders");

    expect(container.querySelector(".answer-canvas")).toHaveAttribute("data-answer-mode", "api-flow");
    expect(container.querySelector(".answer-canvas")).toHaveAttribute("data-answer-focus", "code:route-orders");
    expect(screen.getByRole("heading", { name: "DELETE /api/orders" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "처리 흐름" })).toBeInTheDocument();
    expect(screen.getByText("loadOrders")).toBeInTheDocument();
    expect(container.querySelector(".answer-candidates")).not.toHaveAttribute("open");
    expect(screen.getByText("orders 테이블 후보")).toBeInTheDocument();
  });

  it("opens target evidence from the answer header", () => {
    const map = apiMap();
    const visualMapControls = controls(map, map.mode, map.focus);
    render(
      <AnswerCanvas
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={visualMapControls}
        onOpenSources={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "근거 패널 열기" }));

    expect(visualMapControls.selectNode).toHaveBeenCalledWith(map.nodes[0]);
  });

  it("states that a code target has no confirmed relationship without inventing one", () => {
    renderAnswer(codeMap(), "code:function-load-orders");

    expect(screen.getByRole("heading", { name: "loadOrders" })).toBeInTheDocument();
    expect(screen.getByText("확인된 연결은 없습니다.")).toBeInTheDocument();
    expect(screen.getByText("확인된 관계가 없습니다")).toBeInTheDocument();
  });

  it("does not promote a structural edge with evidence to confirmed", () => {
    const map = codeMap();
    map.nodes.push({ id: "code:file-orders", kind: "file", title: "orders.ts", layer: "code", source: "code" });
    map.edges.push({
      id: "contains-orders",
      from: "code:file-orders",
      to: "code:function-load-orders",
      kind: "contains",
      evidence: [{ kind: "engine", text: "파일에 포함된 심볼입니다." }],
    });

    const { container } = renderAnswer(map, "code:function-load-orders");

    expect(container.querySelector(".answer-verdicts .confirmed")).toHaveTextContent("확정 0");
    expect(container.querySelector(".answer-verdicts .structural")).toHaveTextContent("구조 1");
    expect(container.querySelector(".answer-edge-items .answer-truth.structural")).toHaveTextContent("구조");
    expect(screen.getByText("확정 연결은 없으며 구조 근거 1개를 찾았습니다.")).toBeInTheDocument();
  });

  it("separates structural facts and discloses engine-truncated evidence", () => {
    const { container } = renderAnswer(tableMap(), "db:table:public.orders");

    expect(screen.getByText("확인된 코드 사용")).toBeInTheDocument();
    expect(screen.getByText("DB 구조 근거")).toBeInTheDocument();
    expect(screen.getByText("직접 근거 2개는 엔진 표시 상한 때문에 이 답에서 접혔습니다.")).toBeInTheDocument();
    expect(container.querySelector(".answer-verdicts .confirmed")).toHaveTextContent("확정 0");
    expect(container.querySelector(".answer-verdicts .structural")).toHaveTextContent("구조 1");
    expect(container.querySelector(".answer-truth.structural")).toBeInTheDocument();
  });

  it("does not label unknown coverage gaps as candidates", () => {
    const map = tableMap();
    const unknownLane = map.reviewBoard!.lanes.find((lane) => lane.id === "unknowns")!;
    unknownLane.total = 1;
    unknownLane.items = [{
      id: "missing-code-evidence",
      nodeId: null,
      kind: "code-gap",
      title: "코드 영향 미확인",
      detail: "직접 코드 근거가 없습니다.",
      truthClass: "unknown",
      rank: 1,
      evidence: [],
    }];

    const { container } = renderAnswer(map, "db:table:public.orders");
    expect(container.querySelector(".answer-verdicts .unknown")).toHaveTextContent("확인 필요 1");
    expect(container.querySelector(".answer-verdicts .candidate")).not.toBeInTheDocument();
    expect(screen.getByText("확인되지 않은 구간")).toBeInTheDocument();
  });

  it("uses a quiet project start instead of opening the full atlas by default", () => {
    render(
      <AnswerCanvas
        workspaceControls={workspaceControls()}
        dbProfileControls={{ inventory: null } as DbProfileControls}
        visualMapControls={controls(null, "atlas", null)}
        onOpenSources={vi.fn()}
      />,
    );

    expect(screen.getByRole("heading", { name: "Orders" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /대상 검색/ })).toBeInTheDocument();
    expect(screen.queryByText("빠른 시작")).not.toBeInTheDocument();
    expect(screen.queryByText("loadOrders")).not.toBeInTheDocument();
    expect(screen.queryByText("전체 구조")).not.toBeInTheDocument();
  });
});

function renderAnswer(map: VisualMap, focusId: string) {
  return render(
    <AnswerCanvas
      workspaceControls={workspaceControls()}
      dbProfileControls={{ inventory: null } as DbProfileControls}
      visualMapControls={controls(map, map.mode, focusId)}
      onOpenSources={vi.fn()}
    />,
  );
}

function controls(map: VisualMap | null, mode: string, focusId: string | null): VisualMapControls {
  return {
    currentMap: map,
    mode,
    focusId,
    loading: false,
    enriching: false,
    snapshotStaleReasons: [],
    selectedNode: null,
    selectedEdge: null,
    changeIntent: { kind: "rename", value: null },
    openSearchPopover: vi.fn(),
    showMode: vi.fn(),
    selectNode: vi.fn(),
    selectEdge: vi.fn(),
    setChangeIntent: vi.fn(),
  } as unknown as VisualMapControls;
}

function workspaceControls(): WorkspaceControls {
  return {
    currentWorkspace: { id: "workspace-1", name: "Orders" },
    codeInventory: codeInventory(),
    operationStatus: { phase: "idle", label: "준비", message: "준비됨" },
  } as unknown as WorkspaceControls;
}

function codeInventory(): CodeInventory {
  return {
    project: "orders",
    routes: [{ id: "route-orders", kind: "api", name: "/api/orders", filePath: "src/routes.ts", line: 12, detail: null }],
    services: [],
    handlers: [],
    repositories: [],
    functions: [{ id: "function-load-orders", kind: "function", name: "loadOrders", filePath: "src/orders.ts", line: 23, detail: null }],
    classes: [],
    modules: [],
    unknown: [],
    files: [],
    calls: [],
    summary: { routes: 1, handlers: 0, services: 0, repositories: 0, functions: 1, classes: 0, modules: 0, files: 0, unknown: 0 },
  };
}

function apiMap(): VisualMap {
  return {
    id: "api-orders",
    workspaceId: "workspace-1",
    mode: "api-flow",
    focus: "code:route-orders",
    warnings: [],
    nodes: [
      { id: "code:route-orders", kind: "api", title: "/api/orders", layer: "api", source: "code" },
      { id: "code:function-load-orders", kind: "function", title: "loadOrders", layer: "code", source: "code" },
    ],
    edges: [],
    apiReading: {
      subject: "/api/orders",
      method: "DELETE",
      steps: [
        {
          id: "route-step",
          nodeId: "code:route-orders",
          kind: "api",
          title: "/api/orders",
          detail: "src/routes.ts:12",
          truthClass: "confirmed",
          rank: 1,
          evidence: [],
          depth: 0,
          lane: "route",
          laneBasis: "engine-node",
          incomingEvidence: [],
        },
        {
          id: "function-step",
          nodeId: "code:function-load-orders",
          kind: "function",
          title: "loadOrders",
          detail: "src/orders.ts:23",
          truthClass: "confirmed",
          rank: 2,
          evidence: [],
          depth: 1,
          lane: "service-function",
          laneBasis: "confirmed-handles",
          incomingEvidence: [],
        },
      ],
      dbRelations: [],
      dbCandidates: [{
        id: "orders-candidate",
        nodeId: null,
        kind: "candidate",
        title: "orders 테이블 후보",
        detail: "이름 단서만 확인했습니다.",
        truthClass: "candidate",
        rank: 1,
        evidence: [],
      }],
      unknowns: [],
      recommendedChecks: [],
      hiddenBranches: 0,
      truncated: false,
    },
  };
}

function codeMap(): VisualMap {
  return {
    id: "code-orders",
    workspaceId: "workspace-1",
    mode: "search-focus",
    focus: "code:function-load-orders",
    nodes: [{ id: "code:function-load-orders", kind: "function", title: "loadOrders", layer: "code", source: "code" }],
    edges: [],
    warnings: [],
  };
}

function tableMap(): VisualMap {
  return {
    id: "table-orders",
    workspaceId: "workspace-1",
    mode: "table-usage",
    focus: "db:table:public.orders",
    nodes: [
      { id: "db:table:public.orders", kind: "table", title: "orders", layer: "database", source: "database" },
      { id: "db:constraint:orders-pk", kind: "constraint", title: "orders_pkey", layer: "database", source: "database" },
    ],
    edges: [],
    warnings: [],
    reviewBoard: {
      subject: "orders",
      scope: "table",
      lanes: [
        {
          id: "direct",
          order: 1,
          title: "직접 영향",
          description: "DB 구조",
          tone: "confirmed",
          total: 3,
          hidden: 2,
          emptyMessage: "없음",
          items: [{
            id: "orders-pk",
            nodeId: "db:constraint:orders-pk",
            kind: "primary-key",
            title: "orders_pkey",
            detail: "PK · id",
            truthClass: "structural",
            rank: 1,
            evidence: [],
          }],
        },
        { id: "candidates", order: 2, title: "후보", description: "후보", tone: "candidate", total: 0, hidden: 0, emptyMessage: "없음", items: [] },
        { id: "unknowns", order: 3, title: "확인", description: "확인", tone: "unknown", total: 0, hidden: 0, emptyMessage: "없음", items: [] },
        { id: "checks", order: 4, title: "권장", description: "권장", tone: "action", total: 0, hidden: 0, emptyMessage: "없음", items: [] },
      ],
      markdownSummary: "orders",
    },
  };
}
