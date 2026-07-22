import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CodeInventoryItem } from "../../types/workspace";
import { SourceJumpRow } from "./SourceJumpRow";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const codeItem: CodeInventoryItem = {
  id: "code.example",
  name: "example",
  qualifiedName: "code.example",
  kind: "function",
  engineLabel: "Function",
  filePath: "src/example.ts",
  line: null,
  column: null,
  endLine: null,
  endColumn: null,
  detail: null,
};

describe("SourceJumpRow", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.mocked(invoke).mockReset().mockResolvedValue(undefined);
  });

  it("does not invent line one when source evidence has no position", async () => {
    render(<SourceJumpRow workspaceId="workspace-1" code={codeItem} />);

    fireEvent.click(screen.getByRole("button", { name: "조사함 추가" }));

    expect(screen.getByText("조사함").closest("details")).not.toHaveAttribute("open");
    expect(screen.getByText("src/example.ts")).toBeInTheDocument();
    expect(screen.queryByText("src/example.ts:1")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("backend-visual-map:investigation:v1:workspace-1")).toContain(
      '"line":null',
    );

    fireEvent.click(screen.getByRole("button", { name: "VS Code" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_source_location", {
      request: {
        workspaceId: "workspace-1",
        path: "src/example.ts",
        line: null,
        column: null,
        editor: "vscode",
      },
    }));
  });

  it("shows a readable command error when opening the source fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "editor_unavailable",
      message: "에디터를 찾지 못했습니다. 설치 상태와 PATH를 확인하세요",
      detail: "code executable was not found",
      retryable: false,
    });
    render(<SourceJumpRow workspaceId="workspace-1" code={codeItem} />);

    fireEvent.click(screen.getByRole("button", { name: "VS Code" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "VS Code에서 소스를 열지 못했습니다: 에디터를 찾지 못했습니다. 설치 상태와 PATH를 확인하세요",
    );
    expect(screen.getByRole("status")).not.toHaveTextContent("[object Object]");
  });
});
