import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Workspace } from "../types/workspace";
import { useWorkspaces } from "./useWorkspaces";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const openMock = vi.mocked(open);

describe("workspace deletion", () => {
  beforeEach(() => {
    window.__TAURI_INTERNALS__ = {};
    window.localStorage.clear();
    invokeMock.mockReset();
    openMock.mockReset();
  });

  afterEach(() => {
    window.localStorage.clear();
    delete window.__TAURI_INTERNALS__;
  });

  it("removes a deleted workspace before the list refresh finishes", async () => {
    const refreshed = deferred<Workspace[]>();
    let listCalls = 0;
    invokeMock.mockImplementation((command) => {
      if (command === "list_workspaces") {
        listCalls += 1;
        return listCalls === 1 ? Promise.resolve([workspace]) : refreshed.promise;
      }
      if (command === "get_workspace_recovery_warnings") {
        return Promise.resolve([]);
      }
      if (command === "delete_workspace") {
        return Promise.resolve();
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const { result } = renderHook(() => useWorkspaces({ withBusy }));
    await waitFor(() => expect(result.current.currentWorkspace?.id).toBe(workspace.id));
    window.localStorage.setItem(`backend-visual-map:map-context:v1:${workspace.id}`, "map");
    window.localStorage.setItem(`backend-visual-map:investigation:v1:${workspace.id}`, "investigation");
    window.localStorage.setItem("backend-visual-map:map-context:v1:workspace-other", "keep");

    let deletion!: Promise<void>;
    act(() => {
      deletion = result.current.deleteWorkspace(workspace.id);
    });
    await waitFor(() => expect(result.current.workspaces).toEqual([]));
    expect(result.current.currentWorkspace).toBeNull();
    expect(window.localStorage.getItem("backend-visual-map:last-workspace")).toBeNull();
    expect(window.localStorage.getItem(`backend-visual-map:map-context:v1:${workspace.id}`)).toBeNull();
    expect(window.localStorage.getItem(`backend-visual-map:investigation:v1:${workspace.id}`)).toBeNull();
    expect(window.localStorage.getItem("backend-visual-map:map-context:v1:workspace-other")).toBe("keep");

    await act(async () => {
      refreshed.resolve([]);
      await deletion;
    });
  });

  it("restores the last opened workspace instead of the most recently indexed one", async () => {
    window.localStorage.setItem("backend-visual-map:last-workspace", secondWorkspace.id);
    invokeMock.mockImplementation((command, args) => {
      if (command === "list_workspaces") {
        return Promise.resolve([workspace, secondWorkspace]);
      }
      if (command === "get_workspace_recovery_warnings") {
        return Promise.resolve([]);
      }
      if (command === "open_workspace") {
        return Promise.resolve((args as { workspaceId: string }).workspaceId === workspace.id ? workspace : secondWorkspace);
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const { result } = renderHook(() => useWorkspaces({ withBusy }));
    await waitFor(() => expect(result.current.currentWorkspace?.id).toBe(secondWorkspace.id));

    await act(() => result.current.openWorkspace(workspace.id));

    expect(result.current.currentWorkspace?.id).toBe(workspace.id);
    expect(window.localStorage.getItem("backend-visual-map:last-workspace")).toBe(workspace.id);
  });

  it("reports a folder picker failure instead of leaving an unhandled rejection", async () => {
    openMock.mockRejectedValue(new Error("dialog unavailable"));
    invokeMock.mockImplementation((command) => {
      if (command === "list_workspaces" || command === "get_workspace_recovery_warnings") {
        return Promise.resolve([]);
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const { result } = renderHook(() => useWorkspaces({ withBusy }));
    await waitFor(() => expect(result.current.initialized).toBe(true));
    await act(() => result.current.pickRepoPath());

    expect(result.current.workspaceError).toBe("프로젝트 폴더 선택기를 열지 못했습니다");
  });
});

async function withBusy(_action: string, task: () => Promise<void>) {
  await task();
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

const workspace: Workspace = {
  id: "workspace-1",
  name: "Project",
  repoPath: "D:\\project",
  repoSource: "local",
  dbProfiles: [],
  createdAt: "2026-07-19T00:00:00Z",
  updatedAt: "2026-07-19T00:00:00Z",
};

const secondWorkspace: Workspace = {
  ...workspace,
  id: "workspace-2",
  name: "Second Project",
  repoPath: "D:\\second-project",
};
