import { Copy, LoaderCircle } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { dbProfileWorkStarted } from "../../types/controls";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { codeInventoryItemCount } from "../../types/workspace";
import type { CodeInventory, CodeInventoryItem } from "../../types/workspace";
import type { VisualEdge, VisualMap, VisualNode } from "../../types/visual-map";
import {
  visualEdgeKindLabel as edgeKindLabel,
  visualEdgeTruthClass,
  visualNodeKindLabel as nodeKindLabel,
} from "../../visual/labels";
import { copyValue } from "../../components/common/copyValue";
import type { InspectorAction, InspectorAnswer } from "./mapInspectorModel";

const INSPECTOR_EVIDENCE_LABELS: Record<string, string> = {
  "route-mount": "경로 근거",
  "route-source": "소스 근거",
  "route-binding": "라우트 연결",
  "code-call": "호출 관계",
  "code-handle": "라우트 연결",
  "code-db-read": "DB 조회",
  "code-db-write": "DB 변경",
  "code-db-column": "컬럼 사용",
  "db-constraint": "DB 제약",
  "db-dependency": "DB 의존",
  "db-trigger": "DB 트리거",
  "snapshot-link": "확정 연결",
  "engine-node": "코드 항목",
  "engine-edge": "관계 근거",
  "engine-confidence": "신뢰 수준",
  "engine-confidence-score": "신뢰 점수",
  "engine-strategy": "분석 방식",
  "engine-callee": "호출 표현",
  "candidate-source": "후보 출처",
  "static-sql": "정적 SQL",
};

const INSPECTOR_CONFIDENCE_LABELS: Record<string, string> = {
  high: "높음",
  medium: "중간",
  low: "낮음",
  unknown: "확인 필요",
};

const INSPECTOR_STRATEGY_LABELS: Record<string, string> = {
  lsp_direct: "LSP 직접 확인",
  lsp_implicit_this: "LSP 현재 객체 추적",
  lsp_type_dispatch: "LSP 타입 추적",
  lsp_virtual_dispatch: "LSP 가상 호출 추적",
  import_map: "import 연결 확인",
  import_map_suffix: "import 경로 추적",
  same_module: "같은 모듈 확인",
  service_pattern: "프레임워크 패턴 확인",
  unique_name: "고유 이름 일치",
};

const INSPECTOR_ENGINE_EDGE_LABELS: Record<string, string> = {
  "codebase-memory CALLS": "코드 엔진에서 호출 관계를 확인했습니다.",
  "codebase-memory HANDLES: upstream handler→route was normalized to product route→handler":
    "코드 엔진의 핸들러→라우트 관계를 제품의 라우트→핸들러 읽기 방향으로 정규화했습니다.",
};

export function InspectorUpdating() {
  return (
    <div className="inspector-update-indicator" role="status" aria-live="polite">
      <LoaderCircle className="spin" size={13} />
      분석 업데이트 중
    </div>
  );
}

export function apiEdgeLabel(edge: VisualEdge): string {
  if (edge.kind === "code_handle") return "HANDLES";
  if (edge.kind === "code_call") return "CALLS";
  if (visualEdgeTruthClass(edge) === "candidate") return "DB 후보";
  return edgeKindLabel(edge);
}

export function requiresReview(edge: VisualEdge): boolean {
  const truthClass = visualEdgeTruthClass(edge);
  return truthClass === "candidate" || truthClass === "inferred";
}

export function uniqueInspectorEvidence<T extends { key: string; text: string }>(items: T[]): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = item.text;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function inspectorEvidenceText(kind: string, text: string): string {
  const value =
    kind === "engine-confidence"
      ? (INSPECTOR_CONFIDENCE_LABELS[text] ?? text)
      : kind === "engine-strategy"
        ? (INSPECTOR_STRATEGY_LABELS[text] ?? text)
        : kind === "engine-edge"
          ? (INSPECTOR_ENGINE_EDGE_LABELS[text] ?? text)
          : text;
  return `${INSPECTOR_EVIDENCE_LABELS[kind] ?? "근거"}: ${value}`;
}

export function InspectorSection({
  title,
  count,
  children,
}: {
  title: string;
  count?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="inspector-section">
      <header>
        <strong>{title}</strong>
        {count !== undefined ? <span>{count}</span> : null}
      </header>
      {children}
    </section>
  );
}

export function InspectorEmptyRow({ children }: { children: ReactNode }) {
  return <p className="inspector-empty-row">{children}</p>;
}

export function AnswerSummary({
  answer,
  /**
   * The identity block above already names the target. Repeating it here made
   * the panel state the same string twice before saying anything about it.
   */
  hideTitle = false,
}: {
  answer: InspectorAnswer;
  hideTitle?: boolean;
}) {
  const [firstStep] = answer.steps;

  return (
    <div className={`answer-summary ${answer.tone}`}>
      <div className="answer-head">
        <span>{answer.kicker}</span>
        {hideTitle ? null : <strong title={answer.title}>{answer.title}</strong>}
        <em className="answer-verdict">{answerVerdict(answer)}</em>
      </div>
      <p>{answer.sentence}</p>
      {firstStep && (
        <div className="answer-lead">
          <span>먼저 볼 것</span>
          <b>{firstStep}</b>
        </div>
      )}
      {answer.note && <small>{answer.note}</small>}
    </div>
  );
}

function answerVerdict(answer: InspectorAnswer): string {
  if (answer.tone === "confirmed") {
    return "확정";
  }
  if (answer.tone === "candidate") {
    if (answer.metrics.some((metric) => metric.label === "컬럼" && metric.value === "0")) {
      return "보강";
    }
    return "후보";
  }
  if (answer.kicker === "시작" || answer.kicker === "다음 행동") {
    return "대기";
  }
  if (answer.title === "관계 없음" || answer.kicker === "코드/DB 목록") {
    return "관계 없음";
  }
  return "구조 근거";
}

export function CopyRow({ values, label = "복사" }: { values: Array<[string, string]>; label?: string }) {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const visibleValues = values.filter(([, value]) => value.trim().length > 0);
  if (visibleValues.length === 0) {
    return null;
  }

  return (
    <div className="copy-row" aria-label={`${label} 도구`}>
      <span className="copy-row-title">{label}</span>
      {visibleValues.map(([label, value]) => {
        const key = `${label}-${value}`;
        return (
          <button
            type="button"
            key={key}
            onClick={() => {
              void copyValue(value).then((copied) => {
                if (!copied) {
                  return;
                }
                setCopiedKey(key);
                window.setTimeout(() => {
                  setCopiedKey((current) => (current === key ? null : current));
                }, 1200);
              });
            }}
          >
            <Copy size={12} />
            <span>{copiedKey === key ? "복사됨" : label}</span>
          </button>
        );
      })}
    </div>
  );
}

export function codeInventoryItemById(inventory: CodeInventory | null, id: string | null): CodeInventoryItem | null {
  if (!inventory || !id) {
    return null;
  }
  for (const items of [
    inventory.routes,
    inventory.services,
    inventory.files,
    inventory.handlers,
    inventory.repositories,
    inventory.functions,
    inventory.classes,
    inventory.modules,
    inventory.unknown,
  ]) {
    const item = items.find((candidate) => candidate.id === id);
    if (item) {
      return item;
    }
  }
  return null;
}

export function codeInventoryItemFromNode(node: VisualNode | null): CodeInventoryItem | null {
  if (node?.source !== "code" || !node.location?.path) {
    return null;
  }
  return {
    id: node.id.replace(/^code:/, ""),
    kind: node.kind,
    name: node.title,
    filePath: node.location.path,
    line: node.location.line ?? null,
    column: node.location.column ?? null,
    endLine: node.location.endLine ?? null,
    endColumn: node.location.endColumn ?? null,
    detail: null,
  };
}

export function edgeEndpointAction(
  edge: VisualEdge,
  map: VisualMap | null,
  onSelect: (node: VisualNode) => void,
): InspectorAction | null {
  const toNode = map?.nodes.find((item) => item.id === edge.to) ?? null;
  const fromNode = map?.nodes.find((item) => item.id === edge.from) ?? null;
  const target = toNode ?? fromNode;
  if (!target) {
    return null;
  }
  return {
    label: toNode ? "연결 대상 보기" : "기준 보기",
    run: () => onSelect(target),
    primary: true,
  };
}

export function inspectorEmptyAction(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
  showDbSetup: () => void,
): InspectorAction | null {
  if (!workspaceControls.currentWorkspace) {
    return null;
  }

  const firstVisibleNode = visualMapControls.currentMap?.nodes[0] ?? null;
  if (firstVisibleNode) {
    return {
      label: firstVisibleNodeActionLabel(firstVisibleNode),
      run: () => visualMapControls.selectNode(firstVisibleNode),
      primary: true,
    };
  }

  const dbStarted = dbProfileWorkStarted(dbProfileControls);

  if (!workspaceControls.codeInventory && !dbStarted) {
    if (!workspaceControls.canIndexCode) {
      return {
        label: "DB 정보 입력",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return {
      label: "코드 읽기",
      run: workspaceControls.indexCodeRepository,
      primary: true,
      disabled: workspaceControls.busy,
    };
  }

  if (!dbProfileControls.inventory) {
    if (!dbProfileControls.activeProfile) {
      return dbProfileControls.canSaveProfile
        ? { label: "DB 연결 저장", run: dbProfileControls.saveProfile, primary: true, disabled: dbProfileControls.busy }
        : { label: "DB 정보 입력", run: showDbSetup, disabled: dbProfileControls.busy };
    }
    if (dbProfileControls.dbIndexBlockedReason) {
      return {
        label: "DB 설정 열기",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return dbProfileControls.canIndexProfile
      ? { label: "DB 읽기", run: dbProfileControls.indexProfile, primary: true, disabled: dbProfileControls.busy }
      : { label: "DB 정보 입력", run: showDbSetup, disabled: dbProfileControls.busy };
  }

  const tables = dbProfileControls.inventory.tables;
  if (tables.length > 0 && tables.some((table) => table.columns.length === 0)) {
    if (dbProfileControls.dbIndexBlockedReason) {
      return {
        label: "DB 설정 열기",
        run: showDbSetup,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    if (!dbProfileControls.activeProfile && dbProfileControls.canSaveProfile) {
      return {
        label: "DB 연결 저장",
        run: dbProfileControls.saveProfile,
        primary: true,
        disabled: dbProfileControls.busy,
      };
    }
    return dbProfileControls.canIndexProfile
      ? { label: "컬럼 보강", run: dbProfileControls.indexProfile, primary: true, disabled: dbProfileControls.busy }
      : { label: "DB 정보 입력", run: showDbSetup, primary: true, disabled: dbProfileControls.busy };
  }

  return hasSearchableInventory(workspaceControls, dbProfileControls)
    ? { label: "검색으로 대상 찾기", run: () => focusGlobalSearch(visualMapControls) }
    : null;
}

function focusGlobalSearch(visualMapControls: VisualMapControls) {
  visualMapControls.openSearchPopover();
  window.requestAnimationFrame(() => {
    const target = document.getElementById("global-inventory-search") as HTMLInputElement | null;
    target?.focus();
    target?.select();
  });
}

function firstVisibleNodeActionLabel(node: VisualNode): string {
  return `첫 ${nodeKindLabel(node.kind, node.source)} 보기`;
}

function hasSearchableInventory(workspaceControls: WorkspaceControls, dbProfileControls: DbProfileControls): boolean {
  return (
    codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length)
  );
}
