import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { CodeInventory } from "../../types/workspace";
import type { VisualMap } from "../../types/visual-map";
import { AnswerCanvas } from "./AnswerCanvas";

describe("AnswerCanvas", () => {
  it("shows the confirmed API path before collapsed candidates", () => {
    const map = apiMap();
    const { container } = renderAnswer(map, "code:route-orders");

    expect(screen.getByRole("heading", { name: "DELETE /api/orders" })).toBeInTheDocument();
    expect(screen.getByText("확인된 처리 흐름")).toBeInTheDocument();
    expect(screen.getByText("loadOrders")).toBeInTheDocument();
    expect(container.querySelector(".answer-candidates")).not.toHaveAttribute("open");
    expect(screen.getByText("orders 테이블 후보")).toBeInTheDocument();
  });

  it("states that a code target has no confirmed relationship without inventing one", () => {
    renderAnswer(codeMap(), "code:function-load-orders");

    expect(screen.getByRole("heading", { name: "loadOrders" })).toBeInTheDocument();
    expect(screen.getByText("확인된 직접 연결은 없습니다.")).toBeInTheDocument();
    expect(screen.getByText("확인된 직접 관계가 없습니다")).toBeInTheDocument();
  });

  it("separates structural facts and discloses engine-truncated evidence", () => {
    const { container } = renderAnswer(tableMap(), "db:table:public.orders");

    expect(screen.getByText("확인된 코드 사용")).toBeInTheDocument();
    expect(screen.getByText("DB 구조 근거")).toBeInTheDocument();
    expect(screen.getByText("직접 근거 2개는 엔진 표시 상한 때문에 이 답에서 접혔습니다.")).toBeInTheDocument();
    expect(screen.getByText("구조")).toHaveClass("structural");
    expect(container.querySelector(".answer-truth.structural")).toBeInTheDocument();
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
