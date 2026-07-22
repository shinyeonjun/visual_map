import { act, renderHook } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DB_PROFILE_SOURCE_OPTIONS, dbProfileSourceUsesPath } from "../types/workspace";
import { toDbUserError, useDbProfiles } from "./useDbProfiles";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const openMock = vi.mocked(open);

describe("DB profile picker", () => {
  beforeEach(() => {
    window.__TAURI_INTERNALS__ = {};
    openMock.mockReset();
  });

  afterEach(() => {
    delete window.__TAURI_INTERNALS__;
  });

  it("reports a file picker failure", async () => {
    openMock.mockRejectedValue(new Error("dialog unavailable"));
    const { result } = renderHook(() =>
      useDbProfiles({
        currentWorkspace: null,
        withBusy: async (_action, task) => task(),
        setCurrentWorkspace: vi.fn(),
        refreshWorkspaces: vi.fn(),
        clearVisualMap: vi.fn(),
        refreshInventorySnapshot: vi.fn(),
      }),
    );

    await act(() => result.current.pickDbPath());

    expect(result.current.dbError).toBe("DB 파일 선택기를 열지 못했습니다");
    expect(result.current.dbErrorDetail).toContain("dialog unavailable");
  });

  it("opens a directory picker for a DDL folder", async () => {
    openMock.mockResolvedValue("D:\\schema");
    const { result } = renderHook(() =>
      useDbProfiles({
        currentWorkspace: null,
        withBusy: async (_action, task) => task(),
        setCurrentWorkspace: vi.fn(),
        refreshWorkspaces: vi.fn(),
        clearVisualMap: vi.fn(),
        refreshInventorySnapshot: vi.fn(),
      }),
    );

    await act(() => result.current.pickDbPath(true));

    expect(openMock).toHaveBeenCalledWith(expect.objectContaining({ directory: true, title: "DDL 폴더 선택" }));
    expect(result.current.dbProfilePath).toBe("D:\\schema");
  });
});

describe("DB source contract", () => {
  it("offers every certified native source as a distinct adapter choice", () => {
    expect(DB_PROFILE_SOURCE_OPTIONS.map(({ value }) => value)).toEqual([
      "ddl-sqlite",
      "sqlite",
      "postgres",
      "yugabytedb",
      "mysql",
      "mariadb",
      "sqlserver",
      "oracle",
    ]);
    expect(dbProfileSourceUsesPath("ddl-sqlite")).toBe(true);
    expect(dbProfileSourceUsesPath("yugabytedb")).toBe(false);
    expect(dbProfileSourceUsesPath("mariadb")).toBe(false);
  });
});

describe("DB error guidance", () => {
  it("turns a missing Oracle Client into an actionable message", () => {
    const error = toDbUserError(
      'DPI-1047: Cannot locate a 64-bit Oracle Client library: "The specified module could not be found"',
      "DB 읽기 실패",
    );

    expect(error.message).toBe("DB 읽기 실패: Oracle Client를 설치한 뒤 앱을 다시 시작하세요");
    expect(error.details).toContain("DPI-1047");
  });

  it("explains fail-closed contract errors without presenting partial data", () => {
    const error = toDbUserError(
      "DB inventory가 완전하지 않습니다: expected 20 tables, got 10",
      "DB 읽기 실패",
    );

    expect(error.message).toBe("DB 읽기 실패: 완전한 DB 구조를 확인하지 못했습니다. DB를 다시 읽으세요");
    expect(error.details).toContain("expected 20 tables");
  });
});
