import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { VisualMap } from "../types/visual-map";
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

  it("hides the previous map while a different mode is loading", async () => {
    const { result } = renderHook(() => useVisualMap({ currentWorkspaceId: "workspace-1" }));

    await waitFor(() => expect(requests).toHaveLength(1));
    act(() => requests[0].resolve(visualMap("atlas", "overview")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("atlas"));

    act(() => result.current.showMapMode("api-flow", "code:route-1"));

    expect(result.current.visualMap).toBeNull();
    expect(result.current.visualMapLoading).toBe(true);

    await waitFor(() => expect(requests).toHaveLength(2));
    act(() => requests[1].resolve(visualMap("api-flow", "code:route-1")));
    await waitFor(() => expect(result.current.visualMap?.mode).toBe("api-flow"));
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
