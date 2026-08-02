import { describe, expect, it } from "vitest";
import { codeInventoryAnalysisQuality, type CodeInventory } from "./workspace";

describe("codeInventoryAnalysisQuality", () => {
  it("normalizes engine quality summaries and preserves partial states", () => {
    const inventory = {
      architecture: {
        languages: [
          {
            id: "python",
            name: "Python",
            provider: "native-lsp",
            files_found: 3,
            files_indexed: 2,
            files_missing: 1,
            status: "indexed-partial",
            exclusion_reason: "missing-compile-context",
            exclusion_scope: "language",
          },
        ],
        frameworks: [
          {
            id: "fastapi",
            language: "python",
            name: "FastAPI",
            adapter: "registration-routing",
            status: "detected",
            fact_count: 2,
            relation_count: 1,
          },
        ],
      },
    } as unknown as CodeInventory;

    expect(codeInventoryAnalysisQuality(inventory)).toMatchObject({
      indexedLanguages: 0,
      partialLanguages: 1,
      failedLanguages: 0,
      detectedFrameworks: 1,
      languages: [
        {
          filesFound: 3,
          filesIndexed: 2,
          filesMissing: 1,
          exclusionReason: "missing-compile-context",
          exclusionScope: "language",
        },
      ],
      frameworks: [{ factCount: 2, relationCount: 1 }],
    });
  });

  it("returns no quality summary for legacy architecture payloads", () => {
    expect(codeInventoryAnalysisQuality({ architecture: { nodes: [] } } as unknown as CodeInventory)).toBeNull();
  });
});
