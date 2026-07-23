import { describe, expect, it } from "vitest";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  columnImpactSummary,
  copyValuesForNode,
  edgeTrustLabel,
  edgeTrustTone,
  inspectorAnswer,
  nodeEvidenceSummary,
  relationshipReason,
} from "./inspectorModel";
import { visualEdgeKindLabel, visualNodeKindLabel } from "../../visual/labels";

const node: VisualNode = {
  id: "db:table:public.orders",
  kind: "table",
  title: "orders",
  subtitle: "public",
  layer: "data",
  source: "db",
};

function mapWith(edge: VisualEdge): VisualMap {
  return {
    id: "map",
    workspaceId: "workspace",
    mode: "table-usage",
    focus: node.id,
    nodes: [node],
    edges: [edge],
    warnings: [],
  };
}

describe("inspector trust model", () => {
  it("never presents a strong name match as a confirmed relationship", () => {
    const candidate: VisualEdge = {
      id: "candidate",
      from: "code:repository",
      to: node.id,
      kind: "candidate_table",
      confidence: "high",
      evidence: [{ kind: "name-match", text: "orders identifier match" }],
    };

    expect(edgeTrustLabel(candidate)).toBe("후보 단서 강함");
    expect(edgeTrustTone(candidate)).toBe("amber");
    expect(nodeEvidenceSummary(node, mapWith(candidate))).toMatchObject({
      confidence: "후보 단서 강함",
      badgeTone: "amber",
      connectionSummary: "확정 0 · 구조 0 · 후보 1 · 이름 단서 0",
    });
  });

  it("labels containment without evidence as structural, not direct", () => {
    const structural: VisualEdge = {
      id: "structural",
      from: "group:package:app",
      to: node.id,
      kind: "group_contains",
      confidence: null,
      evidence: [],
    };

    expect(edgeTrustLabel(structural)).toBe("구조 근거");
    expect(edgeTrustTone(structural)).toBe("gray");
  });

  it("keeps grouped code-call evidence classified as code, not database structure", () => {
    const groupedCall: VisualEdge = {
      id: "group-call",
      from: "group:package:app",
      to: "group:package:scripts",
      kind: "group_code_call",
      evidence: [{ kind: "engine-edge", text: "codebase-memory CALLS" }],
    };
    const map: VisualMap = {
      id: "atlas",
      workspaceId: "workspace",
      mode: "atlas",
      focus: "all",
      nodes: [
        { id: groupedCall.from, kind: "group-domain", title: "app", layer: "mixed", source: "projection" },
        { id: groupedCall.to, kind: "group-domain", title: "scripts", layer: "mixed", source: "projection" },
      ],
      edges: [groupedCall],
      warnings: [],
    };

    const answer = inspectorAnswer({
      edge: groupedCall,
      node: null,
      code: null,
      table: null,
      column: null,
      map,
      dbNeedsColumns: false,
      dbMissingColumnTables: 0,
      dbTableCount: 0,
      codeItemCount: 2,
      hasWorkspace: true,
      needsGithub: false,
    });

    expect(answer).toMatchObject({
      title: "app → scripts",
      sentence: "읽은 코드에서 확인된 1차 근거입니다.",
    });
    expect(answer.metrics).toContainEqual(expect.objectContaining({ label: "관계", value: "코드 호출" }));
  });

  it("does not count parent containment as a direct column impact", () => {
    const column: VisualNode = {
      id: "db:column:public.orders:created_at",
      kind: "column",
      title: "created_at",
      layer: "data",
      source: "db",
    };
    const map: VisualMap = {
      id: "column-impact",
      workspaceId: "workspace",
      mode: "column-impact",
      focus: column.id,
      nodes: [column],
      edges: [
        {
          id: "contains",
          from: node.id,
          to: column.id,
          kind: "contains",
          evidence: [{ kind: "schema", text: "orders contains created_at" }],
        },
        {
          id: "candidate",
          from: "code:model",
          to: column.id,
          kind: "candidate_column",
          evidence: [{ kind: "name-match", text: "created_at" }],
        },
      ],
      warnings: [],
    };

    expect(columnImpactSummary(column, map)).toMatchObject({
      directCount: 0,
      candidateCount: 1,
    });
  });

  it("keeps database dependent objects distinct from code symbols", () => {
    const viewNode: VisualNode = {
      id: "db:view:active-orders",
      kind: "view",
      title: "active_orders",
      layer: "data",
      source: "db",
    };
    const dependency: VisualEdge = {
      id: "view-orders",
      from: viewNode.id,
      to: node.id,
      kind: "db_dependency",
      evidence: [{ kind: "db-dependency", text: "active_orders 뷰가 orders 테이블을 참조합니다" }],
    };
    const map: VisualMap = {
      id: "table-impact",
      workspaceId: "workspace",
      mode: "table-usage",
      focus: node.id,
      nodes: [node, viewNode],
      edges: [dependency],
      warnings: [],
    };

    expect(visualNodeKindLabel(viewNode.kind, viewNode.source)).toBe("뷰");
    expect(visualNodeKindLabel("materialized-view", "db")).toBe("DB 객체");
    expect(visualEdgeKindLabel(dependency)).toBe("DB 의존성");
    expect(copyValuesForNode(viewNode)[0]).toEqual(["뷰", "active_orders"]);
    expect(relationshipReason({ ...dependency, evidence: [] })).toBe(
      "DB 뷰 또는 함수/프로시저가 참조하는 테이블이나 컬럼입니다",
    );
    expect(
      inspectorAnswer({
        edge: null,
        node: viewNode,
        code: null,
        table: null,
        column: null,
        map,
        dbNeedsColumns: false,
        dbMissingColumnTables: 0,
        dbTableCount: 1,
        codeItemCount: 0,
        hasWorkspace: true,
        needsGithub: false,
      }),
    ).toMatchObject({
      kicker: "뷰 근거",
      sentence: "이 뷰가 참조하는 테이블과 컬럼의 DB 근거입니다.",
      steps: ["참조하는 테이블/컬럼 확인"],
    });
  });

  it.each([
    ["constraint", "제약"],
    ["index", "인덱스"],
    ["trigger", "트리거"],
    ["routine", "DB 함수/프로시저"],
  ])("labels the %s database node precisely", (kind, label) => {
    expect(visualNodeKindLabel(kind, "db")).toBe(label);
  });
});
