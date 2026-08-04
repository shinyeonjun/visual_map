import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CoverageNotice, analysisCoverageNotice } from "./CoverageNotice";
import type { CodeInventory } from "../../types/workspace";

function inventory(languages: unknown[]): CodeInventory {
  return { architecture: { languages } } as unknown as CodeInventory;
}

const kafka = inventory([
  {
    id: "java",
    name: "Java",
    provider: "native-lsp",
    files_found: 3863,
    files_indexed: 0,
    files_excluded: 3863,
    files_missing: 0,
    status: "excluded",
    exclusion_reason: "missing-compile-context",
  },
  {
    id: "python",
    name: "Python",
    provider: "native-lsp",
    files_found: 21,
    files_indexed: 21,
    files_excluded: 0,
    files_missing: 0,
    status: "indexed",
  },
]);

describe("analysisCoverageNotice", () => {
  it("escalates when the skipped language holds most of the project", () => {
    expect(analysisCoverageNotice(kafka)).toMatchObject({
      severity: "blocked",
      headline: "이 지도는 프로젝트 전체가 아닙니다",
      summary: "파일 3,884개 중 21개만 지도에 있습니다 (1% 미만)",
      basis: "지도의 근거: Python 21개",
    });
  });

  it("escalates when a language is nominally indexed but almost none of it landed", () => {
    // Taken from a real run on the `plane` repository: TypeScript reported
    // `indexed`, yet 8 of 3,312 files reached the map.
    const model = analysisCoverageNotice(
      inventory([
        {
          id: "python",
          name: "Python",
          provider: "native-lsp",
          files_found: 651,
          files_indexed: 651,
          files_excluded: 0,
          files_missing: 0,
          status: "indexed",
        },
        {
          id: "typescript",
          name: "TypeScript",
          provider: "scip",
          files_found: 3312,
          files_indexed: 8,
          files_excluded: 3304,
          files_missing: 0,
          status: "indexed",
        },
        {
          id: "javascript",
          name: "JavaScript",
          provider: "scip",
          files_found: 10,
          files_indexed: 2,
          files_excluded: 8,
          files_missing: 0,
          status: "indexed",
        },
      ]),
    );

    expect(model?.severity).toBe("blocked");
    expect(model?.summary).toBe("파일 3,973개 중 661개만 지도에 있습니다 (17%)");
    expect(model?.gaps.map((gap) => [gap.language.id, gap.missing, gap.effectivelyAbsent])).toEqual([
      ["typescript", 3304, true],
      ["javascript", 8, false],
    ]);
  });

  it("stays a partial warning when the indexed side still dominates", () => {
    const model = analysisCoverageNotice(
      inventory([
        {
          id: "csharp",
          name: "C#",
          provider: "scip",
          files_found: 3614,
          files_indexed: 3613,
          files_excluded: 1,
          files_missing: 0,
          status: "indexed",
        },
        {
          id: "javascript",
          name: "JavaScript",
          provider: "scip",
          files_found: 456,
          files_indexed: 0,
          files_excluded: 456,
          files_missing: 0,
          status: "excluded",
        },
      ]),
    );

    expect(model).toMatchObject({ severity: "partial", headline: "일부 언어가 지도에 없습니다" });
  });

  it("says nothing when every found file reached the index", () => {
    expect(
      analysisCoverageNotice(
        inventory([
          {
            id: "csharp",
            name: "C#",
            provider: "scip",
            files_found: 3614,
            files_indexed: 3614,
            files_excluded: 0,
            files_missing: 0,
            status: "indexed",
          },
        ]),
      ),
    ).toBeNull();
  });

  it("has nothing to say before a project is analysed", () => {
    expect(analysisCoverageNotice(null)).toBeNull();
  });
});

describe("CoverageNotice", () => {
  it("names the missing language, its size, and the reason before the map is read", () => {
    render(<CoverageNotice codeInventory={kafka} />);

    const notice = screen.getByRole("alert");
    expect(notice).toHaveAttribute("data-coverage-severity", "blocked");
    expect(notice).toHaveTextContent("Java");
    expect(notice).toHaveTextContent("3,863개 중 0개 분석 · 컴파일 정보 없음");
    expect(notice).toHaveTextContent("파일 3,884개 중 21개만 지도에 있습니다");
    expect(notice).toHaveTextContent("지도의 근거: Python 21개");
    expect(notice.querySelector('[data-coverage-kind="absent"]')).not.toBeNull();
  });

  it("renders nothing when coverage is complete", () => {
    const { container } = render(
      <CoverageNotice
        codeInventory={inventory([
          {
            id: "csharp",
            name: "C#",
            provider: "scip",
            files_found: 10,
            files_indexed: 10,
            files_excluded: 0,
            files_missing: 0,
            status: "indexed",
          },
        ])}
      />,
    );

    expect(container).toBeEmptyDOMElement();
  });
});
