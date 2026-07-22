import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import { ArchitectureMap } from "./ArchitectureMap";

describe("ArchitectureMap overview", () => {
  it("places ranked groups in fixed lanes and selects a real relationship", () => {
    const onSelectEdge = vi.fn();
    const { container } = renderArchitectureMap(
      map([
        group("group:api", "세션 API", 2, 3, 0),
        group("group:code", "세션 서비스", 0, 5, 0),
        group("group:db", "세션 저장소", 0, 1, 4),
      ], [
        edge("edge-api-code", "group:api", "group:code", "group_code_call", true),
        edge("edge-code-db", "group:code", "group:db", "structural_group_db_fk"),
      ]),
      { onSelectEdge },
    );

    expect(screen.getByText("API 경계")).toBeInTheDocument();
    expect(screen.getByText("코드 영역")).toBeInTheDocument();
    expect(screen.getByText("DB 스키마")).toBeInTheDocument();
    expect(container.querySelectorAll(".at-domain-card")).toHaveLength(3);
    expect(container.querySelectorAll("[data-architecture-edge]")).toHaveLength(2);

    fireEvent.click(container.querySelector('[data-architecture-edge="edge-api-code"]')!);
    expect(onSelectEdge).toHaveBeenCalledWith(expect.objectContaining({ id: "edge-api-code" }));
  });

  it("does not invent connections when the projection has no edges", () => {
    const { container } = renderArchitectureMap(map([
      group("group:api", "독립 API", 1, 1, 0),
      group("group:db", "독립 DB", 0, 0, 2),
    ], []));

    expect(container.querySelectorAll("[data-architecture-edge]")).toHaveLength(0);
    expect(screen.getByText("영역 간 연결 근거가 없습니다")).toBeInTheDocument();
  });

  it("keeps seven groups on the map and exposes the complete ranked list", () => {
    const groups = Array.from({ length: 9 }, (_, index) =>
      group(`group:${index + 1}`, `영역 ${index + 1}`, index === 0 ? 1 : 0, 2, 0));
    const { container } = renderArchitectureMap(map(groups, []));

    expect(container.querySelectorAll(".at-domain-card")).toHaveLength(7);
    fireEvent.click(screen.getByRole("button", { name: /전체 영역 목록/ }));
    expect(container.querySelectorAll(".at-domain-card")).toHaveLength(9);

    fireEvent.click(screen.getByRole("button", { name: /연결 지도로 돌아가기/ }));
    expect(container.querySelectorAll(".at-domain-card")).toHaveLength(7);
  });
});

function renderArchitectureMap(
  visualMap: VisualMap,
  overrides: { onSelectEdge?: (edge: VisualEdge) => void } = {},
) {
  return render(
    <ArchitectureMap
      map={visualMap}
      relationCounts={new Map()}
      selectedNodeId={null}
      selectedEdgeId={null}
      onBack={vi.fn()}
      onOpenGroup={vi.fn()}
      onOpenMember={vi.fn()}
      onSelectEdge={overrides.onSelectEdge ?? vi.fn()}
    />,
  );
}

function map(nodes: VisualNode[], edges: VisualEdge[]): VisualMap {
  return {
    id: "architecture-overview",
    workspaceId: "workspace-1",
    mode: "atlas",
    focus: "all",
    nodes,
    edges,
    warnings: [],
  };
}

function group(id: string, title: string, api: number, code: number, db: number): VisualNode {
  return {
    id,
    title,
    kind: "group-domain",
    layer: "mixed",
    source: "projection",
    subtitle: `API ${api} · 코드 ${code} · DB ${db}|/api/${title}|${title}Service|${title.toLowerCase()}`,
  };
}

function edge(id: string, from: string, to: string, kind: string, withEvidence = false): VisualEdge {
  return {
    id,
    from,
    to,
    kind,
    evidence: withEvidence ? [{ kind: "engine", text: `${from} -> ${to}` }] : [],
  };
}
