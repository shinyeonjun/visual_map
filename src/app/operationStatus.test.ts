import { describe, expect, it } from "vitest";
import { runningOperation, toUserError } from "./operationStatus";

describe("operation status", () => {
  it("exposes a stable label for GitHub refresh", () => {
    expect(runningOperation("workspace-refresh")).toEqual({
      phase: "running",
      label: "GitHub 업데이트",
      message: "GitHub 업데이트 진행 중",
    });
  });

  it("explains why source actions are locked during snapshot restoration", () => {
    expect(runningOperation("snapshot-restore")).toEqual({
      phase: "running",
      label: "저장 결과 확인",
      message: "저장 결과 확인 진행 중",
    });
  });

  it("preserves structured command details without parsing message fragments", () => {
    const error = {
      code: "workspace_dirty",
      message: "로컬 변경이 있습니다",
      detail: "README.md",
      retryable: false,
    };

    expect(toUserError(error, "업데이트 실패")).toEqual({
      message: "업데이트 실패: 로컬 변경이 있습니다",
      details: "README.md",
      code: "workspace_dirty",
    });
  });
});
