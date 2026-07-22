import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ChangeIntent, ImpactReviewBoard as ImpactReviewBoardModel, VisualMap } from "../../types/visual-map";
import { ImpactReviewBoard } from "./ImpactReviewBoard";

describe("ImpactReviewBoard", () => {
  it("keeps candidate strength distinct from confirmed relationships", () => {
    renderBoard({ kind: "rename", value: null });

    expect(screen.getByText("후보")).toBeInTheDocument();
    expect(screen.getByText("강함")).toBeInTheDocument();
    expect(screen.queryByText("높음")).not.toBeInTheDocument();
  });

  it("emits an explicit change scenario and synchronizes external values", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const view = renderBoard({ kind: "rename", value: null }, onChange);

    await user.click(screen.getByRole("button", { name: "타입 변경" }));
    expect(onChange).toHaveBeenLastCalledWith({ kind: "type", value: null });

    view.rerender(component({ kind: "type", value: "uuid" }, onChange));
    expect(screen.getByRole("textbox", { name: "목표 타입" })).toHaveValue("uuid");
    await user.clear(screen.getByRole("textbox", { name: "목표 타입" }));
    await user.type(screen.getByRole("textbox", { name: "목표 타입" }), "varchar(255)");
    await user.click(screen.getByRole("button", { name: "적용" }));
    expect(onChange).toHaveBeenLastCalledWith({ kind: "type", value: "varchar(255)" });
  });
});

function renderBoard(intent: ChangeIntent, onChange = vi.fn()) {
  return render(component(intent, onChange));
}

function component(intent: ChangeIntent, onChange: (intent: ChangeIntent) => void) {
  return (
    <ImpactReviewBoard
      board={board}
      map={map}
      onSelectNode={() => undefined}
      changeIntent={intent}
      onChangeIntent={onChange}
    />
  );
}

const board: ImpactReviewBoardModel = {
  subject: "users.email",
  scope: "column",
  lanes: [
    {
      id: "candidates",
      order: 2,
      title: "코드 후보",
      description: "검증이 필요한 이름 단서",
      tone: "candidate",
      total: 1,
      hidden: 0,
      emptyMessage: "후보 없음",
      items: [
        {
          id: "candidate-1",
          kind: "code-reference",
          title: "updateEmail",
          detail: "문자열 일치 단서이며 실제 의존성은 확정되지 않았습니다.",
          truthClass: "candidate",
          confidence: "high",
          rank: 1,
          evidence: [{ kind: "column-name-match", text: "email" }],
        },
      ],
    },
  ],
  markdownSummary: "# users.email",
};

const map: VisualMap = {
  id: "map-1",
  workspaceId: "workspace-1",
  mode: "column-impact",
  focus: "db:column:users:email",
  nodes: [],
  edges: [],
  warnings: [],
};
