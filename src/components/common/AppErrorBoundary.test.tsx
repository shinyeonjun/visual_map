import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppErrorBoundary } from "./AppErrorBoundary";

function BrokenView(): never {
  throw new Error("render failed");
}

describe("AppErrorBoundary", () => {
  let consoleError: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
  });
  afterEach(() => consoleError.mockRestore());

  it("keeps a render failure recoverable without clearing product data", () => {
    render(
      <AppErrorBoundary>
        <BrokenView />
      </AppErrorBoundary>,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("프로젝트 원본과 저장된 읽기 결과는 변경하지 않았습니다");
    expect(screen.getByRole("button", { name: "앱 다시 불러오기" })).toBeInTheDocument();
  });
});
