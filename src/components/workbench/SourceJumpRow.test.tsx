import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import type { CodeInventoryItem } from "../../types/workspace";
import { SourceJumpRow } from "./SourceJumpRow";

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
  beforeEach(() => window.localStorage.clear());

  it("does not invent line one when source evidence has no position", () => {
    render(<SourceJumpRow workspaceId="workspace-1" code={codeItem} />);

    fireEvent.click(screen.getByRole("button", { name: "조사함 추가" }));

    expect(screen.getByText("조사함").closest("details")).not.toHaveAttribute("open");
    expect(screen.getByText("src/example.ts")).toBeInTheDocument();
    expect(screen.queryByText("src/example.ts:1")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("backend-visual-map:investigation:v1:workspace-1")).toContain(
      '"line":null',
    );
  });
});
