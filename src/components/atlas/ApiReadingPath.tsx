import type {
  ApiReadingAnswer,
  ApiReadingStep,
  ImpactReviewItem,
  VisualMap,
  VisualNode,
} from "../../types/visual-map";
import { ImpactReviewEntry } from "./ImpactReviewBoard";

export function ApiReadingPath({
  answer,
  map,
  onSelectNode,
}: {
  answer: ApiReadingAnswer;
  map: VisualMap;
  onSelectNode: (node: VisualNode) => void;
}) {
  const lanes: Array<{
    id: string;
    number: string;
    title: string;
    description: string;
    empty: string;
    tone: "structural" | "confirmed" | "candidate";
    items: Array<ApiReadingStep | ImpactReviewItem>;
  }> = [
    { id: "route", number: "01", title: "진입 라우트", description: "선택한 API 진입점", empty: "선택한 라우트를 읽지 못했습니다.", tone: "structural", items: answer.steps.filter((step) => step.lane === "route") },
    { id: "handler", number: "02", title: "처리기", description: "확정 HANDLES 대상", empty: "확정 처리기를 찾지 못했습니다.", tone: "confirmed", items: answer.steps.filter((step) => step.lane === "handler") },
    { id: "service-function", number: "03", title: "서비스 / 함수", description: "CALLS 확정 · 역할은 이름 기반 추정", empty: "확정 CALLS 경로에서 서비스/함수 역할 후보를 찾지 못했습니다.", tone: "confirmed", items: answer.steps.filter((step) => step.lane === "service-function") },
    { id: "repository-query", number: "04", title: "저장소 / 쿼리", description: "CALLS 확정 · 데이터 접근 역할은 추정", empty: "확정 CALLS 경로에서 저장소/쿼리 역할 후보를 찾지 못했습니다.", tone: "confirmed", items: answer.steps.filter((step) => step.lane === "repository-query") },
    { id: "db-candidate", number: "05", title: "DB 후보", description: "확정 경로 뒤의 검증 후보", empty: "찾은 DB 후보가 없습니다. DB 미사용이 확정된 것은 아닙니다.", tone: "candidate", items: answer.dbCandidates },
  ];
  const handlerCount = answer.steps.filter((step) => step.lane === "handler").length;
  const routeMethod = map.focus.match(/__route__([A-Z]+)__/i)?.[1]?.toUpperCase() ?? null;
  const answerState = handlerCount === 0
    ? { label: "근거 부족", tone: "unknown" }
    : answer.truncated || answer.unknowns.length > 0
      ? { label: "부분 답변", tone: "partial" }
      : { label: "답변됨", tone: "answered" };

  return (
    <section className="at-impact-board at-api-reading" aria-label={`${answer.subject} API 읽기 경로`}>
      <header className="at-impact-board-head">
        <div>
          <span>API 실행 경로</span>
          <strong>{routeMethod ? <code className="at-api-method">{routeMethod}</code> : null}{answer.subject}</strong>
          <small>HANDLES/CALLS 관계만 확정 경로로 사용하고 역할 분류와 DB 연결은 후보로 구분했습니다.</small>
        </div>
        <div className="at-board-status">
          <span className={`at-answer-state ${answerState.tone}`}>답 상태 · {answerState.label}</span>
          {answer.truncated ? (
            <em className="at-api-truncated">
              {answer.hiddenBranches > 0
                ? answer.hiddenBranchesIsLowerBound
                  ? `최소 +${answer.hiddenBranches} 경계 관계 · 하위 미탐색`
                  : `+${answer.hiddenBranches}개 접힘`
                : "표시 한도에서 경로가 잘렸습니다"}
            </em>
          ) : null}
        </div>
      </header>
      <div className="at-impact-lanes">
        {lanes.map((lane) => (
          <section className={`at-impact-lane ${lane.tone}`} key={lane.id} aria-labelledby={`api-lane-${lane.id}`}>
            <header>
              <span>{lane.number}</span>
              <div>
                <strong id={`api-lane-${lane.id}`}>{lane.title}</strong>
                <small>{lane.description}</small>
              </div>
              <em>{lane.items.length}</em>
            </header>
            <div className="at-impact-items">
              {lane.items.length === 0 ? <p className="at-impact-empty">{lane.empty}</p> : null}
              {lane.items.map((item) => {
                const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
                const contextBadge = "laneBasis" in item && item.laneBasis === "name-inferred" ? "역할 추정" : null;
                return <ImpactReviewEntry item={item} key={item.id} onSelect={node ? () => onSelectNode(node) : null} contextBadge={contextBadge} />;
              })}
            </div>
          </section>
        ))}
      </div>
      <div className="at-api-followups">
        <ApiReadingFollowup title="확인 안 된 구간" tone="unknown" items={answer.unknowns} map={map} onSelectNode={onSelectNode} empty="현재 표시 범위에서 확인 안 된 구간이 없습니다." />
        <ApiReadingFollowup title="권장 확인" tone="action" items={answer.recommendedChecks} map={map} onSelectNode={onSelectNode} empty="추가 권장 확인이 없습니다." />
      </div>
      {answer.truncationReason ? <p className="at-api-cap-note">표시 한도 · {answer.truncationReason}</p> : null}
    </section>
  );
}

function ApiReadingFollowup({
  title,
  tone,
  items,
  map,
  onSelectNode,
  empty,
}: {
  title: string;
  tone: "unknown" | "action";
  items: ImpactReviewItem[];
  map: VisualMap;
  onSelectNode: (node: VisualNode) => void;
  empty: string;
}) {
  return (
    <section className={`at-impact-lane ${tone}`}>
      <header>
        <span>{tone === "unknown" ? "?" : "✓"}</span>
        <div><strong>{title}</strong><small>{tone === "unknown" ? "근거가 없거나 접힌 영역" : "다음에 열어볼 대상"}</small></div>
        <em>{items.length}</em>
      </header>
      <div className="at-impact-items">
        {items.length === 0 ? <p className="at-impact-empty">{empty}</p> : null}
        {items.map((item) => {
          const node = item.nodeId ? map.nodes.find((candidate) => candidate.id === item.nodeId) ?? null : null;
          return <ImpactReviewEntry item={item} key={item.id} onSelect={node ? () => onSelectNode(node) : null} />;
        })}
      </div>
    </section>
  );
}
