import {
  ArrowRight,
  Braces,
  CheckCircle2,
  ChevronRight,
  CircleDashed,
  Code2,
  Database,
  FileSearch,
  GitBranch,
  LoaderCircle,
  Search,
  ShieldAlert,
  Table2,
  TriangleAlert,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type {
  ChangeIntent,
  ChangeIntentKind,
  ImpactReviewItem,
  VisualEdge,
  VisualMap,
} from "../../types/visual-map";
import {
  codeInventoryCodeItems,
  codeInventoryItemCount,
  dbInventoryTableCount,
  routeDisplayName,
  routeMethodFromIdentity,
} from "../../types/workspace";
import { visualEdgeKindLabel } from "../../visual/labels";
import { columnRefFromNodeId, dbTableIdentityLabel, tableKeyFromDbNodeId } from "../../visual/nodeIds";
import { buildTargetCatalog, type TargetItem } from "./targetModel";

const ANSWER_MODES = new Set(["api-flow", "search-focus", "table-usage", "column-impact"]);

export function AnswerCanvas({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onOpenSources,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onOpenSources: () => void;
}) {
  const visibleMode = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.mode
    : visualMapControls.mode;
  const committedFocus = answerFocusId(visualMapControls);
  const hasTarget = ANSWER_MODES.has(visibleMode) && Boolean(committedFocus);

  if (visualMapControls.loading && !visualMapControls.currentMap && visualMapControls.focusId) {
    return <AnswerLoading mode={visualMapControls.mode} />;
  }

  if (!hasTarget) {
    return (
      <AnswerHome
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
        onOpenSources={onOpenSources}
      />
    );
  }

  const map = visualMapControls.currentMap;
  return (
    <main className={`answer-canvas${visualMapControls.loading ? " is-refreshing" : ""}`} aria-busy={visualMapControls.loading}>
      {visualMapControls.loading ? (
        <div className="answer-refreshing" role="status" aria-live="polite">
          <LoaderCircle className="spin" size={14} />
          새 대상을 읽는 중 · 이전 답 유지
        </div>
      ) : null}
      <TrustNotice
        workspaceControls={workspaceControls}
        visualMapControls={visualMapControls}
        onOpenSources={onOpenSources}
      />
      {visibleMode === "api-flow" && map?.apiReading ? (
        <ApiAnswer map={map} visualMapControls={visualMapControls} />
      ) : (visibleMode === "table-usage" || visibleMode === "column-impact") && map?.reviewBoard ? (
        <ImpactAnswer map={map} visualMapControls={visualMapControls} />
      ) : (
        <CodeAnswer
          focusId={committedFocus!}
          map={map}
          workspaceControls={workspaceControls}
          visualMapControls={visualMapControls}
        />
      )}
    </main>
  );
}

function AnswerHome({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onOpenSources,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onOpenSources: () => void;
}) {
  const catalog = useMemo(
    () => buildTargetCatalog(workspaceControls.codeInventory, dbProfileControls.inventory),
    [workspaceControls.codeInventory, dbProfileControls.inventory],
  );
  const suggestions = balancedSuggestions(catalog);
  const hasInventory = codeInventoryItemCount(workspaceControls.codeInventory) > 0 || dbInventoryTableCount(dbProfileControls.inventory) > 0;

  return (
    <main className="answer-canvas answer-home">
      <TrustNotice
        workspaceControls={workspaceControls}
        visualMapControls={visualMapControls}
        onOpenSources={onOpenSources}
      />
      <section className="answer-home-intro">
        <span>프로젝트</span>
        <h1>{workspaceControls.currentWorkspace?.name ?? "분석 대상"}</h1>
        <p>API, 함수, 테이블 또는 컬럼을 선택하면 지금 필요한 흐름과 영향만 정리합니다.</p>
        <button
          className="answer-search-action"
          type="button"
          disabled={!hasInventory}
          onClick={() => focusGlobalSearch(visualMapControls)}
        >
          <Search size={18} />
          <span>
            <strong>{hasInventory ? "대상 검색" : "먼저 코드 또는 DB를 읽어 주세요"}</strong>
            <small>{hasInventory ? "API · 함수 · 파일 · 테이블 · 컬럼" : "소스를 연결하면 검색할 수 있습니다"}</small>
          </span>
          {hasInventory ? <kbd>Ctrl K</kbd> : null}
        </button>
      </section>

      {suggestions.length > 0 ? (
        <section className="answer-home-section" aria-labelledby="answer-suggestions-title">
          <header>
            <span>
              <strong id="answer-suggestions-title">바로 보기</strong>
              <small>프로젝트에서 먼저 확인하기 좋은 대상</small>
            </span>
          </header>
          <div className="answer-suggestions">
            {suggestions.map((item) => (
              <button
                type="button"
                onClick={() => visualMapControls.showMode(item.mode, item.focusId)}
                key={item.id}
              >
                <TargetIcon kind={item.kind} />
                <span>
                  <small>{targetAnswerLabel(item)}</small>
                  <strong>{item.title}</strong>
                  <em>{item.meta}</em>
                </span>
                <ChevronRight size={16} />
              </button>
            ))}
          </div>
        </section>
      ) : (
        <section className="answer-empty-source">
          <FileSearch size={22} />
          <span>
            <strong>읽은 프로젝트 정보가 없습니다</strong>
            <small>코드 소스를 먼저 읽고, 필요하면 DB를 연결하세요.</small>
          </span>
          <button type="button" onClick={onOpenSources}>소스 연결</button>
        </section>
      )}
    </main>
  );
}

function ApiAnswer({ map, visualMapControls }: { map: VisualMap; visualMapControls: VisualMapControls }) {
  const answer = map.apiReading!;
  const method = answer.method ?? routeMethodFromIdentity(map.focus);
  const subject = routeDisplayName(answer.subject, method);
  const confirmedSteps = answer.steps.filter((step) => step.truthClass === "confirmed" || step.truthClass === "structural");
  const candidateCount = answer.dbCandidates.length;
  const unknownCount = answer.unknowns.length;
  const visibleSteps = confirmedSteps.slice(0, 5);
  const hiddenSteps = confirmedSteps.slice(5);
  const maxDepth = confirmedSteps.reduce((depth, step) => Math.max(depth, step.depth), 0);
  const directItems = [...confirmedSteps, ...(answer.dbRelations ?? [])];
  const confirmedCount = directItems.filter((item) => item.truthClass === "confirmed").length;
  const structuralCount = directItems.filter((item) => item.truthClass === "structural").length;
  const conclusion = confirmedSteps.length > 1
    ? `${confirmedSteps.length}개 코드 항목을 호출 깊이 ${maxDepth}까지 확인했습니다${answer.dbRelations?.length ? ` · DB 직접 연결 ${answer.dbRelations.length}개` : ""}.`
    : "라우트는 확인했지만 다음 확정 호출 경로는 찾지 못했습니다.";

  return (
    <>
      <AnswerHeader
        icon={<Braces size={18} />}
        kicker="API 처리 흐름"
        title={subject}
        conclusion={conclusion}
        confirmed={confirmedCount}
        structural={structuralCount}
        candidates={candidateCount}
        unknowns={unknownCount}
      />

      <AnswerSection title="확인된 처리 흐름" count={confirmedSteps.length} description="호출 깊이와 역할로 정렬한 코드 근거">
        {visibleSteps.length > 0 ? (
          <ol className="answer-flow">
            {visibleSteps.map((step, index) => (
              <li key={step.id}>
                <button
                  type="button"
                  disabled={!step.nodeId}
                  style={{ "--answer-depth": Math.min(step.depth, 4) } as CSSProperties}
                  onClick={() => selectReviewNode(step.nodeId, map, visualMapControls)}
                >
                  <span className="answer-flow-index">{String(index + 1).padStart(2, "0")}</span>
                  <span className="answer-flow-copy">
                    <small>{apiLaneLabel(step.lane)} · 깊이 {step.depth}</small>
                    <strong>{step.lane === "route" ? routeDisplayName(step.title, method) : step.title}</strong>
                    <em>{step.detail}</em>
                  </span>
                  <TruthMark truthClass={step.truthClass} />
                </button>
              </li>
            ))}
          </ol>
        ) : (
          <AnswerEmpty title="확인된 다음 호출이 없습니다" detail="라우트 소스에서 실제 핸들러 연결을 먼저 확인하세요." />
        )}
        {hiddenSteps.length > 0 ? (
          <details className="answer-more">
            <summary>나머지 경로 {hiddenSteps.length}개 보기</summary>
            <ReviewItems items={hiddenSteps} map={map} visualMapControls={visualMapControls} />
          </details>
        ) : null}
      </AnswerSection>

      {(answer.dbRelations?.length ?? 0) > 0 ? (
        <AnswerSection title="확인된 데이터 사용" count={answer.dbRelations!.length} description="정적 SQL 또는 구조 근거로 연결된 DB 대상">
          <ReviewItems items={answer.dbRelations!} map={map} visualMapControls={visualMapControls} />
        </AnswerSection>
      ) : null}

      <CandidateDetails
        title={reviewDetailsTitle(candidateCount, unknownCount)}
        items={[...answer.dbCandidates, ...answer.unknowns]}
        map={map}
        visualMapControls={visualMapControls}
      />
      <NextChecks items={answer.recommendedChecks} map={map} visualMapControls={visualMapControls} />
      {answer.truncated || answer.hiddenBranches > 0 ? (
        <CoverageNote>
          {answer.truncated
            ? answer.truncationReason ?? "표시 상한 때문에 일부 경로가 접혔습니다."
            : `${answer.hiddenBranchesIsLowerBound ? "최소 " : ""}${answer.hiddenBranches}개 분기 관계를 접었습니다.`}
        </CoverageNote>
      ) : null}
    </>
  );
}

function ImpactAnswer({ map, visualMapControls }: { map: VisualMap; visualMapControls: VisualMapControls }) {
  const board = map.reviewBoard!;
  const tableUsage = board.scope === "table";
  const direct = board.lanes.find((lane) => lane.id === "direct") ?? board.lanes[0];
  const candidates = board.lanes.find((lane) => lane.id === "candidates");
  const unknowns = board.lanes.find((lane) => lane.id === "unknowns");
  const checks = board.lanes.find((lane) => lane.id === "checks");
  const candidateItems = [...(candidates?.items ?? []), ...(unknowns?.items ?? [])];
  const hiddenCandidateCount = (candidates?.hidden ?? 0) + (unknowns?.hidden ?? 0);
  const candidateCount = candidates?.total ?? 0;
  const unknownCount = unknowns?.total ?? 0;
  const confirmedCount = direct?.total ?? 0;
  const hiddenDirectCount = direct?.hidden ?? 0;
  const subject = impactSubject(map, board.subject);
  const codeUsageItems = tableUsage ? (direct?.items ?? []).filter(isCodeUsageReviewItem) : [];
  const structuralItems = tableUsage ? (direct?.items ?? []).filter((item) => !isCodeUsageReviewItem(item)) : [];
  const conclusion = tableUsage
    ? codeUsageItems.length > 0
      ? `표시된 확정 코드 사용 ${codeUsageItems.length}개와 DB 구조 근거 ${structuralItems.length}개를 찾았습니다.`
      : `표시된 확정 코드 사용은 없으며, DB 구조 근거 ${structuralItems.length}개를 확인했습니다.`
    : confirmedCount > 0
      ? `${confirmedCount}개의 직접 영향을 찾았습니다.`
      : "확인된 직접 영향은 없습니다.";

  return (
    <>
      <AnswerHeader
        icon={tableUsage ? <Table2 size={18} /> : <Database size={18} />}
        kicker={tableUsage ? "테이블 연결과 사용 위치" : "컬럼 변경 영향"}
        title={subject}
        conclusion={conclusion}
        confirmed={confirmedCount}
        confirmedLabel="직접 근거"
        candidates={candidateCount}
        unknowns={unknownCount}
      />
      {!tableUsage ? (
        <ChangeIntentBar intent={board.changeIntent ?? visualMapControls.changeIntent} onChange={visualMapControls.setChangeIntent} />
      ) : null}
      {tableUsage ? (
        <>
          <AnswerSection title="확인된 코드 사용" count={codeUsageItems.length} description="정적 SQL에서 확인한 조회·변경·컬럼 사용">
            {codeUsageItems.length > 0 ? (
              <ReviewItems items={codeUsageItems.slice(0, 5)} map={map} visualMapControls={visualMapControls} />
            ) : (
                <AnswerEmpty title="확정된 코드 사용이 없습니다" detail="아래 후보는 직접 근거가 아니므로 소스에서 확인해야 합니다." />
            )}
            {codeUsageItems.length > 5 ? (
              <details className="answer-more">
                <summary>나머지 코드 사용 {codeUsageItems.length - 5}개 보기</summary>
                <ReviewItems items={codeUsageItems.slice(5)} map={map} visualMapControls={visualMapControls} />
              </details>
            ) : null}
          </AnswerSection>
          {structuralItems.length > 0 ? (
            <AnswerSection title="DB 구조 근거" count={structuralItems.length} description="PK·FK·인덱스·뷰처럼 DB에서 직접 읽은 사실">
              <ReviewItems items={structuralItems.slice(0, 5)} map={map} visualMapControls={visualMapControls} />
              {structuralItems.length > 5 ? (
                <details className="answer-more">
                  <summary>나머지 구조 근거 {structuralItems.length - 5}개 보기</summary>
                  <ReviewItems items={structuralItems.slice(5)} map={map} visualMapControls={visualMapControls} />
                </details>
              ) : null}
            </AnswerSection>
          ) : null}
        </>
      ) : (
        <AnswerSection
          title={direct?.title ?? "직접 영향"}
          count={confirmedCount}
          description={direct?.description ?? "확정된 구조와 코드 근거"}
        >
          {direct?.items.length ? (
            <ReviewItems items={direct.items.slice(0, 5)} map={map} visualMapControls={visualMapControls} />
          ) : (
            <AnswerEmpty title={direct?.emptyMessage ?? "직접 관계가 없습니다"} detail="후보가 있더라도 확정 사실과는 분리해 확인하세요." />
          )}
          {(direct?.items.length ?? 0) > 5 ? (
            <details className="answer-more">
              <summary>나머지 직접 항목 {(direct?.items.length ?? 0) - 5}개 보기</summary>
              <ReviewItems items={direct!.items.slice(5)} map={map} visualMapControls={visualMapControls} />
            </details>
          ) : null}
        </AnswerSection>
      )}
      <CandidateDetails
        title={reviewDetailsTitle(candidateCount, unknownCount)}
        items={candidateItems}
        hidden={hiddenCandidateCount}
        map={map}
        visualMapControls={visualMapControls}
      />
      <NextChecks items={checks?.items ?? []} map={map} visualMapControls={visualMapControls} />
      {hiddenDirectCount > 0 ? (
        <CoverageNote>직접 근거 {hiddenDirectCount}개는 엔진 표시 상한 때문에 이 답에서 접혔습니다.</CoverageNote>
      ) : null}
    </>
  );
}

function CodeAnswer({
  focusId,
  map,
  workspaceControls,
  visualMapControls,
}: {
  focusId: string;
  map: VisualMap | null;
  workspaceControls: WorkspaceControls;
  visualMapControls: VisualMapControls;
}) {
  const node = map?.nodes.find((item) => item.id === focusId) ?? null;
  const itemId = focusId.replace(/^code:/, "");
  const inventoryItem = [
    ...(workspaceControls.codeInventory?.routes ?? []),
    ...codeInventoryCodeItems(workspaceControls.codeInventory),
    ...(workspaceControls.codeInventory?.files ?? []),
  ].find((item) => item.id === itemId) ?? null;
  const title = node?.title ?? inventoryItem?.name ?? itemId;
  const edges = (map?.edges ?? []).filter((edge) => edge.from === focusId || edge.to === focusId);
  const confirmed = edges.filter((edge) => !isCandidateEdge(edge));
  const candidates = edges.filter(isCandidateEdge);
  const conclusion = confirmed.length > 0
    ? `${confirmed.length}개의 직접 연결을 찾았습니다.`
    : "확인된 직접 연결은 없습니다.";

  return (
    <>
      <AnswerHeader
        icon={<Code2 size={18} />}
        kicker="코드 호출 경로"
        title={title}
        conclusion={conclusion}
        confirmed={confirmed.length}
        confirmedLabel="직접"
        candidates={candidates.length}
      />
      <AnswerSection title="직접 연결" count={confirmed.length} description="호출과 데이터 사용을 확정 근거 우선으로 정렬">
        {confirmed.length > 0 ? (
          <EdgeItems edges={confirmed.slice(0, 5)} focusId={focusId} map={map} visualMapControls={visualMapControls} />
        ) : (
          <AnswerEmpty
            title="확인된 직접 관계가 없습니다"
            detail="관계가 없다는 사실과 분석하지 못했다는 상태를 구분해 오른쪽 근거에서 확인할 수 있습니다."
          />
        )}
        {confirmed.length > 5 ? (
          <details className="answer-more">
            <summary>나머지 직접 연결 {confirmed.length - 5}개 보기</summary>
            <EdgeItems edges={confirmed.slice(5)} focusId={focusId} map={map} visualMapControls={visualMapControls} />
          </details>
        ) : null}
      </AnswerSection>
      {candidates.length > 0 ? (
        <details className="answer-candidates">
          <summary>
            <span><TriangleAlert size={16} /><strong>확인할 후보</strong><small>직접 근거가 아닌 연결</small></span>
            <em>{candidates.length}</em>
          </summary>
          <EdgeItems edges={candidates} focusId={focusId} map={map} visualMapControls={visualMapControls} />
        </details>
      ) : null}
      {(map?.warnings.length ?? 0) > 0 ? <CoverageNote>{map!.warnings[0]}</CoverageNote> : null}
    </>
  );
}

function AnswerHeader({
  icon,
  kicker,
  title,
  conclusion,
  confirmed,
  confirmedLabel = "확정",
  structural = 0,
  candidates,
  unknowns = 0,
}: {
  icon: ReactNode;
  kicker: string;
  title: string;
  conclusion: string;
  confirmed: number;
  confirmedLabel?: string;
  structural?: number;
  candidates: number;
  unknowns?: number;
}) {
  return (
    <header className="answer-header">
      <div className="answer-header-icon" aria-hidden="true">{icon}</div>
      <div>
        <span>{kicker}</span>
        <h1>{title}</h1>
        <p>{conclusion}</p>
      </div>
      <div className="answer-verdicts" aria-label="근거 요약">
        <span className="confirmed"><CheckCircle2 size={14} />{confirmedLabel} {confirmed}</span>
        {structural > 0 ? <span className="structural"><CircleDashed size={14} />구조 {structural}</span> : null}
        {candidates > 0 ? <span className="candidate"><TriangleAlert size={14} />후보 {candidates}</span> : null}
        {unknowns > 0 ? <span className="unknown"><ShieldAlert size={14} />확인 필요 {unknowns}</span> : null}
      </div>
    </header>
  );
}

function AnswerSection({
  title,
  description,
  count,
  children,
}: {
  title: string;
  description: string;
  count: number;
  children: ReactNode;
}) {
  return (
    <section className="answer-section">
      <header>
        <span><strong>{title}</strong><small>{description}</small></span>
        <em>{count}</em>
      </header>
      {children}
    </section>
  );
}

function ReviewItems({
  items,
  map,
  visualMapControls,
}: {
  items: ImpactReviewItem[];
  map: VisualMap;
  visualMapControls: VisualMapControls;
}) {
  return (
    <div className="answer-review-items">
      {items.map((item) => (
        <button
          type="button"
          disabled={!item.nodeId}
          onClick={() => selectReviewNode(item.nodeId, map, visualMapControls)}
          key={item.id}
        >
          <TruthMark truthClass={item.truthClass} />
          <span>
            <strong>{item.title}</strong>
            <small>{item.detail}</small>
            {item.location ? <code>{sourceLabel(item.location.path, item.location.line)}</code> : null}
          </span>
          {item.nodeId ? <ChevronRight size={15} /> : null}
        </button>
      ))}
    </div>
  );
}

function EdgeItems({
  edges,
  focusId,
  map,
  visualMapControls,
}: {
  edges: VisualEdge[];
  focusId: string;
  map: VisualMap | null;
  visualMapControls: VisualMapControls;
}) {
  return (
    <div className="answer-edge-items">
      {edges.map((edge) => {
        const outbound = edge.from === focusId;
        const otherId = outbound ? edge.to : edge.from;
        const other = map?.nodes.find((node) => node.id === otherId) ?? null;
        return (
          <button type="button" onClick={() => visualMapControls.selectEdge(edge)} key={edge.id}>
            <span className={`answer-edge-direction${outbound ? "" : " inbound"}`}><ArrowRight size={14} /></span>
            <span>
              <small>{visualEdgeKindLabel(edge)}</small>
              <strong>{other?.title ?? otherId}</strong>
              <em>{edge.evidence[0]?.text ?? (isCandidateEdge(edge) ? "직접 근거 없음" : "구조 관계")}</em>
            </span>
            <TruthMark truthClass={isCandidateEdge(edge) ? "candidate" : edge.evidence.length ? "confirmed" : "structural"} />
          </button>
        );
      })}
    </div>
  );
}

function CandidateDetails({
  title,
  items,
  hidden = 0,
  map,
  visualMapControls,
}: {
  title: string;
  items: ImpactReviewItem[];
  hidden?: number;
  map: VisualMap;
  visualMapControls: VisualMapControls;
}) {
  if (items.length === 0 && hidden === 0) return null;
  return (
    <details className="answer-candidates">
      <summary>
        <span><TriangleAlert size={16} /><strong>{title}</strong><small>확정 사실과 분리해서 검토</small></span>
        <em>{items.length + hidden}</em>
      </summary>
      <ReviewItems items={items} map={map} visualMapControls={visualMapControls} />
      {hidden > 0 ? <CoverageNote>확인 항목 {hidden}개는 엔진 표시 상한 때문에 접혔습니다.</CoverageNote> : null}
    </details>
  );
}

function reviewDetailsTitle(candidateCount: number, unknownCount: number): string {
  if (candidateCount > 0 && unknownCount > 0) return "확인할 후보와 빈 구간";
  if (candidateCount > 0) return "확인할 후보";
  return "확인되지 않은 구간";
}

function NextChecks({
  items,
  map,
  visualMapControls,
}: {
  items: ImpactReviewItem[];
  map: VisualMap;
  visualMapControls: VisualMapControls;
}) {
  if (items.length === 0) return null;
  return (
    <section className="answer-next-checks">
      <header><GitBranch size={16} /><strong>다음 확인</strong></header>
      <ReviewItems items={items.slice(0, 3)} map={map} visualMapControls={visualMapControls} />
    </section>
  );
}

function ChangeIntentBar({ intent, onChange }: { intent: ChangeIntent; onChange: (intent: ChangeIntent) => void }) {
  const [draft, setDraft] = useState(intent.value ?? "");
  const needsValue = intent.kind === "rename" || intent.kind === "type";

  useEffect(() => setDraft(intent.value ?? ""), [intent.kind, intent.value]);

  return (
    <section className="answer-change-intent" aria-label="변경 시나리오">
      <label>
        <span>변경 시나리오</span>
        <select
          value={intent.kind}
          onChange={(event) => {
            const kind = event.currentTarget.value as ChangeIntentKind;
            onChange({ kind, value: kind === "nullability" ? "required" : null });
          }}
        >
          <option value="rename">이름 변경</option>
          <option value="drop">컬럼 삭제</option>
          <option value="type">타입 변경</option>
          <option value="nullability">NULL 제약</option>
        </select>
      </label>
      {needsValue ? (
        <form onSubmit={(event) => { event.preventDefault(); onChange({ kind: intent.kind, value: draft.trim() || null }); }}>
          <input
            value={draft}
            maxLength={128}
            aria-label={intent.kind === "rename" ? "새 컬럼명" : "목표 타입"}
            placeholder={intent.kind === "rename" ? "새 컬럼명" : "목표 타입"}
            onChange={(event) => setDraft(event.currentTarget.value)}
          />
          <button type="submit" disabled={(intent.value ?? "") === draft.trim()}>적용</button>
        </form>
      ) : intent.kind === "nullability" ? (
        <div role="group" aria-label="NULL 제약 방향">
          <button className={intent.value === "required" ? "active" : ""} type="button" onClick={() => onChange({ kind: "nullability", value: "required" })}>NOT NULL</button>
          <button className={intent.value === "nullable" ? "active" : ""} type="button" onClick={() => onChange({ kind: "nullability", value: "nullable" })}>NULL 허용</button>
        </div>
      ) : (
        <p>삭제 전에 코드 참조, 제약, 데이터 보존 순서로 확인합니다.</p>
      )}
    </section>
  );
}

function TrustNotice({
  workspaceControls,
  visualMapControls,
  onOpenSources,
}: {
  workspaceControls: WorkspaceControls;
  visualMapControls: VisualMapControls;
  onOpenSources: () => void;
}) {
  if (workspaceControls.operationStatus.phase === "error") {
    return (
      <div className="answer-trust-notice error" role="alert">
        <ShieldAlert size={16} />
        <span><strong>{workspaceControls.operationStatus.label}</strong><small>{workspaceControls.operationStatus.message}</small></span>
        <button type="button" onClick={onOpenSources}>확인</button>
      </div>
    );
  }
  if (visualMapControls.snapshotStaleReasons.length > 0) {
    return (
      <div className="answer-trust-notice stale" role="status">
        <TriangleAlert size={16} />
        <span><strong>마지막 분석 결과가 오래되었습니다</strong><small>{visualMapControls.snapshotStaleReasons.join(" · ")}</small></span>
        <button type="button" onClick={onOpenSources}>다시 읽기</button>
      </div>
    );
  }
  return null;
}

function TruthMark({ truthClass }: { truthClass: string }) {
  const tone = truthClass === "confirmed"
    ? "confirmed"
    : truthClass === "structural"
      ? "structural"
      : truthClass === "candidate"
        ? "candidate"
        : "unknown";
  return (
    <span className={`answer-truth ${tone}`}>
      {truthClass === "confirmed"
        ? <CheckCircle2 size={13} />
        : truthClass === "candidate"
          ? <TriangleAlert size={13} />
          : <CircleDashed size={13} />}
      {truthClass === "confirmed" ? "확정" : truthClass === "structural" ? "구조" : truthClass === "candidate" ? "후보" : "확인"}
    </span>
  );
}

function AnswerEmpty({ title, detail }: { title: string; detail: string }) {
  return <div className="answer-empty"><CircleDashed size={18} /><span><strong>{title}</strong><small>{detail}</small></span></div>;
}

function CoverageNote({ children }: { children: ReactNode }) {
  return <div className="answer-coverage"><TriangleAlert size={15} /><span>{children}</span></div>;
}

function AnswerLoading({ mode }: { mode: string }) {
  return (
    <main className="answer-canvas answer-loading" aria-busy="true">
      <div className="answer-loading-header">
        <LoaderCircle className="spin" size={18} />
        <span><strong>{answerModeLabel(mode)} 준비 중</strong><small>선택한 대상과 일치하는 근거를 읽고 있습니다.</small></span>
      </div>
      <section aria-hidden="true"><i /><i /><i /></section>
    </main>
  );
}

function answerFocusId(controls: VisualMapControls): string | null {
  if (controls.loading && controls.currentMap) {
    return validAnswerFocus(controls.currentMap.focus) ? controls.currentMap.focus : null;
  }
  if (validAnswerFocus(controls.focusId)) return controls.focusId;
  return validAnswerFocus(controls.currentMap?.focus) ? controls.currentMap!.focus : null;
}

function validAnswerFocus(value: string | null | undefined): value is string {
  return Boolean(value && value !== "narrow-focus" && value !== "overview" && !value.startsWith("group:"));
}

function balancedSuggestions(catalog: ReturnType<typeof buildTargetCatalog>): TargetItem[] {
  const groups = [catalog.api, catalog.table, catalog.code, catalog.column];
  const result: TargetItem[] = [];
  for (let round = 0; result.length < 4; round += 1) {
    let added = false;
    for (const group of groups) {
      if (group[round] && result.length < 4) {
        result.push(group[round]);
        added = true;
      }
    }
    if (!added) break;
  }
  return result;
}

function TargetIcon({ kind }: { kind: TargetItem["kind"] }) {
  if (kind === "api") return <Braces size={17} />;
  if (kind === "table") return <Database size={17} />;
  if (kind === "column") return <Table2 size={17} />;
  return <Code2 size={17} />;
}

function targetAnswerLabel(item: TargetItem): string {
  if (item.kind === "api") return "처리 흐름";
  if (item.kind === "table") return "사용 위치";
  if (item.kind === "column") return "변경 영향";
  return "호출 경로";
}

function selectReviewNode(nodeId: string | null | undefined, map: VisualMap, controls: VisualMapControls) {
  if (!nodeId) return;
  const node = map.nodes.find((candidate) => candidate.id === nodeId);
  if (node) controls.selectNode(node);
}

function isCandidateEdge(edge: VisualEdge): boolean {
  return edge.kind.startsWith("candidate") || edge.kind === "code_flow";
}

function isCodeUsageReviewItem(item: ImpactReviewItem): boolean {
  return item.kind === "code_db_read" || item.kind === "code_db_write" || item.kind === "code_db_uses_column";
}

function apiLaneLabel(lane: string): string {
  if (lane === "route") return "Route";
  if (lane === "handler") return "Handler";
  if (lane === "service-function") return "Service / Function";
  if (lane === "repository-query") return "Repository / Query";
  return lane;
}

function sourceLabel(path: string, line: number | null | undefined): string {
  const compact = path.replace(/\\/g, "/").split("/").filter(Boolean).slice(-3).join("/");
  return `${compact}${line ? `:${line}` : ""}`;
}

function answerModeLabel(mode: string): string {
  if (mode === "api-flow") return "API 처리 흐름";
  if (mode === "table-usage") return "테이블 사용 위치";
  if (mode === "column-impact") return "컬럼 변경 영향";
  return "코드 호출 경로";
}

function impactSubject(map: VisualMap, fallback: string): string {
  const column = columnRefFromNodeId(map.focus);
  if (column) return `${dbTableIdentityLabel(column.tableKey)}.${column.columnName}`;
  const tableKey = tableKeyFromDbNodeId(map.focus);
  return tableKey ? dbTableIdentityLabel(tableKey) : fallback;
}

function focusGlobalSearch(controls: VisualMapControls) {
  controls.openSearchPopover();
  window.requestAnimationFrame(() => {
    const input = document.querySelector<HTMLInputElement>("#global-inventory-search");
    input?.focus();
    input?.select();
  });
}
