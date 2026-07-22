import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CodeInventory, DbInventory, DbProfile, Workspace } from "../types/workspace";
import { useCodeInventory } from "./useCodeInventory";
import { useDbProfiles } from "./useDbProfiles";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

describe("inventory snapshot persistence", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("keeps code indexing busy until the persisted snapshot is refreshed", async () => {
    const saved = deferred();
    const refreshInventorySnapshot = vi.fn(() => saved.promise);
    const refreshWorkspaces = vi.fn(async () => undefined);
    invokeMock.mockImplementation((command) => {
      if (command === "index_code_repository") {
        return Promise.resolve({ workspace, run: { ok: true, stderr: "" }, inventory: codeInventory });
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    const { result } = renderHook(() =>
      useCodeInventory({
        currentWorkspace: workspace,
        withBusy,
        setCurrentWorkspace: vi.fn(),
        refreshWorkspaces,
        refreshInventorySnapshot,
      }),
    );

    let operation!: Promise<void>;
    act(() => {
      operation = result.current.indexCodeRepository();
    });
    await waitFor(() => expect(refreshInventorySnapshot).toHaveBeenCalledOnce());
    expect(refreshWorkspaces).not.toHaveBeenCalled();

    await act(async () => {
      saved.resolve();
      await operation;
    });
    expect(refreshWorkspaces).toHaveBeenCalledWith(workspace.id);
  });

  it("keeps DB indexing busy until the persisted snapshot is refreshed", async () => {
    const saved = deferred();
    const refreshInventorySnapshot = vi.fn(() => saved.promise);
    const refreshWorkspaces = vi.fn(async () => undefined);
    invokeMock.mockImplementation((command) => {
      if (command === "index_db_profile") {
        return Promise.resolve({
          workspace: dbWorkspace,
          run: { ok: true, stderr: "" },
          indexJson: null,
          inventory: dbInventory,
        });
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    const { result } = renderHook(() =>
      useDbProfiles({
        currentWorkspace: dbWorkspace,
        withBusy,
        setCurrentWorkspace: vi.fn(),
        refreshWorkspaces,
        clearVisualMap: vi.fn(),
        refreshInventorySnapshot,
      }),
    );
    await waitFor(() => expect(result.current.dbProfileName).toBe(dbProfile.name));

    let operation!: Promise<void>;
    act(() => {
      operation = result.current.indexDbProfile();
    });
    await waitFor(() => expect(refreshInventorySnapshot).toHaveBeenCalledOnce());
    expect(refreshWorkspaces).not.toHaveBeenCalled();

    await act(async () => {
      saved.resolve();
      await operation;
    });
    expect(refreshWorkspaces).toHaveBeenCalledWith(dbWorkspace.id);
  });

  it("does not snapshot a successful code index without extracted inventory", async () => {
    const refreshInventorySnapshot = vi.fn(async () => undefined);
    invokeMock.mockResolvedValue({
      workspace,
      run: { ok: true, stderr: "" },
      inventory: null,
      inventoryError: "inventory extraction failed",
    });
    const { result } = renderHook(() =>
      useCodeInventory({
        currentWorkspace: workspace,
        withBusy,
        setCurrentWorkspace: vi.fn(),
        refreshWorkspaces: vi.fn(async () => undefined),
        refreshInventorySnapshot,
      }),
    );

    await act(() => result.current.indexCodeRepository());

    expect(refreshInventorySnapshot).not.toHaveBeenCalled();
    expect(result.current.codeError).toContain("코드 목록을 불러오지 못했습니다");
  });

  it("does not snapshot a successful DB index without extracted inventory", async () => {
    const refreshInventorySnapshot = vi.fn(async () => undefined);
    invokeMock.mockResolvedValue({
      workspace: dbWorkspace,
      run: { ok: true, stderr: "" },
      indexJson: null,
      inventory: null,
      inventoryError: "inventory extraction failed",
    });
    const { result } = renderHook(() =>
      useDbProfiles({
        currentWorkspace: dbWorkspace,
        withBusy,
        setCurrentWorkspace: vi.fn(),
        refreshWorkspaces: vi.fn(async () => undefined),
        clearVisualMap: vi.fn(),
        refreshInventorySnapshot,
      }),
    );
    await waitFor(() => expect(result.current.dbProfileName).toBe(dbProfile.name));

    await act(() => result.current.indexDbProfile());

    expect(refreshInventorySnapshot).not.toHaveBeenCalled();
    expect(result.current.dbError).toContain("테이블 목록을 불러오지 못했습니다");
  });
});

async function withBusy(_action: string, task: () => Promise<void>) {
  await task();
}

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((resolvePromise) => {
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

const codeInventory: CodeInventory = {
  project: "Project",
  routes: [],
  services: [],
  files: [],
  handlers: [],
  repositories: [],
  functions: [],
  classes: [],
  modules: [],
  unknown: [],
  summary: {
    routes: 0,
    handlers: 0,
    services: 0,
    repositories: 0,
    functions: 0,
    classes: 0,
    modules: 0,
    files: 0,
    unknown: 0,
  },
  calls: [],
};

const dbProfile: DbProfile = {
  id: "profile-1",
  name: "Local schema",
  source: "ddl-sqlite",
  path: "D:\\schema.sql",
  cachePath: "D:\\cache",
  passwordStored: false,
};

const dbWorkspace: Workspace = {
  ...workspace,
  dbProfiles: [dbProfile],
  activeDbProfileId: dbProfile.id,
};

const dbInventory: DbInventory = {
  profileId: dbProfile.id,
  tables: [],
};
