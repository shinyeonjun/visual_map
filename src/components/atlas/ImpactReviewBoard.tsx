import { CheckCircle2, ClipboardCopy } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  ChangeIntent,
  ChangeIntentKind,
  ImpactReviewBoard as ImpactReviewBoardModel,
  ImpactReviewItem,
  VisualMap,
  VisualNode,
} from "../../types/visual-map";
import { copyValue } from "../common/copyValue";

export function ImpactReviewBoard({
  board,
  map,
  onSelectNode,
  changeIntent,
  onChangeIntent,
}: {
  board: ImpactReviewBoardModel;
  map: VisualMap;
  onSelectNode: (node: VisualNode) => void;
  changeIntent: ChangeIntent;
  onChangeIntent: (intent: ChangeIntent) => void;
}) {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const tableUsage = board.scope === "table";

  async function copySummary() {
    const copied = await copyValue(board.markdownSummary);
    setCopyState(copied ? "copied" : "failed");
    window.setTimeout(() => setCopyState("idle"), 1800);
  }

  return (
    <section className="at-impact-board" aria-label={`${board.subject} ${tableUsage ? "테이블 사용처" : "변경 영향 리뷰"}`}>
      <header className="at-impact-board-head">
        <div>
          <span>{tableUsage ? "테이블 사용처" : "컬럼 변경 검토"}</span>
          <strong>{board.subject}</strong>
          <small>
            {tableUsage
              ? "확정된 스키마 사실과 코드 후보를 구분해 정리했습니다."
              : "직접 사실과 후보를 섞지 않고 수정 전 확인 순서로 정리했습니다."}
          </small>
        </div>
        <button type="button" onClick={() => void copySummary()} aria-label={`${tableUsage ? "테이블 사용처" : "변경 영향"} Markdown 요약 복사`}>
          {copyState === "copied" ? <CheckCircle2 size={14} /> : <ClipboardCopy size={14} />}
          {copyState === "copied" ? "복사됨" : copyState === "failed" ? "복사 실패" : "Markdown 복사"}
        </button>
      </header>

      {!tableUsage ? (
        <ChangeIntentControls intent={board.changeIntent ?? changeIntent} onChange={onChangeIntent} />
      ) : null}

      <div className="at-impact-lanes">
        {board.lanes.map((lane) => (
          <section className={`at-impact-lane ${lane.tone}`} key={lane.id} aria-labelledby={`impact-lane-${lane.id}`}>
            <header>
              <span>{String(lane.order).padStart(2, "0")}</span>
              <div>
                <strong id={`impact-lane-${lane.id}`}>{lane.title}</strong>
                <small>{lane.description}</small>
              </div>
              <em>{lane.total}</em>
            </header>
            <div className="at-impact-items">
              {lane.items.length === 0 ? <p className="at-impact-empty">{lane.emptyMessage}</p> : null}
              {lane.items.map((item) => {
                const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
                return <ImpactReviewEntry item={item} key={item.id} onSelect={node ? () => onSelectNode(node) : null} />;
              })}
            </div>
            {lane.hidden > 0 ? <footer>+{lane.hidden}개 접힘</footer> : null}
          </section>
        ))}
      </div>
      <span className="sr-only" aria-live="polite">
        {copyState === "copied" ? "Markdown 요약을 복사했습니다." : copyState === "failed" ? "Markdown 요약을 복사하지 못했습니다." : ""}
      </span>
    </section>
  );
}

const CHANGE_INTENT_OPTIONS: Array<{ kind: ChangeIntentKind; label: string }> = [
  { kind: "rename", label: "이름 변경" },
  { kind: "drop", label: "삭제" },
  { kind: "type", label: "타입 변경" },
  { kind: "nullability", label: "NULL 제약" },
];

function ChangeIntentControls({
  intent,
  onChange,
}: {
  intent: ChangeIntent;
  onChange: (intent: ChangeIntent) => void;
}) {
  const [draft, setDraft] = useState(intent.value ?? "");
  const needsText = intent.kind === "rename" || intent.kind === "type";

  useEffect(() => {
    setDraft(intent.value ?? "");
  }, [intent.kind, intent.value]);

  function selectKind(kind: ChangeIntentKind) {
    const value = kind === "nullability" ? "required" : null;
    setDraft("");
    onChange({ kind, value });
  }

  function applyTextValue() {
    if (needsText) {
      onChange({ kind: intent.kind, value: draft.trim() || null });
    }
  }

  return (
    <section className="at-change-intent" aria-label="변경 시나리오">
      <div className="at-change-intent-kinds" role="group" aria-label="변경 종류">
        <strong>무엇을 바꾸나요?</strong>
        {CHANGE_INTENT_OPTIONS.map((option) => (
          <button
            type="button"
            key={option.kind}
            className={intent.kind === option.kind ? "active" : ""}
            aria-pressed={intent.kind === option.kind}
            onClick={() => selectKind(option.kind)}
          >
            {option.label}
          </button>
        ))}
      </div>
      {needsText ? (
        <form
          className="at-change-intent-value"
          onSubmit={(event) => {
            event.preventDefault();
            applyTextValue();
          }}
        >
          <label htmlFor="change-intent-value">{intent.kind === "rename" ? "새 컬럼명" : "목표 타입"}</label>
          <input
            id="change-intent-value"
            value={draft}
            maxLength={128}
            placeholder={intent.kind === "rename" ? "예: display_name" : "예: varchar(255)"}
            onChange={(event) => setDraft(event.target.value)}
          />
          <button type="submit" disabled={(intent.value ?? "") === draft.trim()}>
            적용
          </button>
        </form>
      ) : null}
      {intent.kind === "nullability" ? (
        <div className="at-change-intent-value" role="group" aria-label="NULL 제약 방향">
          <span>변경 방향</span>
          <button
            type="button"
            className={intent.value === "required" ? "active" : ""}
            aria-pressed={intent.value === "required"}
            onClick={() => onChange({ kind: "nullability", value: "required" })}
          >
            NOT NULL
          </button>
          <button
            type="button"
            className={intent.value === "nullable" ? "active" : ""}
            aria-pressed={intent.value === "nullable"}
            onClick={() => onChange({ kind: "nullability", value: "nullable" })}
          >
            NULL 허용
          </button>
        </div>
      ) : null}
      {intent.kind === "drop" ? (
        <p>컬럼 삭제 기준으로 코드 참조, 제약, 데이터 보존과 롤백 확인 순서를 다시 계산합니다.</p>
      ) : null}
    </section>
  );
}

function ImpactReviewEntry({
  item,
  onSelect,
  contextBadge = null,
}: {
  item: ImpactReviewItem;
  onSelect: (() => void) | null;
  contextBadge?: string | null;
}) {
  const content = (
    <>
      <div className="at-impact-item-head">
        <span>#{item.rank}</span>
        <strong>{item.title}</strong>
      </div>
      <div className="at-impact-item-badges">
        <span className={item.truthClass}>{reviewTruthLabel(item.truthClass)}</span>
        {contextBadge ? <span className="classification">{contextBadge}</span> : null}
        {item.confidence ? <span className="confidence">{confidenceLabel(item.confidence)}</span> : null}
        <code>{item.kind}</code>
      </div>
      <p>{item.detail}</p>
      {item.location ? (
        <small className="at-impact-location" title={item.location.path}>
          {item.location.path}
          {item.location.line ? `:L${item.location.line}` : ""}
        </small>
      ) : null}
      {item.evidence.length > 0 ? (
        <small className="at-impact-evidence" title={item.evidence.map((evidence) => evidence.text).join("\n")}>
          근거 · {impactEvidenceLabel(item.evidence[0])}
          {item.evidence.length > 1 ? ` · +${item.evidence.length - 1}` : ""}
        </small>
      ) : null}
    </>
  );

  return onSelect ? (
    <button className="at-impact-item selectable" type="button" onClick={onSelect}>
      {content}
    </button>
  ) : (
    <article className="at-impact-item">{content}</article>
  );
}

function impactEvidenceLabel(evidence: ImpactReviewItem["evidence"][number]): string {
  if (evidence.kind.startsWith("db-") || evidence.kind === "engine-edge") return "DB 메타데이터";
  if (evidence.kind === "column-name-match") return "컬럼명 일치";
  if (evidence.kind.startsWith("code-search")) return "코드 검색";
  if (evidence.kind === "handles") return "HANDLES 관계";
  if (evidence.kind === "calls") return "CALLS 관계";
  return evidence.text;
}

function reviewTruthLabel(value: string): string {
  if (value === "confirmed") return "확정";
  if (value === "structural") return "구조";
  if (value === "candidate") return "후보";
  if (value === "action") return "행동";
  return "알 수 없음";
}

function confidenceLabel(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (normalized === "high") return "강함";
  if (normalized === "medium") return "보통";
  if (normalized === "low") return "약함";
  return value;
}
