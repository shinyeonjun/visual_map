import { describe, expect, it } from "vitest";
import { currentOperationStatus } from "./appState";

const baseState = {
  busyAction: null,
  latestAction: null,
  workspaceStatus: null,
  workspaceError: null,
  codeStatus: null,
  codeError: null,
  codeErrorDetail: null,
  dbStatus: null,
  dbError: null,
  dbErrorDetail: null,
  mapStatus: null,
  mapLoading: false,
  mapError: null,
  mapErrorDetail: null,
};

describe("current operation status", () => {
  it("shows the most recently completed DB operation over older map and code messages", () => {
    expect(
      currentOperationStatus({
        ...baseState,
        latestAction: "db-index",
        codeStatus: "코드 120개 읽음",
        dbStatus: "테이블 24개, 컬럼 180개 읽음",
        mapStatus: "캔버스 항목 40개 표시",
      }),
    ).toEqual({
      phase: "success",
      label: "DB",
      message: "테이블 24개, 컬럼 180개 읽음",
    });
  });

  it("keeps a recent code error visible instead of an older workspace error", () => {
    expect(
      currentOperationStatus({
        ...baseState,
        latestAction: "code-index",
        workspaceError: "이전 프로젝트 오류",
        codeError: "코드 읽기 실패",
        codeErrorDetail: "parser exited with code 1",
      }),
    ).toEqual({
      phase: "error",
      label: "코드",
      message: "코드 읽기 실패",
      details: "parser exited with code 1",
    });
  });

  it("always gives an actively running operation precedence", () => {
    expect(
      currentOperationStatus({
        ...baseState,
        busyAction: "workspace-repair",
        latestAction: "db-index",
        dbStatus: "테이블 24개 읽음",
      }),
    ).toEqual({
      phase: "running",
      label: "프로젝트 복구",
      message: "프로젝트 복구 진행 중",
    });
  });
});
