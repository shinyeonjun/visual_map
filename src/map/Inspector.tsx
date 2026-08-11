import {
  ArrowRoutingRegular as FlowRight,
  CheckmarkCircleRegular as CheckmarkCircle,
  CodeRegular as FileCode2,
  FlowRegular as Waypoints,
  GlobeRegular as Globe,
  OpenRegular as Open,
} from "@fluentui/react-icons";
import { dispatchLabel, dispatchNote, evidenceLabel, traceStateLabel } from "./presentation";
import type {
  AnalysisGapItem,
  EvidenceRef,
  MapTrace,
  RelationTally,
  Selection,
  SourceExcerpt,
  TruthClass,
} from "./types";

/**
 * What is true about the one thing the reader picked.
 *
 * Everything on this panel is traceable: a relation count comes from the
 * engine's accounting over the whole graph rather than from the lines drawn
 * on screen, and every claim ends at a file and a line the reader can open.
 */
export function Inspector({
  selection,
  onHighlight,
  onOpenEvidence,
  onOpenTrace,
}: {
  selection: Selection | null;
  onHighlight?: (id: string) => void;
  onOpenEvidence?: (evidence: EvidenceRef) => void;
  /** Leaves for the flow view of whichever area owns this selection. */
  onOpenTrace?: () => void;
}) {
  if (!selection) {
    return (
      <div className="inspector-empty">
        <Waypoints fontSize={22} aria-hidden="true" />
        <p>지도에서 영역이나 항목을 선택하세요.</p>
      </div>
    );
  }

  const verifiedCount = relationCount(selection, "verified");
  const structuralCount = relationCount(selection, "structural");
  const candidateCount = relationCount(selection, "candidate");
  const relationGroups = groupRelationTallies(selection.relations);
  const traces = selection.traces ?? [];
  const entries = entryPoints(traces);
  const firstEvidence = selection.evidence[0];
  const gaps = selection.analysisGaps;

  return (
    <div className="inspector-body">
      <section className="inspector-identity">
        <span className="inspector-identity-icon" aria-hidden="true">
          <Waypoints fontSize={18} />
        </span>
        <div>
          <strong>{selection.title}</strong>
          <span>
            <CheckmarkCircle fontSize={12} /> 선택된 지도 항목
          </span>
        </div>
      </section>

      {onOpenTrace || (onOpenEvidence && firstEvidence) ? (
        <section className="inspector-actions">
          {onOpenTrace ? (
            <button type="button" className="primary" onClick={onOpenTrace} disabled={traces.length === 0}>
              <FlowRight fontSize={15} aria-hidden="true" />
              <span>
                <strong>흐름 보기</strong>
                <small>{traces.length > 0 ? `확인된 경로 ${traces.length}` : "확인된 경로 없음"}</small>
              </span>
            </button>
          ) : null}
          {onOpenEvidence && firstEvidence ? (
            <button type="button" onClick={() => onOpenEvidence(firstEvidence)}>
              <FileCode2 fontSize={15} aria-hidden="true" />
              <span>
                <strong>코드 열기</strong>
                <small>{evidenceLabel(firstEvidence)}</small>
              </span>
            </button>
          ) : null}
        </section>
      ) : null}

      <section className="inspector-section">
        <h3>
          책임 <span>Responsibility</span>
        </h3>
        <p>{selection.role}</p>
      </section>

      {selection.relations.length > 0 ? (
        <section className="inspector-section evidence-overview">
          <h3>
            관계 근거 <span>Evidence</span>
          </h3>
          <div className="evidence-metrics">
            <div className="verified">
              <strong>{verifiedCount.toLocaleString("ko-KR")}</strong>
              <span>확인됨</span>
            </div>
            <div className="structural">
              <strong>{structuralCount.toLocaleString("ko-KR")}</strong>
              <span>구조</span>
            </div>
            <div className="candidate">
              <strong>{candidateCount.toLocaleString("ko-KR")}</strong>
              <span>후보</span>
            </div>
          </div>
          <div className="evidence-meter" aria-hidden="true">
            <i className="verified" style={{ flexGrow: verifiedCount }} />
            <i className="structural" style={{ flexGrow: structuralCount }} />
            <i className="candidate" style={{ flexGrow: candidateCount }} />
          </div>
        </section>
      ) : null}

      {gaps.totalCount > 0 ? (
        <section className="inspector-section gap-section">
          <h3>
            분석 공백 <span>{gaps.totalCount.toLocaleString("ko-KR")}</span>
          </h3>
          {/*
            A count on its own only says "something is missing". The engine
            already knows which capability fell short and why, and that is the
            difference between a number the reader can act on and one they
            can only worry about.
          */}
          <div className="gap-list">
            {groupGaps(gaps.items).map((gap) => (
              <div className="gap-row" key={gap.key}>
                <code>{gap.code}</code>
                {gap.occurrences > 1 ? <b>×{gap.occurrences.toLocaleString("ko-KR")}</b> : null}
                <p>{gap.message}</p>
                {gap.capability ? <small>{gap.capability}</small> : null}
              </div>
            ))}
          </div>
          {gaps.truncatedCount > 0 ? (
            <p className="gap-more">+{gaps.truncatedCount.toLocaleString("ko-KR")}건 더 있음</p>
          ) : null}
        </section>
      ) : null}

      {entries.length > 0 ? (
        <section className="inspector-section">
          <h3>
            진입 API <span>{entries.length}</span>
          </h3>
          <div className="entry-list">
            {entries.map((entry) => (
              <div className="entry-row" key={entry.id}>
                <Globe fontSize={13} aria-hidden="true" />
                <span>
                  <strong>{entry.name}</strong>
                  {entry.handler ? <small>{entry.handler}</small> : null}
                </span>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {traces.length > 0 ? (
        <section className="inspector-section">
          <h3>
            정적 실행 경로 <span>{traces.length}</span>
          </h3>
          {/*
            The steps themselves belong on the flow view, where they have room
            to be a shape. Here each path is one line: where it starts, where
            it got to, and whether it finished.
          */}
          <div className="trace-list">
            {traces.map((trace) => (
              <TraceRow trace={trace} key={trace.id} onOpen={onOpenTrace} />
            ))}
          </div>
        </section>
      ) : null}

      {selection.relations.length > 0 ? (
        <section className="inspector-section">
          <h3>
            관련 관계 <span>{relationGroups.length}</span>
          </h3>
          <div className="relation-tally">
            {relationGroups.map((relation) => (
              <div className="relation-row" key={`${relation.truth}-${relation.dispatch ?? ""}-${relation.label}`}>
                <i className={`relation-mark ${relation.truth}`} aria-hidden="true" />
                <span className={truthTextClass(relation.truth)}>
                  {relation.label}
                  {/*
                    A resolved target is not always the only target. Saying so
                    beside the count keeps "we know" apart from "we found one".
                  */}
                  {relation.dispatch && dispatchLabel(relation.dispatch) ? (
                    <em title={dispatchNote(relation.dispatch) ?? undefined}>{dispatchLabel(relation.dispatch)}</em>
                  ) : null}
                </span>
                <strong className={truthTextClass(relation.truth)}>{relation.count.toLocaleString("ko-KR")}</strong>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {selection.evidence.length > 0 ? (
        <section className="inspector-section">
          <h3>
            소스 근거 <span>{selection.evidence.length}</span>
          </h3>
          <div className="evidence-list">
            {selection.evidence.map((item) => (
              <button
                type="button"
                className={evidenceClass(item, selection.source)}
                key={`${item.path}:${item.line ?? ""}`}
                onClick={() => onOpenEvidence?.(item)}
                aria-label={evidenceLabel(item)}
                title={onOpenEvidence ? "VS Code에서 근거 열기" : undefined}
              >
                <FileCode2 fontSize={13} aria-hidden="true" />
                <span>
                  <strong>{evidenceLabel(item)}</strong>
                  <small>{item.path}</small>
                </span>
                <Open fontSize={12} aria-hidden="true" />
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {selection.source ? <SourceView source={selection.source} /> : null}

      {onHighlight ? (
        <button type="button" className="inspector-link" onClick={() => onHighlight(selection.id)}>
          <Waypoints fontSize={14} aria-hidden="true" />
          선택 경로 다시 보기
        </button>
      ) : null}
    </div>
  );
}

function TraceRow({ trace, onOpen }: { trace: MapTrace; onOpen?: () => void }) {
  const first = trace.steps[0];
  const last = trace.steps[trace.steps.length - 1];
  const hops = Math.max(0, trace.steps.length - 1);
  const body = (
    <>
      <span className={`trace-state ${trace.state}`}>{traceStateLabel(trace.state)}</span>
      <span className="trace-row-path">
        <strong>{first?.name ?? "시작 미확인"}</strong>
        {last && last !== first ? <small>→ {last.name}</small> : null}
      </span>
      <em>{hops.toLocaleString("ko-KR")}홉</em>
    </>
  );
  if (!onOpen) return <div className="trace-row">{body}</div>;
  return (
    <button type="button" className="trace-row" onClick={onOpen}>
      {body}
    </button>
  );
}

/**
 * Identical gap records folded into one row that says how many.
 *
 * The engine reports a record per occurrence, and the same shortfall in the
 * same capability legitimately repeats across units. Four identical rows in a
 * narrow panel spend four lines to say one thing, and the summary's own
 * `totalCount` already carries the real number, so nothing is hidden by
 * showing the shortfall once with a multiplier.
 */
function groupGaps(items: AnalysisGapItem[]): Array<AnalysisGapItem & { key: string; occurrences: number }> {
  const grouped = new Map<string, AnalysisGapItem & { key: string; occurrences: number }>();
  for (const item of items) {
    const key = [item.code, item.capability ?? "", item.message].join("\0");
    const current = grouped.get(key);
    if (current) current.occurrences += 1;
    else grouped.set(key, { ...item, key, occurrences: 1 });
  }
  return [...grouped.values()];
}

/**
 * The confirmed entrypoints among these paths.
 *
 * Read straight off the first step of each walk rather than searched for: a
 * path the engine started at a route is the only evidence that the route
 * reaches this code at all.
 */
function entryPoints(traces: MapTrace[]): Array<{ id: string; name: string; handler: string | null }> {
  const found = new Map<string, { id: string; name: string; handler: string | null }>();
  for (const trace of traces) {
    const [entry, next] = trace.steps;
    if (!entry || entry.role !== "endpoint" || found.has(entry.id)) continue;
    found.set(entry.id, { id: entry.id, name: entry.name, handler: next?.name ?? null });
  }
  return [...found.values()];
}

function SourceView({ source }: { source: SourceExcerpt }) {
  return (
    <section className="inspector-section">
      <h3>
        소스 <span className="inspector-path">{evidenceLabel({ path: source.path, line: source.hitLine })}</span>
      </h3>
      <div className="source-excerpt">
        {source.lines.map((line, index) => {
          const lineNumber = source.startLine + index;
          return (
            <div className={lineNumber === source.hitLine ? "source-line hit" : "source-line"} key={lineNumber}>
              <span className="source-number">{lineNumber}</span>
              <span className="source-text">{line}</span>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function evidenceClass(item: EvidenceRef, source: SourceExcerpt | null): string {
  const active = source && source.path === item.path && source.hitLine === item.line;
  return active ? "evidence-row active" : "evidence-row";
}

function relationCount(selection: Selection, truth: TruthClass): number {
  return selection.relations
    .filter((relation) => relation.truth === truth)
    .reduce((sum, relation) => sum + relation.count, 0);
}

/** The selection contract has counts, not target names, so identical labels read as one honest aggregate. */
function groupRelationTallies(relations: RelationTally[]): RelationTally[] {
  const grouped = new Map<string, RelationTally>();
  relations.forEach((relation) => {
    const key = `${relation.truth}:${relation.dispatch ?? ""}:${relation.label}`;
    const current = grouped.get(key);
    grouped.set(key, current ? { ...current, count: current.count + relation.count } : { ...relation });
  });
  return [...grouped.values()];
}

/** Candidates read in their own colour so they never pass for verified facts. */
function truthTextClass(truth: TruthClass): string {
  return truth === "candidate" ? "is-candidate" : "";
}
