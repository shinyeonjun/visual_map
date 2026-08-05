import { describe, expect, it } from "vitest";
import {
  codeInventoryAnalysisQuality,
  codeInventoryRouteCount,
  dbInventoryTableCount,
  type CodeInventory,
} from "./workspace";

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

  it("counts a whole-language skip as a blind spot even though the engine calls it excluded", () => {
    const inventory = {
      architecture: {
        languages: [
          {
            id: "java",
            name: "Java",
            provider: "native-lsp",
            files_found: 3863,
            files_indexed: 0,
            files_excluded: 3863,
            files_missing: 0,
            status: "excluded",
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
        ],
      },
    } as unknown as CodeInventory;

    const quality = codeInventoryAnalysisQuality(inventory);

    // `failedLanguages` stays 0 because the engine reported a deliberate skip;
    // the reader still lost 3,863 files, so the blind spot has to be separate.
    expect(quality).toMatchObject({ failedLanguages: 0, filesFound: 3884, filesIndexed: 21 });
    expect(quality?.blindSpots.map((language) => language.id)).toEqual(["java"]);
    expect(quality?.partialSpots).toEqual([]);
  });

  it("separates an incompletely indexed language from a skipped one", () => {
    const inventory = {
      architecture: {
        languages: [
          {
            id: "c",
            name: "C",
            provider: "native-lsp",
            files_found: 501,
            files_indexed: 377,
            files_excluded: 124,
            files_missing: 0,
            status: "indexed-partial",
          },
        ],
      },
    } as unknown as CodeInventory;

    const quality = codeInventoryAnalysisQuality(inventory);

    expect(quality?.blindSpots).toEqual([]);
    expect(quality?.partialSpots.map((language) => language.id)).toEqual(["c"]);
  });
});

describe("dbInventoryTableCount", () => {
  it("does not present a bounded response as the complete table count", () => {
    expect(
      dbInventoryTableCount({
        profileId: "db-1",
        tables: [{ name: "orders", columns: [] }],
        totalTables: 101,
        truncated: true,
      }),
    ).toBe(1);
    expect(
      dbInventoryTableCount({
        profileId: "db-1",
        tables: [{ name: "orders", columns: [] }],
        totalTables: 1,
      }),
    ).toBe(1);
  });
});

describe("codeInventoryRouteCount", () => {
  it("uses the exact summary when the returned route list is bounded", () => {
    const inventory = {
      routes: [{ kind: "api", detail: {} }],
      summary: { routes: 649 },
      partial: true,
    } as CodeInventory;

    expect(codeInventoryRouteCount(inventory)).toBe(649);
  });
});
