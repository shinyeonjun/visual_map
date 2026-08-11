/**
 * The words and marks the map uses for what the engine reports.
 *
 * Kept apart from the canvases so both views name the same thing the same
 * way. A step called a repository in the overview must not become a store in
 * the trace view.
 */

import {
  CodeRegular as Code2,
  DatabaseRegular as Database,
  FlashRegular as Zap,
  GlobeRegular as Globe,
  LayerRegular as Layers,
  PersonRegular as UserRound,
  TableRegular as Table2,
} from "@fluentui/react-icons";
import type { MapDetail } from "./detail";
import type { UnfinishedTraceState } from "./traceGraph";
import type {
  AreaCategory,
  DispatchKind,
  EvidenceRef,
  MapExecutionOccurrence,
  NodeRole,
  SemanticFallbackReason,
  TraceState,
} from "./types";

export function NodeIcon({ role, size = 15 }: { role: NodeRole; size?: number }) {
  if (role === "endpoint") return <Globe fontSize={size} />;
  if (role === "controller") return <UserRound fontSize={size} />;
  if (role === "service") return <Layers fontSize={size} />;
  if (role === "repository") return <Database fontSize={size} />;
  if (role === "table" || role === "table-reference") return <Table2 fontSize={size} />;
  if (role === "event") return <Zap fontSize={size} />;
  return <Code2 fontSize={size} />;
}

export function roleLabel(role: NodeRole): string {
  if (role === "endpoint") return "진입점";
  if (role === "controller") return "컨트롤러";
  if (role === "service") return "서비스";
  if (role === "repository") return "저장소";
  if (role === "table") return "데이터 경계";
  // A reference is not the table: the code names one it does not itself define.
  if (role === "table-reference") return "데이터 참조";
  if (role === "event") return "이벤트";
  return "구현";
}

/**
 * The approved category, in the reader's words.
 *
 * Shown as text on every area, not as colour alone. The map has only two
 * category hues to spend — every other hue on this canvas already means a
 * truth class or a selection — so the word is the identity and the colour is
 * a scanning aid for the two categories a reader looks for first.
 */
export function categoryLabel(category: AreaCategory): string {
  if (category === "domain") return "도메인";
  if (category === "integration") return "연동";
  if (category === "shared") return "공통";
  if (category === "infrastructure") return "인프라";
  return "구조";
}

/**
 * Why the verifier kept a structural name instead of a semantic one.
 *
 * A name the analysis copied from the code's own structure is a different
 * claim from one it derived from evidence, and the reader is owed the
 * difference — an area called "handlers" because nothing better could be
 * proven should not read like one called "주문 결제".
 */
export function fallbackReasonLabel(reason: SemanticFallbackReason): string {
  if (reason === "insufficient-semantic-signal") return "근거가 부족해 구조 이름을 그대로 씁니다";
  return "책임이 섞여 있어 구조 이름을 그대로 씁니다";
}

/** `orders/order.service.ts:87` → `order.service.ts:87`. */
export function evidenceLabel(evidence: EvidenceRef): string {
  const name = evidence.path.split(/[\\/]/).filter(Boolean).pop() ?? evidence.path;
  return evidence.line ? `${name}:${evidence.line}` : name;
}

/** How the walk ended. Never a confidence score — these are engine outcomes. */
export function traceStateLabel(state: TraceState): string {
  if (state === "complete") return "경로 끝 확인";
  if (state === "partial") return "일부만 확인";
  if (state === "gap") return "분석 공백";
  if (state === "cycle") return "순환 감지";
  return "표시 깊이 제한";
}

/** Why a path stops here, in the words of the thing that stopped it. */
export function terminalReason(state: UnfinishedTraceState): string {
  if (state === "partial") return "이 뒤는 확인하지 못했습니다";
  if (state === "gap") return "분석 공백으로 여기서 멈춥니다";
  if (state === "cycle") return "순환이 감지되어 멈춥니다";
  return "표시 깊이 제한으로 멈춥니다";
}

/**
 * How sure the engine is about which target a call reaches.
 *
 * Returns null for relations that are not calls, so nothing has to invent a
 * word for a question that was never asked.
 */
export function dispatchLabel(dispatch: DispatchKind): string | null {
  if (dispatch === "direct") return "확정 호출";
  if (dispatch === "virtual") return "가상 호출";
  if (dispatch === "interface") return "인터페이스 호출";
  if (dispatch === "dynamic") return "동적 호출";
  if (dispatch === "unknown") return "호출 종류 미분류";
  return null;
}

/** Why that classification limits what the line can claim. */
export function dispatchNote(dispatch: DispatchKind): string | null {
  if (dispatch === "direct") return "호출 지점에서 대상이 확정됩니다.";
  if (dispatch === "virtual") return "실제 대상은 런타임 타입에 따라 달라집니다.";
  if (dispatch === "interface") return "구현체가 런타임에 결정됩니다.";
  if (dispatch === "dynamic") return "정적 분석으로는 대상을 확정할 수 없습니다.";
  if (dispatch === "unknown") return "대상은 찾았지만 호출 종류를 분류하지 못했습니다.";
  return null;
}

/**
 * What the written source says about whether this call runs.
 *
 * Lexical facts only. `조건부` means the call sits inside a branch, not that
 * it was skipped, and nothing here reports an observed execution.
 */
export function controlFlags(execution: MapExecutionOccurrence | null): string[] {
  if (!execution) return [];
  const flags: string[] = [];
  if (execution.guarded) flags.push("조건부");
  if (execution.repeated) flags.push("반복");
  if (execution.awaited) flags.push("await");
  if (execution.deferred) flags.push("지연 실행");
  return flags;
}

/** What the reader is being shown, in the words of the thing they can see. */
export function detailLabel(detail: MapDetail): string {
  if (detail === "full") return "상세 구조";
  if (detail === "outline") return "영역 구성";
  return "전체 구조";
}
