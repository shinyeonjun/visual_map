import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { VisualMapControls } from "../../types/controls";
import { SearchResultsPopover } from "./SearchResultsPopover";

describe("SearchResultsPopover", () => {
  it("labels supported results as additions while relationship mode is active", () => {
    const selectSearchResult = vi.fn();
    const controls = {
      mode: "composition",
      searchQuery: "orders",
      searchPopoverOpen: true,
      searchSummary: "1개 결과",
      searchGroups: [{
        title: "DB 테이블",
        results: [{
          id: "table:public.orders",
          title: "public.orders",
          subtitle: "테이블",
          focusId: "db:table:public.orders",
          tableKey: "public.orders",
        }],
      }],
      closeSearchPopover: vi.fn(),
      selectSearchResult,
    } as unknown as VisualMapControls;

    render(<SearchResultsPopover visualMapControls={controls} searchScope="전체 인벤토리" />);

    const result = screen.getByRole("button", { name: /선택에 추가/ });
    fireEvent.click(result);
    expect(selectSearchResult).toHaveBeenCalledWith(controls.searchGroups[0].results[0]);
  });
});
