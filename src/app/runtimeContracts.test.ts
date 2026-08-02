import { describe, expect, it } from "vitest";
import {
  validateCodeIndexResult,
  validateDbIndexResult,
  validateEngineRegistry,
  validateInventoryBootstrap,
  validateInventorySearchResult,
  validateVisualMap,
  validateWorkspaceAnalysisResult,
} from "./runtimeContracts";

const workspace = {
  id: "workspace-1",
  name: "fixture",
  repoPath: "C:/fixture",
  repoSource: "local",
  codeProject: "fixture",
  engineCache: {},
  dbProfiles: [],
  activeDbProfileId: null,
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

const codeInventory = {
  project: "fixture",
  routes: [],
  services: [],
  files: [{ id: "file:main", kind: "file", name: "main.ts" }],
  handlers: [],
  repositories: [],
  functions: [],
  classes: [],
  modules: [],
  unknown: [],
  summary: {
    routes: 0,
    handlers: 0,
    services: 0,
    repositories: 0,
    functions: 0,
    classes: 0,
    modules: 0,
    files: 1,
    unknown: 0,
  },
  calls: [],
};

const dbInventory = {
  profileId: "db-1",
  tables: [{ name: "orders", columns: [{ name: "id" }] }],
};

const run = { ok: true, stderr: "" };

describe("runtime engine contracts", () => {
  it("accepts the minimum valid engine responses", () => {
    expect(
      validateEngineRegistry({
        mode: "internal",
        engineDir: "C:/engines",
        engines: [{ id: "code", role: "code", path: "C:/engines/code.exe", integrity: "release" }],
      }).engines,
    ).toHaveLength(1);
    expect(validateCodeIndexResult({ workspace, run, inventory: codeInventory }).inventory?.project).toBe("fixture");
    expect(validateDbIndexResult({ workspace, run, inventory: dbInventory }).inventory?.tables[0].name).toBe("orders");
  });

  it("rejects malformed provider output before it reaches the UI", () => {
    expect(() => validateCodeIndexResult({ workspace, run, inventory: { ...codeInventory, calls: {} } })).toThrow(
      "코드 호출 관계",
    );
    expect(() =>
      validateDbIndexResult({ workspace, run, inventory: { ...dbInventory, tables: [{ name: "orders" }] } }),
    ).toThrow("DB 컬럼 목록");
    expect(() =>
      validateEngineRegistry({ mode: "internal", engineDir: "C:/engines", engines: [{ id: "code" }] }),
    ).toThrow("읽기 도구 역할");
  });

  it("validates integrated analysis and saved snapshot envelopes", () => {
    const result = validateWorkspaceAnalysisResult({
      workspace,
      code: { workspace, run, inventory: codeInventory },
      db: null,
      snapshotSaved: true,
    });
    expect(result.snapshotSaved).toBe(true);
    expect(
      validateInventoryBootstrap({
        snapshot: { workspaceId: workspace.id, savedAt: "2026-01-01T00:00:00Z", items: [], links: [] },
        summary: { workspaceId: workspace.id },
      })?.snapshot.workspaceId,
    ).toBe(workspace.id);
  });

  it("fails closed for a malformed saved snapshot", () => {
    expect(() =>
      validateInventoryBootstrap({
        snapshot: { workspaceId: workspace.id, savedAt: "2026-01-01T00:00:00Z", items: [{ id: "x" }], links: [] },
        summary: { workspaceId: workspace.id },
      }),
    ).toThrow("저장된 항목 종류");
  });

  it("rejects malformed visual maps and search envelopes", () => {
    expect(
      validateVisualMap({
        id: "map-1",
        workspaceId: "workspace-1",
        mode: "atlas",
        focus: "overview",
        nodes: [],
        edges: [],
        warnings: [],
      }).mode,
    ).toBe("atlas");
    expect(validateInventorySearchResult({ hits: [], total: 0, counts: {}, truncated: false }).total).toBe(0);
    expect(() =>
      validateVisualMap({
        id: "map-1",
        workspaceId: "workspace-1",
        mode: "atlas",
        focus: "overview",
        nodes: {},
        edges: [],
        warnings: [],
      }),
    ).toThrow("시각화 항목");
    expect(() => validateInventorySearchResult({ hits: [], total: "0", counts: {}, truncated: false })).toThrow(
      "검색 요약",
    );
  });
});
