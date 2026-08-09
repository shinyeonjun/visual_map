import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MapView, Selection } from "./map/types";

const mocks = vi.hoisted(() => ({
  analyzeWorkspace: vi.fn(),
  getMapView: vi.fn(),
  getMapSelection: vi.fn(),
  setWorkspaceProvider: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock("./desktop", () => ({
  hasDesktopRuntime: () => true,
  listWorkspaces: async () => [
    {
      schemaVersion: 2,
      id: "ws-0123456789abcdef",
      name: "commerce",
      repoPath: "D:\\commerce",
      provider: { kind: "codex", model: "gpt-5.6-sol", effort: "high" },
      createdAt: 1,
      updatedAt: 1,
    },
  ],
  listProviders: async () => [
    { kind: "codex", label: "Codex", installed: true, executable: "codex", version: "0.142.5", error: null },
    { kind: "claude", label: "Claude", installed: true, executable: "claude", version: "2.1.202", error: null },
  ],
  getEngineRegistry: async () => ({ mode: "dev", engineDir: "D:\\engines", engines: [] }),
  getFactGraphStatus: async () => ({
    schemaVersion: 1,
    snapshotId: null,
    sourceRevision: null,
    nodeCount: 0,
    edgeCount: 0,
    evidenceCount: 0,
    coverageCount: 0,
  }),
  analyzeWorkspace: mocks.analyzeWorkspace,
  getMapView: mocks.getMapView,
  getMapSelection: mocks.getMapSelection,
  chooseRepositoryFolder: vi.fn(),
  createWorkspace: vi.fn(),
  setWorkspaceProvider: mocks.setWorkspaceProvider,
}));

import App from "./App";

const map: MapView = {
  areas: [
    {
      id: "area-orders",
      name: "주문",
      originalName: "orders",
      summary: "주문을 처리합니다.",
      depth: 0,
      areas: [],
      nodes: [],
      hiddenNodeCount: 0,
      position: { x: 100, y: 100 },
      width: 240,
    },
  ],
  relations: [],
};

const selection: Selection = {
  id: "area-orders",
  title: "주문",
  role: "주문을 처리합니다.",
  relations: [],
  evidence: [{ path: "src/orders/service.ts", line: 12 }],
  source: null,
};

describe("analysis vertical slice", () => {
  beforeEach(() => {
    mocks.analyzeWorkspace.mockReset();
    mocks.getMapView.mockReset();
    mocks.getMapSelection.mockReset();
    mocks.setWorkspaceProvider.mockReset();
    mocks.getMapView.mockResolvedValueOnce(null).mockResolvedValue(map);
    mocks.analyzeWorkspace.mockResolvedValue({
      factGraph: {
        schemaVersion: 1,
        snapshotId: "snapshot-a",
        sourceRevision: "source-a",
        nodeCount: 12,
        edgeCount: 8,
        evidenceCount: 8,
        coverageCount: 3,
      },
      semanticRevisionId: "semantic-a",
      semanticError: null,
    });
    mocks.getMapSelection.mockResolvedValue(selection);
    mocks.setWorkspaceProvider.mockResolvedValue({
      schemaVersion: 2,
      id: "ws-0123456789abcdef",
      name: "commerce",
      repoPath: "D:\\commerce",
      provider: { kind: "codex", model: "gpt-5.6-terra", effort: "xhigh" },
      createdAt: 1,
      updatedAt: 2,
    });
  });

  it("replaces the empty ledger with the published map and loads selection evidence", async () => {
    render(<App />);
    const start = await screen.findByRole("button", { name: "코드 분석 시작" });
    fireEvent.click(start);

    expect(await screen.findByText("주문을 처리합니다.")).toBeVisible();
    expect(mocks.analyzeWorkspace).toHaveBeenCalledWith("ws-0123456789abcdef");
    fireEvent.click(screen.getByRole("button", { name: /주문.*orders.*L0/ }));

    await waitFor(() => expect(mocks.getMapSelection).toHaveBeenCalledWith("ws-0123456789abcdef", "area-orders"));
    expect(await screen.findByText("service.ts:12")).toBeVisible();
  });

  it("shows the exact CLI model and defaults reasoning to high", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /GPT-5\.6 Sol.*높음/ }));

    expect(await screen.findByRole("dialog", { name: "AI 모델 설정" })).toBeVisible();
    expect(screen.getByRole("radio", { name: /GPT-5\.6 Sol/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: "높음high" })).toBeChecked();
    expect(screen.getByText('codex --model gpt-5.6-sol --config model_reasoning_effort="high"')).toBeVisible();

    fireEvent.click(screen.getByRole("radio", { name: /GPT-5\.6 Terra/ }));
    fireEvent.click(screen.getByRole("radio", { name: "매우 높음xhigh" }));
    fireEvent.click(screen.getByRole("button", { name: "설정 저장" }));

    await waitFor(() =>
      expect(mocks.setWorkspaceProvider).toHaveBeenCalledWith("ws-0123456789abcdef", "codex", "gpt-5.6-terra", "xhigh"),
    );
  });
});
