import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Workspace } from "../types/workspace";
import { useWorkspaces } from "./useWorkspaces";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const invokeMock = vi.mocked(invoke);

describe("workspace deletion", () => {
  beforeEach(() => {
    window.__TAURI_INTERNALS__ = {};
    invokeMock.mockReset();
  });

  afterEach(() => {
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

    let deletion!: Promise<void>;
    act(() => {
      deletion = result.current.deleteWorkspace(workspace.id);
    });
    await waitFor(() => expect(result.current.workspaces).toEqual([]));
    expect(result.current.currentWorkspace).toBeNull();

    await act(async () => {
      refreshed.resolve([]);
      await deletion;
    });
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
