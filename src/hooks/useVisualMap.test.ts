import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../types/visual-map";
import { saveMapContext, savedMapContext, savedModeMapContext } from "../visual/mapContext";
import { useVisualMap } from "./useVisualMap";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
};

const invokeMock = vi.mocked(invoke);

describe("useVisualMap transitions", () => {
  const requests: Deferred<VisualMap>[] = [];

  beforeEach(() => {
    localStorage.clear();
    requests.length = 0;
    invokeMock.mockReset();
    invokeMock.mockImplementation(() => {
      const request = deferred<VisualMap>();
      requests.push(request);
      return request.promise;
    });
  });

  it("keeps the last committed mode visible while the next mode loads", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.showMapMode("api-flow", "code:route-1"));

    expect(result.current.visualMap?.mode).toBe("atlas");
    expect(result.current.visualMapLoading).toBe(true);
    expect(result.current.mapFocusId).toBe("code:route-1");

    await waitFor(() => expect(requests).toHaveLength(2));
    act(() => requests[1].resolve(visualMap("api-flow", "code:route-1")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("api-flow"));
  });

  it("commits an enriched DB answer once instead of flashing the base answer first", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.showMapMode("column-impact", "db:column:public.users:email"));
    await waitFor(() => expect(requests).toHaveLength(2));
    act(() => requests[1].resolve(visualMap("column-impact", "db:column:public.users:email")));
    await waitFor(() => expect(requests).toHaveLength(3));

    expect(result.current.visualMap?.mode).toBe("atlas");
    expect(result.current.visualMapLoading).toBe(true);
    expect(result.current.visualMapEnriching).toBe(true);

    act(() => requests[2].resolve(visualMap("column-impact", "db:column:public.users:email")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("column-impact"));
    expect(result.current.visualMapLoading).toBe(false);
  });

  it("enriches an API answer when its base path has a DB candidate", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.showMapMode("api-flow", "code:route-1"));
    await waitFor(() => expect(requests).toHaveLength(2));
    const base = visualMap("api-flow", "code:route-1");
    base.apiReading = {
      subject: "/sessions",
      steps: [],
      dbCandidates: [{
        id: "candidate",
        nodeId: "db:table:public.sessions",
        kind: "db-candidate",
        title: "sessions",
        detail: "검증 필요",
        truthClass: "candidate",
        confidence: "medium",
        rank: 1,
        evidence: [],
      }],
      unknowns: [],
      recommendedChecks: [],
      hiddenBranches: 0,
      hiddenBranchesIsLowerBound: false,
      truncated: false,
    };
    act(() => requests[1].resolve(base));
    await waitFor(() => expect(requests).toHaveLength(3));

    expect(result.current.visualMap?.mode).toBe("atlas");
    expect(result.current.visualMapEnriching).toBe(true);
    expect(invokeMock).toHaveBeenNthCalledWith(3, "get_visual_map", expect.objectContaining({
      mode: "api-flow",
      enrichCodeEvidence: true,
    }));

    act(() => requests[2].resolve(base));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("api-flow"));
    expect(result.current.visualMapLoading).toBe(false);
  });

  it("keeps the previous answer visible while another target in the same mode loads", async () => {
    saveMapContext("workspace-1", "api-flow", "code:route-1");
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("api-flow", "code:route-1")));
    await waitFor(() => expect(result.current.visualMap?.focus).toBe("code:route-1"));

    act(() => result.current.showMapMode("api-flow", "code:route-2"));

    expect(result.current.visualMap?.focus).toBe("code:route-1");
    expect(result.current.visualMapLoading).toBe(true);
    expect(result.current.mapFocusId).toBe("code:route-2");

    await waitFor(() => expect(requests).toHaveLength(2));
    act(() => requests[1].resolve(visualMap("api-flow", "code:route-2")));
    await waitFor(() => expect(result.current.visualMap?.focus).toBe("code:route-2"));
  });

  it("ignores an older response that finishes after the latest request", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.showMapMode("api-flow", "code:route-1"));
    act(() => result.current.showMapMode("explore", "code:function-1"));
    await waitFor(() => expect(requests).toHaveLength(3));

    act(() => requests[2].resolve(visualMap("explore", "code:function-1")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("explore"));

    act(() => requests[1].resolve(visualMap("api-flow", "code:route-1")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("explore"));
    expect(result.current.visualMap?.focus).toBe("code:function-1");
  });

  it("never exposes a response from the previously selected workspace", async () => {
    const { result, rerender } = renderHook(
      ({ workspaceId }: { workspaceId: string }) => useVisualMap({ currentWorkspaceId: workspaceId }),
      { initialProps: { workspaceId: "workspace-1" } },
    );

    await waitFor(() => expect(requests).toHaveLength(1));
    rerender({ workspaceId: "workspace-2" });
    await waitFor(() => expect(requests).toHaveLength(2));

    act(() => requests[0].resolve(visualMap("atlas", "overview", "workspace-1")));
    expect(result.current.visualMap).toBeNull();

    act(() => requests[1].resolve(visualMap("atlas", "overview", "workspace-2")));
    await waitFor(() => expect(result.current.visualMap?.workspaceId).toBe("workspace-2"));
  });

  it("treats a stale snapshot as a re-read state instead of a canvas failure", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() =>
      requests[0].reject({
        code: "snapshot_stale",
        message: "읽기 결과가 현재 소스와 다릅니다",
        detail: "코드/DB 읽기 결과가 최신이 아닙니다",
        retryable: true,
      }),
    );

    await waitFor(() => expect(result.current.visualMapLoading).toBe(false));
    expect(result.current.visualMap).toBeNull();
    expect(result.current.visualMapError).toBeNull();
    expect(result.current.visualMapErrorDetail).toBeNull();
    expect(result.current.visualMapStatus).toBe("코드/DB 읽기 결과 필요");
  });

  it("keeps an explicitly selected focus target visible when an older projection omitted its node", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));
    const focusId = "code:function:target";
    const focusNode: VisualNode = {
      id: focusId,
      kind: "function",
      title: "target",
      layer: "code",
      source: "code",
    };

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));
    act(() => result.current.showMapMode("search-focus", focusId));
    await waitFor(() => expect(requests).toHaveLength(2));
    act(() => requests[1].resolve(visualMap("search-focus", focusId)));
    await waitFor(() => expect(result.current.visualMap?.focus).toBe(focusId));

    act(() => result.current.setSelectedVisualNode(focusNode));

    expect(result.current.selectedVisualNode).toEqual(focusNode);
  });

  it("clears a stale global search session when navigating to any product mode", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.setSearchQuery("audio"));
    expect(result.current.searchQuery).toBe("audio");

    act(() => result.current.showMapMode("search-focus", "code:function-1"));

    expect(result.current.searchQuery).toBe("");
    expect(result.current.searchPopoverOpen).toBe(false);
    expect(result.current.searchSummary).toBeNull();
    expect(result.current.searchGroups).toEqual([]);
  });

  it("preserves the query only while the global search flow is active", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.setSearchQuery("a"));
    act(() =>
      result.current.runSearch({
        codeInventory: null,
        dbInventory: null,
        selectCodeItem: vi.fn(),
        selectDbTable: vi.fn(),
      }),
    );

    expect(result.current.searchQuery).toBe("a");
    expect(result.current.searchPopoverOpen).toBe(true);
    expect(result.current.searchSummary).toBe("두 글자 이상 입력하면 더 정확합니다.");
  });

  it("uses a navigation focus as the inspector selection after the map commits", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));
    const focusId = "code:function:target";
    const map = visualMap("search-focus", focusId);
    map.nodes = [{ id: focusId, kind: "function", title: "target", layer: "code", source: "code" }];

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));
    act(() => result.current.showMapMode("search-focus", focusId));
    await waitFor(() => expect(requests).toHaveLength(2));
    act(() => requests[1].resolve(map));
    await waitFor(() => expect(result.current.visualMap?.focus).toBe(focusId));

    expect(result.current.selectedVisualNode).toEqual(map.nodes[0]);
    expect(result.current.selectedVisualEdge).toBeNull();
  });

  it("keeps node and relationship selection mutually exclusive", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));
    const node: VisualNode = {
      id: "code:function:target",
      kind: "function",
      title: "target",
      layer: "code",
      source: "code",
    };
    const edge: VisualEdge = {
      id: "edge-1",
      from: node.id,
      to: "db:table:public.users",
      kind: "USES_TABLE",
      evidence: [],
    };

    await waitFor(() => expect(requests).toHaveLength(1));
    const map = visualMap("atlas", "overview");
    map.nodes = [node];
    map.edges = [edge];
    act(() => requests[0].resolve(map));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.setSelectedVisualEdge(edge));
    expect(result.current.selectedVisualEdge).toEqual(edge);
    expect(result.current.selectedVisualNode).toBeNull();

    act(() => result.current.setSelectedVisualNode(node));
    expect(result.current.selectedVisualNode).toEqual(node);
    expect(result.current.selectedVisualEdge).toBeNull();
  });

  it("clears a removed DB target from the persisted map context", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));
    act(() => result.current.showMapMode("column-impact", "db:column:main.orders:user_id"));

    act(() => result.current.clearVisualMap());

    expect(result.current.mapMode).toBe("atlas");
    expect(result.current.mapFocusId).toBeNull();
    expect(result.current.visualMap).toBeNull();
    expect(savedMapContext("workspace-1")).toEqual({ mode: "atlas", focusId: null });
    expect(savedModeMapContext("workspace-1", "column-impact")).toBeNull();
  });
});

function visualMap(mode: string, focus: string, workspaceId = "workspace-1"): VisualMap {
  return {
    id: `${mode}:${focus}`,
    workspaceId,
    mode,
    focus,
    nodes: [],
    edges: [],
    warnings: [],
  };
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
