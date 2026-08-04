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
  evidence: {
    schema: "code-memory.evidence-summary.v1",
    collectors: [
      {
        id: "contracts",
        capability: "api-contracts",
        mode: "passive",
        status: "collected",
        detectedBy: ["openapi.yaml"],
        detectedByTotal: 1,
        factCount: 2,
        relationCount: 1,
        diagnosticCount: 0,
      },
    ],
    factCount: 2,
    relationCount: 1,
    diagnosticCount: 0,
    diagnostics: [],
    diagnosticsHidden: 0,
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
        engines: [
          {
            id: "code",
            label: "Code",
            role: "code",
            executable: "code.exe",
            expectedVersion: "1",
            contractVersion: "1",
            path: "C:/engines/code.exe",
            available: true,
            releasable: true,
            integrity: "release",
          },
        ],
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
      validateCodeIndexResult({
        workspace,
        run,
        inventory: { ...codeInventory, evidence: { ...codeInventory.evidence, factCount: "2" } },
      }),
    ).toThrow("코드 프로젝트 근거 factCount");
    expect(() =>
      validateDbIndexResult({ workspace, run, inventory: { ...dbInventory, tables: [{ name: "orders" }] } }),
    ).toThrow("DB 컬럼 목록");
    expect(() =>
      validateEngineRegistry({
        mode: "internal",
        engineDir: "C:/engines",
        engines: [
          {
            id: "code",
            label: "Code",
            executable: "code.exe",
            expectedVersion: "1",
            contractVersion: "1",
            path: "C:/engines/code.exe",
            available: true,
            releasable: true,
            integrity: "release",
          },
        ],
      }),
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
        snapshot: {
          workspaceId: workspace.id,
          savedAt: "2026-01-01T00:00:00Z",
          metadata: { evidence: codeInventory.evidence },
          items: [],
          links: [],
        },
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
    expect(() =>
      validateVisualMap({
        id: "map-1",
        workspaceId: "workspace-1",
        mode: "atlas",
        focus: "overview",
        nodes: [
          { id: "node-1", kind: "code", title: "A" },
          { id: "node-1", kind: "code", title: "B" },
        ],
        edges: [],
        warnings: [],
      }),
    ).toThrow("시각화 항목 ID가 중복");
    expect(() =>
      validateVisualMap({
        id: "map-1",
        workspaceId: "workspace-1",
        mode: "atlas",
        focus: "overview",
        nodes: [{ id: "node-1", kind: "code", title: "A" }],
        edges: [{ id: "edge-1", from: "node-1", to: "missing", kind: "calls" }],
        warnings: [],
      }),
    ).toThrow("존재하지 않는 항목");
  });

  it("accepts structured architecture metrics and rejects malformed weights", () => {
    const map = {
      id: "map-structured",
      workspaceId: "workspace-1",
      mode: "atlas",
      focus: "overview",
      nodes: [
        {
          id: "group:orders",
          kind: "group-domain",
          title: "주문",
          metrics: {
            memberCount: 4,
            apiCount: 1,
            codeCount: 2,
            dbCount: 1,
            topApi: ["GET /orders"],
            topCode: ["OrderService"],
            topDb: ["orders"],
            inDegree: 7,
            outDegree: 5,
          },
          coverage: { languages: ["typescript"], hasBlindSpot: false, hasPartial: true },
        },
      ],
      edges: [],
      warnings: [],
    };

    expect(validateVisualMap(map).nodes[0].coverage?.hasPartial).toBe(true);
    expect(() =>
      validateVisualMap({
        ...map,
        edges: [{ id: "edge-1", from: "group:orders", to: "group:orders", kind: "calls", weight: -1 }],
      }),
    ).toThrow("시각화 관계 가중치");
  });

  it("rejects duplicate provider identities before rendering", () => {
    expect(() =>
      validateCodeIndexResult({
        workspace,
        run,
        inventory: {
          ...codeInventory,
          files: [...codeInventory.files, { id: "file:main", kind: "file", name: "duplicate.ts" }],
        },
      }),
    ).toThrow("코드 항목 ID가 중복");
    expect(() =>
      validateDbIndexResult({
        workspace,
        run,
        inventory: {
          ...dbInventory,
          tables: [...dbInventory.tables, { name: "orders", columns: [{ name: "id" }] }],
        },
      }),
    ).toThrow("DB 테이블 식별자가 중복");
  });
});
