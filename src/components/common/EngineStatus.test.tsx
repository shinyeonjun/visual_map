import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EngineStatus } from "./EngineStatus";
import type { EngineRegistry } from "../../types/engine";

const registry: EngineRegistry = {
  mode: "dev",
  engineDir: "C:/VisualMap/engines",
  engines: [
    {
      id: "codebase-memory",
      label: "codebase-memory",
      role: "code",
      executable: "code-memory-language.exe",
      expectedVersion: "0.1.0",
      contractVersion: "1",
      path: "C:/VisualMap/engines/code-memory-language.exe",
      available: false,
      releasable: false,
      integrity: "mismatch",
      error: "읽기 도구 체크섬이 manifest와 일치하지 않습니다",
    },
  ],
};

describe("EngineStatus", () => {
  it("shows the exact engine failure in the status UI", () => {
    render(<EngineStatus label="코드 읽기" role="code" registry={registry} error={null} />);

    expect(screen.getByText("읽기 도구 체크섬이 manifest와 일치하지 않습니다")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("읽기 도구 체크섬이 manifest와 일치하지 않습니다");
    expect(screen.getByLabelText(/코드 읽기: 설치 필요/)).toHaveAttribute(
      "data-engine-error",
      "읽기 도구 체크섬이 manifest와 일치하지 않습니다",
    );
  });

  it("shows a registry lookup failure even without an engine entry", () => {
    render(
      <EngineStatus
        label="DB 읽기"
        role="db"
        registry={null}
        error="읽기 도구 상태를 확인하지 못했습니다"
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("읽기 도구 상태를 확인하지 못했습니다");
  });
});
