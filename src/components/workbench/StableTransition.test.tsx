import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualMap } from "../../types/visual-map";
import { AtlasCanvas } from "../atlas/AtlasCanvas";
import { CodeSourceSection } from "./CodeSourceSection";
import { DatabaseSourceSection } from "./DatabaseSourceSection";
import { InspectorPanel } from "./InspectorPanel";
import { WorkbenchTopBar } from "./WorkbenchTopBar";

describe("stable mode transitions", () => {
  it("keeps the selected mode structure visible while the canvas reloads", () => {
    render(
      <AtlasCanvas
        openSourceManager={vi.fn()}
        workspaceControls={{ codeInventory: null } as unknown as WorkspaceControls}
        dbProfileControls={{ inventory: null } as unknown as DbProfileControls}
        visualMapControls={loadingControls("api-flow")}
      />,
    );

    expect(screen.getByText("API 읽기 경로")).toBeInTheDocument();
    expect(screen.getByText("Route")).toBeInTheDocument();
    expect(screen.getByText("DB 후보")).toBeInTheDocument();
    expect(screen.getByText("새 근거 구성 중")).toBeInTheDocument();
  });

  it("keeps the evidence sections mounted while their values reload", () => {
    render(
      <InspectorPanel
        workspaceControls={{} as WorkspaceControls}
        dbProfileControls={{} as DbProfileControls}
        visualMapControls={loadingControls("column-impact")}
      />,
    );

    expect(screen.getByText("선택한 대상")).toBeInTheDocument();
    expect(screen.getByText("컬럼 변경 영향")).toBeInTheDocument();
    expect(screen.getByText("바로 연결")).toBeInTheDocument();
    expect(screen.getByText("근거")).toBeInTheDocument();
  });

  it("keeps the previous same-mode answer visible but clearly disabled while a new target loads", () => {
    const previousMap = visualMap("search-focus", "code:function-old");
    const controls = loadingControls("search-focus", previousMap);
    const { container } = render(
      <AtlasCanvas
        openSourceManager={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={controls}
      />,
    );

    expect(container.querySelector(".at-canvas.is-refreshing")).toBeInTheDocument();
    expect(screen.getByText(/이전 결과 표시/)).toBeInTheDocument();
  });

  it("keeps the previous inspector mounted during a same-mode refresh", () => {
    const controls = loadingControls("search-focus", visualMap("search-focus", "code:function-old"));
    const { container } = render(
      <InspectorPanel
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={controls}
      />,
    );

    expect(container.querySelector(".inspector.is-refreshing")).toBeInTheDocument();
    expect(screen.getByText("새 대상 분석 중 · 이전 근거 표시")).toBeInTheDocument();
  });

  it("shows a neutral target prompt instead of choosing an item on first mode entry", () => {
    render(
      <AtlasCanvas
        openSourceManager={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={{
          ...loadingControls("api-flow", visualMap("api-flow", "narrow-focus")),
          focusId: null,
          loading: false,
        }}
      />,
    );

    expect(screen.getByText("확인할 API 라우트를 선택하세요")).toBeInTheDocument();
    expect(screen.getByText(/왼쪽 API 라우트 목록/)).toBeInTheDocument();
  });

  it("keeps every disconnected composition subject neutral until the user selects a card", () => {
    const map: VisualMap = {
      ...visualMap("composition", "code:function-old"),
      nodes: [
        { id: "code:function-old", kind: "function", title: "oldFunction", subtitle: "src/old.ts", layer: "code", source: "code" },
        { id: "db:table:public.users", kind: "table", title: "users", subtitle: "public", layer: "database", source: "db" },
      ],
    };
    const controls = {
      ...readyControls(map),
      focusId: null,
      compositionFocusIds: ["code:function-old", "db:table:public.users"],
      relationView: "data",
      toggleCompositionFocus: vi.fn(),
      clearCompositionFocus: vi.fn(),
      setRelationView: vi.fn(),
    } as VisualMapControls;
    const db = {
      ...dbProfileControls(),
      inventory: {
        tables: [{
          schema: "public",
          name: "users",
          columns: [{ name: "id", dataType: "uuid", isPrimaryKey: true, isForeignKey: false }],
        }],
      },
    } as DbProfileControls;
    const canvas = render(
      <AtlasCanvas
        openSourceManager={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={db}
        visualMapControls={controls}
      />,
    );

    expect(canvas.container.querySelector(".at-map-surface.has-relation-focus")).not.toBeInTheDocument();
    expect(canvas.container.querySelector(".at-card.code")).toBeInTheDocument();
    expect(canvas.container.querySelector(".at-card.table")).toBeInTheDocument();
    canvas.unmount();

    render(
      <InspectorPanel
        workspaceControls={workspaceControls()}
        dbProfileControls={db}
        visualMapControls={controls}
      />,
    );
    expect(screen.getByText(/관계는 아직 없고 실제 항목 2개가 있습니다/)).toBeInTheDocument();
    expect(screen.getByText("관계 분석")).toBeInTheDocument();
    expect(screen.queryByText("oldFunction")).not.toBeInTheDocument();
  });

  it("uses the requested analysis target when a narrow projection has no focused node", () => {
    render(
      <AtlasCanvas
        openSourceManager={vi.fn()}
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={{
          ...loadingControls("search-focus", visualMap("search-focus", "narrow-focus")),
          focusId: "code:function-old",
          loading: false,
        }}
      />,
    );

    expect(screen.getAllByText("oldFunction").length).toBeGreaterThan(0);
    expect(screen.queryByText("확인할 코드 항목을 선택하세요")).not.toBeInTheDocument();
  });

  it("separates unrelated nearby code from a focused target with no relationships", () => {
    const workspace = workspaceControls();
    workspace.codeInventory!.functions.push({
      id: "function-nearby",
      kind: "function",
      name: "nearbyFunction",
      filePath: "src/nearby.ts",
      line: 8,
      detail: null,
    });
    const currentMap = codeMap();
    currentMap.nodes.push({
      id: "code:function-nearby",
      kind: "function",
      title: "nearbyFunction",
      subtitle: "src/nearby.ts",
      layer: "code",
      source: "code",
    });

    const { container } = render(
      <AtlasCanvas
        openSourceManager={vi.fn()}
        workspaceControls={workspace}
        dbProfileControls={dbProfileControls()}
        visualMapControls={readyControls(currentMap)}
      />,
    );

    expect(container.querySelector(".at-disconnected-focus")).toBeInTheDocument();
    expect(screen.getByText("확인된 직접 관계가 없습니다")).toBeInTheDocument();
    expect(screen.queryByText("nearbyFunction")).not.toBeInTheDocument();
    expect(screen.getByText(/1개 항목은 관계 근거가 없어 지도에서 분리했습니다/)).toBeInTheDocument();
  });

  it("uses the requested analysis target as the default inspector subject", () => {
    const currentMap = visualMap("search-focus", "narrow-focus");
    const { container } = render(
      <InspectorPanel
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={{ ...readyControls(currentMap), focusId: "code:function-old" }}
      />,
    );

    expect(screen.getAllByText("oldFunction").length).toBeGreaterThan(0);
    expect(Array.from(container.querySelectorAll(".inspector-section > header > strong")).map((node) => node.textContent))
      .toEqual(["요약", "바로 연결", "근거", "소스", "다음 확인"]);
  });

  it("keeps source actions for a map node omitted from the bounded inventory", () => {
    const currentMap = visualMap("search-focus", "code:function-outside-bootstrap");
    currentMap.nodes = [{
      id: "code:function-outside-bootstrap",
      kind: "function",
      title: "outsideBootstrap",
      subtitle: "src/outside.ts",
      layer: "code",
      source: "code",
      location: { path: "src/outside.ts", line: 42, column: 3 },
    }];

    render(
      <InspectorPanel
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={{ ...readyControls(currentMap), selectedNode: currentMap.nodes[0] }}
      />,
    );

    expect(screen.getByText("function · 42행")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "VS Code" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cursor" })).toBeInTheDocument();
  });

  it("opens source management from the stale source status", () => {
    const onToggleSourceManager = vi.fn();
    render(
      <WorkbenchTopBar
        sourceManagerOpen={false}
        onToggleSourceManager={onToggleSourceManager}
        workspaceControls={{
          initialized: true,
          busy: false,
          currentWorkspace: { id: "workspace-1", name: "Shop API" },
          workspaces: [{ id: "workspace-1", name: "Shop API" }],
          codeInventory: null,
          operationStatus: { phase: "idle", label: "작업 없음", message: "실행 중인 작업 없음" },
          openWorkspace: vi.fn(),
        } as unknown as WorkspaceControls}
        dbProfileControls={{ inventory: null } as unknown as DbProfileControls}
        visualMapControls={{
          searchQuery: "",
          searchGroups: [],
          snapshotStaleReasons: ["코드 파일이 마지막 읽기 이후 바뀌었습니다"],
          setSearchQuery: vi.fn(),
          openSearchPopover: vi.fn(),
          closeSearchPopover: vi.fn(),
          runSearch: vi.fn(),
          selectSearchResult: vi.fn(),
        } as unknown as VisualMapControls}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "다시 읽기 필요" }));

    expect(onToggleSourceManager).toHaveBeenCalledOnce();
  });

  it("lists each project once in the project switcher", () => {
    const openWorkspace = vi.fn();
    render(
      <WorkbenchTopBar
        sourceManagerOpen={false}
        onToggleSourceManager={vi.fn()}
        workspaceControls={{
          initialized: true,
          busy: false,
          currentWorkspace: { id: "workspace-1", name: "Shop API" },
          workspaces: [
            { id: "workspace-1", name: "Shop API" },
            { id: "workspace-2", name: "Billing API" },
          ],
          codeInventory: null,
          operationStatus: { phase: "idle", label: "작업 없음", message: "실행 중인 작업 없음" },
          openWorkspace,
        } as unknown as WorkspaceControls}
        dbProfileControls={{ inventory: null } as unknown as DbProfileControls}
        visualMapControls={{
          searchQuery: "",
          searchGroups: [],
          snapshotStaleReasons: [],
          setSearchQuery: vi.fn(),
          openSearchPopover: vi.fn(),
          closeSearchPopover: vi.fn(),
          runSearch: vi.fn(),
          selectSearchResult: vi.fn(),
        } as unknown as VisualMapControls}
      />,
    );

    expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
      "Shop API",
      "Billing API",
    ]);
    fireEvent.change(screen.getByRole("combobox", { name: "프로젝트" }), {
      target: { value: "workspace-2" },
    });
    expect(openWorkspace).toHaveBeenCalledWith("workspace-2");
  });

  it("keeps code re-reading visible without expanding project details", () => {
    const indexCodeRepository = vi.fn();
    const { container } = render(
      <CodeSourceSection
        workspaceControls={{
          ...workspaceWithApi(),
          busy: false,
          canIndexCode: true,
          codeIndexing: false,
          indexCodeRepository,
        }}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "다시 읽기" }));

    expect(indexCodeRepository).toHaveBeenCalledOnce();
    expect(container.querySelector("details.source-advanced")).not.toHaveAttribute("open");
  });

  it("keeps database re-reading visible without expanding connection details", () => {
    const indexProfile = vi.fn();
    const { container } = render(
      <DatabaseSourceSection
        dbProfileControls={{
          ...dbControlsWithUsers(),
          hasWorkspace: true,
          activeProfile: {
            id: "profile-1",
            name: "Main DB",
            source: "ddl-sqlite",
            path: "D:\\schema.sql",
            cachePath: "D:\\cache\\db.json",
            passwordStored: false,
          },
          profileName: "Main DB",
          profileSource: "ddl-sqlite",
          profilePath: "D:\\schema.sql",
          connectionString: "",
          status: null,
          error: null,
          errorDetail: null,
          busy: false,
          saving: false,
          indexing: false,
          deleting: false,
          canSaveProfile: false,
          canIndexProfile: true,
          dbIndexBlockedReason: null,
          setProfileName: vi.fn(),
          setProfileSource: vi.fn(),
          setProfilePath: vi.fn(),
          setConnectionString: vi.fn(),
          pickPath: vi.fn(),
          saveProfile: vi.fn(),
          indexProfile,
          deleteProfile: vi.fn(),
          openTable: vi.fn(),
          openColumn: vi.fn(),
        }}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "다시 읽기" }));

    expect(indexProfile).toHaveBeenCalledOnce();
    expect(container.querySelector("details.source-advanced")).not.toHaveAttribute("open");
  });

  it.each([
    ["api-flow", apiMap()],
    ["search-focus", codeMap()],
    ["table-usage", dbMap("table-usage", "db:table:public.users")],
    ["column-impact", dbMap("column-impact", "db:column:public.users:id")],
  ])("keeps the same inspector reading order in %s mode", (_mode, currentMap) => {
    const { container } = render(
      <InspectorPanel
        workspaceControls={workspaceWithApi()}
        dbProfileControls={dbControlsWithUsers()}
        visualMapControls={readyControls(currentMap)}
      />,
    );

    const headings = Array.from(container.querySelectorAll(".inspector-section > header > strong"))
      .map((node) => node.textContent);
    expect(headings).toEqual(["요약", "바로 연결", "근거", "소스", "다음 확인"]);
  });

  it("collapses repeated empty evidence sections until an analysis target is chosen", () => {
    const { container } = render(
      <InspectorPanel
        workspaceControls={workspaceControls()}
        dbProfileControls={dbProfileControls()}
        visualMapControls={readyControls(emptyMap("atlas"))}
      />,
    );

    const headings = Array.from(container.querySelectorAll(".inspector-section > header > strong"))
      .map((node) => node.textContent);
    expect(headings).toEqual(["요약", "다음 확인"]);
  });

  it("keeps source controls in the scroll area and the next check in a fixed footer", () => {
    const { container } = render(
      <InspectorPanel
        workspaceControls={workspaceWithApi()}
        dbProfileControls={dbControlsWithUsers()}
        visualMapControls={readyControls(apiMap())}
      />,
    );

    const scrollBody = container.querySelector(".inspector-scroll-body");
    const footer = container.querySelector(".inspector > .inspector-section:last-child");
    expect(scrollBody?.querySelectorAll(":scope > .inspector-section")).toHaveLength(4);
    expect(footer?.querySelector("header > strong")).toHaveTextContent("다음 확인");
    expect(scrollBody?.contains(footer)).toBe(false);
  });

  it("keeps the review board next check consistent with the answer canvas", () => {
    const currentMap = dbMap("column-impact", "db:column:public.users:id");
    currentMap.reviewBoard = {
      subject: "public.users.id",
      scope: "column",
      lanes: [{
        id: "checks",
        order: 4,
        title: "다음 확인",
        description: "검토 순서",
        tone: "action",
        total: 1,
        hidden: 0,
        emptyMessage: "추가 확인 없음",
        items: [{
          id: "check-migration",
          nodeId: null,
          kind: "action",
          title: "마이그레이션 확인",
          detail: "컬럼 변경 전 배포 순서를 확인하세요.",
          truthClass: "unknown",
          rank: 1,
          evidence: [],
        }],
      }],
      markdownSummary: "public.users.id",
    };

    const { container } = render(
      <InspectorPanel
        workspaceControls={workspaceWithApi()}
        dbProfileControls={dbControlsWithUsers()}
        visualMapControls={readyControls(currentMap)}
      />,
    );

    const footer = container.querySelector(".inspector > .inspector-section:last-child");
    expect(footer).toHaveTextContent("마이그레이션 확인");
    expect(footer).toHaveTextContent("컬럼 변경 전 배포 순서를 확인하세요.");
  });

  it("keeps the API method in the inspector identity", () => {
    render(
      <InspectorPanel
        workspaceControls={workspaceWithApi()}
        dbProfileControls={dbControlsWithUsers()}
        visualMapControls={readyControls(apiMap())}
      />,
    );

    expect(screen.getByText("DELETE /api/v1/sessions")).toBeInTheDocument();
  });

  it("opens a direct API relationship without replacing the inspector structure", () => {
    const selectEdge = vi.fn();
    const currentMap = apiMap();
    render(
      <InspectorPanel
        workspaceControls={workspaceWithApi()}
        dbProfileControls={dbControlsWithUsers()}
        visualMapControls={{ ...readyControls(currentMap), selectEdge } as unknown as VisualMapControls}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /HANDLES.*loadSession/ }));
    expect(selectEdge).toHaveBeenCalledWith(currentMap.edges[0]);
    expect(screen.getByText(/소스 근거: 라우트 소스 위치를 확인했습니다/)).toBeInTheDocument();
    expect(screen.getByText(/경로 근거: 정적 prefix를 합성해 전체 경로를 확인했습니다/)).toBeInTheDocument();
  });

  it("presents code engine evidence in reader-facing language", () => {
    const currentMap = apiMap();
    currentMap.edges[0].evidence = [
      { kind: "code-call", text: "bootstrap_admin 코드 항목이 _to_session_response 코드 항목을 호출합니다" },
      { kind: "engine-callee", text: "_to_session_response" },
      { kind: "engine-confidence", text: "high" },
      { kind: "engine-confidence-score", text: "95%" },
      { kind: "engine-edge", text: "codebase-memory CALLS" },
      { kind: "engine-strategy", text: "lsp_direct" },
      {
        kind: "engine-edge",
        text: "codebase-memory HANDLES: upstream handler→route was normalized to product route→handler",
      },
    ];

    render(
      <InspectorPanel
        workspaceControls={workspaceWithApi()}
        dbProfileControls={dbControlsWithUsers()}
        visualMapControls={{ ...readyControls(currentMap), selectedEdge: currentMap.edges[0] }}
      />,
    );

    expect(screen.getByText("호출 관계: bootstrap_admin 코드 항목이 _to_session_response 코드 항목을 호출합니다")).toBeInTheDocument();
    expect(screen.getByText("호출 표현: _to_session_response")).toBeInTheDocument();
    expect(screen.getByText("신뢰 수준: 높음")).toBeInTheDocument();
    expect(screen.getByText("신뢰 점수: 95%")).toBeInTheDocument();
    expect(screen.getByText("관계 근거: 코드 엔진에서 호출 관계를 확인했습니다.")).toBeInTheDocument();
    expect(screen.getByText("분석 방식: LSP 직접 확인")).toBeInTheDocument();
    const moreEvidence = screen.getByText("1개 더 보기").closest("details");
    expect(moreEvidence).not.toHaveAttribute("open");
    fireEvent.click(screen.getByText("1개 더 보기"));
    expect(moreEvidence).toHaveAttribute("open");
    expect(screen.getByText(
      "관계 근거: 코드 엔진의 핸들러→라우트 관계를 제품의 라우트→핸들러 읽기 방향으로 정규화했습니다.",
    )).toBeInTheDocument();
    expect(screen.queryByText(/lsp_direct|codebase-memory (CALLS|HANDLES)|근거: high/)).not.toBeInTheDocument();
  });
});

function loadingControls(mode: string, currentMap: VisualMap | null = null): VisualMapControls {
  return {
    currentMap,
    mode,
    focusId: "code:route-next",
    loading: true,
    enriching: false,
    selectedNode: null,
    selectedEdge: null,
  } as unknown as VisualMapControls;
}

function visualMap(mode: string, focus: string): VisualMap {
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

function readyControls(currentMap: VisualMap): VisualMapControls {
  return {
    currentMap,
    mode: currentMap.mode,
    focusId: currentMap.focus,
    loading: false,
    enriching: false,
    selectedNode: null,
    selectedEdge: null,
    selectNode: vi.fn(),
    selectEdge: vi.fn(),
  } as unknown as VisualMapControls;
}

function emptyMap(mode: string): VisualMap {
  return visualMap(mode, "narrow-focus");
}

function codeMap(): VisualMap {
  return {
    ...visualMap("search-focus", "code:function-old"),
    nodes: [{ id: "code:function-old", kind: "function", title: "oldFunction", subtitle: "src/old.ts", layer: "code", source: "code" }],
  };
}

function apiMap(): VisualMap {
  return {
    ...visualMap("api-flow", "code:route-session"),
    nodes: [
      { id: "code:route-session", kind: "api", title: "/api/v1/sessions", subtitle: "src/routes.ts", layer: "api", source: "code" },
      { id: "code:handler-session", kind: "handler", title: "loadSession", subtitle: "src/handlers.ts", layer: "code", source: "code" },
    ],
    edges: [{
      id: "handles-session",
      from: "code:route-session",
      to: "code:handler-session",
      kind: "code_handle",
      confidence: "high",
      evidence: [{ kind: "route-binding", text: "라우트 등록에서 핸들러를 직접 확인했습니다." }],
    }],
    apiReading: {
      subject: "/api/v1/sessions",
      method: "DELETE",
      steps: [{
        id: "route-step",
        nodeId: "code:route-session",
        kind: "api",
        title: "/api/v1/sessions",
        detail: "src/routes.ts:12",
        truthClass: "confirmed",
        rank: 0,
        evidence: [
          { kind: "route-source", text: "라우트 소스 위치를 확인했습니다." },
          { kind: "route-mount", text: "정적 prefix를 합성해 전체 경로를 확인했습니다." },
        ],
        depth: 0,
        lane: "route",
        laneBasis: "engine-node",
        incomingEvidence: [],
      }],
      dbCandidates: [],
      unknowns: [],
      recommendedChecks: [{
        id: "check-handler",
        nodeId: "code:handler-session",
        kind: "action",
        title: "핸들러 구현 확인",
        detail: "loadSession의 호출을 이어서 확인합니다.",
        truthClass: "action",
        rank: 1,
        evidence: [],
      }],
      hiddenBranches: 0,
      truncated: false,
    },
  };
}

function dbMap(mode: string, focus: string): VisualMap {
  return {
    ...visualMap(mode, focus),
    nodes: [
      { id: "db:table:public.users", kind: "table", title: "users", subtitle: "public", layer: "database", source: "db" },
      { id: "db:column:public.users:id", kind: "column", title: "id", subtitle: "uuid", layer: "database", source: "db" },
    ],
  };
}

function workspaceControls(): WorkspaceControls {
  return {
    currentWorkspace: { id: "workspace-1", name: "backend" },
    codeInventory: {
      routes: [],
      services: [],
      handlers: [],
      repositories: [],
      functions: [{ id: "function-old", kind: "function", name: "oldFunction", filePath: "src/old.ts", line: 1 }],
      classes: [],
      modules: [],
      unknown: [],
      files: [],
      calls: [],
      summary: { routes: 0, handlers: 0, services: 0, repositories: 0, functions: 1, classes: 0, modules: 0, files: 0, unknown: 0 },
    },
  } as unknown as WorkspaceControls;
}

function dbProfileControls(): DbProfileControls {
  return {
    inventory: null,
    profileName: "",
    profilePath: "",
    connectionString: "",
    canSaveProfile: false,
  } as unknown as DbProfileControls;
}

function workspaceWithApi(): WorkspaceControls {
  const controls = workspaceControls();
  return {
    ...controls,
    codeInventory: {
      ...controls.codeInventory!,
      routes: [{ id: "route-session", kind: "api", name: "/api/v1/sessions", filePath: "src/routes.ts", line: 12 }],
      handlers: [{ id: "handler-session", kind: "handler", name: "loadSession", filePath: "src/handlers.ts", line: 24 }],
    },
  } as WorkspaceControls;
}

function dbControlsWithUsers(): DbProfileControls {
  return {
    ...dbProfileControls(),
    inventory: {
      tables: [{
        schema: "public",
        name: "users",
        columns: [{ name: "id", dataType: "uuid", isPrimaryKey: true, isForeignKey: false, nullable: false }],
      }],
      summary: { tables: 1, columns: 1 },
    },
    selectedTableKey: "public.users",
    openColumn: vi.fn(),
  } as unknown as DbProfileControls;
}
