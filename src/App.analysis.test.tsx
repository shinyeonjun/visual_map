import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MapView, Selection } from "./map/types";

const mocks = vi.hoisted(() => ({
  analyzeWorkspace: vi.fn(),
  getMapView: vi.fn(),
  getMapSelection: vi.fn(),
  setWorkspaceProvider: vi.fn(),
  cancelWorkspaceAnalysis: vi.fn(),
  deleteWorkspace: vi.fn(),
  openSourceLocation: vi.fn(),
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
  cancelWorkspaceAnalysis: mocks.cancelWorkspaceAnalysis,
  getMapView: mocks.getMapView,
  getMapSelection: mocks.getMapSelection,
  chooseRepositoryFolder: vi.fn(),
  createWorkspace: vi.fn(),
  deleteWorkspace: mocks.deleteWorkspace,
  openSourceLocation: mocks.openSourceLocation,
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
      category: "domain",
      labelSource: "semantic",
      fallbackReason: null,
      depth: 0,
      areas: [],
      nodes: [],
      hiddenNodeCount: 0,
      position: { x: 100, y: 100 },
      width: 240,
      boundaryRelationCounts: { verified: 0, structural: 0, candidate: 0 },
      affectingAnalysisGapCount: 0,
    },
  ],
  relations: [],
  unattributedAnalysisGapCount: 0,
};

const selection: Selection = {
  id: "area-orders",
  title: "주문",
  role: "주문을 처리합니다.",
  relations: [],
  evidence: [{ path: "src/orders/service.ts", line: 12 }],
  source: null,
  analysisGaps: { totalCount: 0, items: [], truncatedCount: 0 },
};

const evidenceConsentKey = "codebase-workspace.ai-source-evidence-consent.v1:ws-0123456789abcdef:codex";

describe("analysis vertical slice", () => {
  beforeEach(() => {
    window.localStorage.clear();
    window.localStorage.setItem(evidenceConsentKey, "accepted");
    mocks.analyzeWorkspace.mockReset();
    mocks.getMapView.mockReset();
    mocks.getMapSelection.mockReset();
    mocks.setWorkspaceProvider.mockReset();
    mocks.cancelWorkspaceAnalysis.mockReset();
    mocks.deleteWorkspace.mockReset();
    mocks.openSourceLocation.mockReset();
    mocks.cancelWorkspaceAnalysis.mockResolvedValue(true);
    mocks.deleteWorkspace.mockResolvedValue(undefined);
    mocks.openSourceLocation.mockResolvedValue({
      path: "D:\\commerce\\src\\orders\\service.ts",
      line: 12,
      column: null,
      action: "vscode",
    });
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

    const areaButton = await screen.findByRole("button", { name: /주문.*orders.*L0/ });
    expect(areaButton).toBeVisible();
    expect(screen.getByRole("button", { name: "재분석" })).toHaveAttribute(
      "title",
      "이전 분석 결과를 재사용하지 않고 현재 코드를 처음부터 다시 분석합니다",
    );
    expect(mocks.analyzeWorkspace).toHaveBeenCalledWith("ws-0123456789abcdef", "fresh");
    fireEvent.click(areaButton);

    await waitFor(() => expect(mocks.getMapSelection).toHaveBeenCalledWith("ws-0123456789abcdef", "area-orders"));
    const evidence = await screen.findByRole("button", { name: "service.ts:12" });
    expect(evidence).toBeVisible();
    fireEvent.click(evidence);
    await waitFor(() =>
      expect(mocks.openSourceLocation).toHaveBeenCalledWith(
        "ws-0123456789abcdef",
        "src/orders/service.ts",
        12,
        null,
        "vscode",
      ),
    );
  });

  it("cancels the static engine and every semantic child through one workspace action", async () => {
    const deferred: { resolve?: (value: unknown) => void } = {};
    mocks.analyzeWorkspace.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          deferred.resolve = resolve;
        }),
    );
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "코드 분석 시작" }));

    const cancel = await screen.findAllByRole("button", { name: /분석.*취소/ });
    expect(cancel[0]).toHaveTextContent("분석 중 · 취소");
    expect(cancel[0]).not.toHaveTextContent("%");
    fireEvent.click(cancel[0]);
    await waitFor(() => expect(mocks.cancelWorkspaceAnalysis).toHaveBeenCalledWith("ws-0123456789abcdef"));

    deferred.resolve?.({
      factGraph: {
        schemaVersion: 1,
        snapshotId: null,
        sourceRevision: null,
        nodeCount: 0,
        edgeCount: 0,
        evidenceCount: 0,
        coverageCount: 0,
      },
      semanticRevisionId: null,
      semanticError: null,
    });
    await waitFor(() => expect(mocks.getMapView).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("button", { name: /분석.*취소/ })).not.toBeInTheDocument());
  });

  it("keeps long engine diagnostics collapsed behind a short error summary", async () => {
    const diagnostic = `의미 지도 검증에 실패했습니다: ${"trace crosses an unrelated region ".repeat(30)}`;
    mocks.analyzeWorkspace.mockRejectedValueOnce(diagnostic);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "코드 분석 시작" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toBeVisible();
    expect(alert).toHaveTextContent("의미 지도 검증에 실패했습니다");
    const details = screen.getByText("자세히").closest("details");
    expect(details).not.toHaveAttribute("open");
    expect(details?.querySelector("pre")).toHaveTextContent(diagnostic.trim());
  });

  it("keeps newly published static facts visible when semantic map generation fails", async () => {
    mocks.getMapView.mockReset().mockResolvedValue(null);
    mocks.analyzeWorkspace.mockResolvedValueOnce({
      factGraph: {
        schemaVersion: 1,
        snapshotId: "snapshot-new",
        sourceRevision: "source-new",
        nodeCount: 21,
        edgeCount: 13,
        evidenceCount: 34,
        coverageCount: 8,
      },
      semanticRevisionId: null,
      semanticError: "provider unavailable",
    });
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "코드 분석 시작" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("코드 사실은 저장됐지만 의미 지도를 만들지 못했습니다");
    const ledger = screen.getByLabelText("분석 원장");
    expect(ledger).toHaveTextContent("노드21");
    expect(ledger).toHaveTextContent("관계13");
    expect(ledger).toHaveTextContent("근거34");
    expect(ledger).toHaveTextContent("파일8");
    expect(await screen.findByText("snapshot snapshot-new")).toBeVisible();
    expect(mocks.getMapView).toHaveBeenCalledTimes(2);
  });

  it("requires explicit consent before source evidence can cross the AI provider boundary", async () => {
    window.localStorage.removeItem(evidenceConsentKey);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "코드 분석 시작" }));

    const dialog = await screen.findByRole("dialog", { name: "AI 코드 근거 전송 동의" });
    expect(dialog).toHaveTextContent("선택된 소스 코드 근거 발췌");
    expect(dialog).toHaveTextContent("외부 AI 서비스로 전송될 수 있습니다");
    expect(dialog).toHaveTextContent("알려진 비밀값 패턴은 전송 전에 마스킹");
    expect(mocks.analyzeWorkspace).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "동의하고 분석" }));
    await waitFor(() => expect(mocks.analyzeWorkspace).toHaveBeenCalledWith("ws-0123456789abcdef", "fresh"));
    expect(window.localStorage.getItem(evidenceConsentKey)).toBe("accepted");
  });

  it("deletes only the app workspace after explicit confirmation", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "commerce 프로젝트 삭제" }));
    expect(await screen.findByRole("dialog", { name: "프로젝트 삭제 확인" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "앱에서 삭제" }));
    await waitFor(() => expect(mocks.deleteWorkspace).toHaveBeenCalledWith("ws-0123456789abcdef"));
    expect(screen.queryByText("commerce")).not.toBeInTheDocument();
    expect(window.localStorage.getItem(evidenceConsentKey)).toBeNull();
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
