import { CodeRegular as FileCode2, FlowRegular as Waypoints } from "@fluentui/react-icons";
import type { EvidenceRef, MapTrace, Selection, SourceExcerpt, TraceState, TruthClass } from "./types";

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
}: {
  selection: Selection | null;
  onHighlight?: (id: string) => void;
  onOpenEvidence?: (evidence: EvidenceRef) => void;
}) {
  if (!selection) {
    return (
      <div className="inspector-empty">
        <Waypoints fontSize={22} aria-hidden="true" />
        <p>지도에서 영역이나 항목을 선택하세요.</p>
      </div>
    );
  }

  return (
    <div className="inspector-body">
      <section className="inspector-section">
        <h3>역할</h3>
        <p>{selection.role}</p>
      </section>

      {(selection.traces?.length ?? 0) > 0 ? (
        <section className="inspector-section">
          <h3>정적 실행 경로</h3>
          <div className="trace-list">
            {selection.traces?.map((trace) => (
              <TraceRow trace={trace} key={trace.id} />
            ))}
          </div>
        </section>
      ) : null}

      {selection.relations.length > 0 ? (
        <section className="inspector-section">
          <h3>관계</h3>
          <div className="relation-tally">
            {selection.relations.map((relation) => (
              <div className="relation-row" key={`${relation.truth}-${relation.label}`}>
                <i className={`relation-mark ${relation.truth}`} aria-hidden="true" />
                <span className={truthTextClass(relation.truth)}>{relation.label}</span>
                <strong className={truthTextClass(relation.truth)}>{relation.count.toLocaleString("ko-KR")}</strong>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {selection.evidence.length > 0 ? (
        <section className="inspector-section">
          <h3>증거</h3>
          <div className="evidence-list">
            {selection.evidence.map((item) => (
              <button
                type="button"
                className={evidenceClass(item, selection.source)}
                key={`${item.path}:${item.line ?? ""}`}
                onClick={() => onOpenEvidence?.(item)}
                title={onOpenEvidence ? "VS Code에서 근거 열기" : undefined}
              >
                <FileCode2 fontSize={13} aria-hidden="true" />
                {evidenceLabel(item)}
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {selection.source ? <SourceView source={selection.source} /> : null}

      {onHighlight ? (
        <button type="button" className="inspector-link" onClick={() => onHighlight(selection.id)}>
          <Waypoints fontSize={14} aria-hidden="true" />
          지도에서 경로 강조
        </button>
      ) : null}
    </div>
  );
}

function TraceRow({ trace }: { trace: MapTrace }) {
  return (
    <div className="trace-row">
      <span className={`trace-state ${trace.state}`}>{traceStateLabel(trace.state)}</span>
      <ol>
        {trace.steps.map((step, index) => (
          <li key={`${step.id}:${index}`}>
            <strong>{step.name}</strong>
            <small>{step.kind}</small>
          </li>
        ))}
      </ol>
    </div>
  );
}

function traceStateLabel(state: TraceState): string {
  if (state === "complete") return "경로 끝 확인";
  if (state === "partial") return "일부만 확인";
  if (state === "gap") return "분석 공백 있음";
  if (state === "cycle") return "순환 감지";
  return "표시 깊이 제한";
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

function evidenceLabel(item: EvidenceRef): string {
  const name = item.path.split(/[\\/]/).filter(Boolean).pop() ?? item.path;
  return item.line ? `${name}:${item.line}` : name;
}

function evidenceClass(item: EvidenceRef, source: SourceExcerpt | null): string {
  const active = source && source.path === item.path && source.hitLine === item.line;
  return active ? "evidence-row active" : "evidence-row";
}

/** Candidates read in their own colour so they never pass for verified facts. */
function truthTextClass(truth: TruthClass): string {
  return truth === "candidate" ? "is-candidate" : "";
}
