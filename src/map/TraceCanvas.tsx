import { ArrowLeftRegular as ArrowLeft, BranchForkRegular as BranchFork } from "@fluentui/react-icons";
import { useMemo } from "react";
import type { CSSProperties } from "react";
import { cx } from "./classNames";
import {
  NodeIcon,
  controlFlags,
  dispatchNote,
  evidenceLabel,
  roleLabel,
  terminalReason,
  traceStateLabel,
} from "./presentation";
import { buildTraceGraph } from "./traceGraph";
import type { TraceGraph } from "./traceGraph";
import type { MapTrace } from "./types";
import { useCanvasViewport } from "./useCanvasViewport";

/**
 * One entrypoint's confirmed execution, drawn as lines.
 *
 * A path is an ordered sequence, and sequences read left to right, so the
 * engine's step index is the horizontal position and nothing here re-orders
 * it. Paths that begin identically share one line until they actually part,
 * which is what turns eight separate walks into a shape a person can hold.
 *
 * A walk that stopped early ends in a broken stub that says why. Continuing
 * the line past the last confirmed step would be the one thing this map must
 * never do.
 */

const NODE_WIDTH = 190;
const NODE_HEIGHT = 70;
const COLUMN_PITCH = NODE_WIDTH + 74;
const LANE_PITCH = NODE_HEIGHT + 30;
const HEADER_BAND = 46;
const EDGE_INSET = 48;
const STAGE_PADDING = 40;
/** A stub is short on purpose: it is a stop, not a step. */
const TERMINAL_LENGTH = 56;
/*
  This view exists to be read: symbol names, and the file and line a call is
  written on. Fitting a deep path into the pane would shrink that text past
  legibility, so below this the map overflows and is panned instead.
*/
const MIN_READABLE_SCALE = 0.85;

interface TraceCanvasProps {
  title: string;
  summary: string;
  traces: MapTrace[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onBack: () => void;
}

export function TraceCanvas({ title, summary, traces, selectedId, onSelect, onBack }: TraceCanvasProps) {
  const graph = useMemo(() => buildTraceGraph(traces), [traces]);
  const world = worldSize(graph);
  const canvas = useCanvasViewport(world.width, world.height, MIN_READABLE_SCALE);
  /*
    Selecting a step asks "which routes go through here". Every path that runs
    over it stays lit and the rest recede, so a branch can be followed without
    the other seven getting in the way.
  */
  const litTraces = useMemo(() => litTraceIds(graph, selectedId), [graph, selectedId]);
  const nodeByKey = useMemo(() => new Map(graph.nodes.map((item) => [item.key, item])), [graph.nodes]);

  const completed = traces.filter((trace) => trace.state === "complete").length;

  return (
    <section className="trace-canvas" aria-label={`${title} 실행 경로`}>
      <header className="trace-header">
        <button type="button" className="trace-back" onClick={onBack}>
          <ArrowLeft fontSize={15} /> 전체 구조
        </button>
        <div className="trace-heading-copy">
          <span className="trace-eyebrow">실행 경로</span>
          <h1>{title}</h1>
          {summary ? <p>{summary}</p> : null}
        </div>
        <dl className="trace-summary">
          <div>
            <dt>경로</dt>
            <dd>{traces.length.toLocaleString("ko-KR")}</dd>
          </div>
          <div>
            <dt>끝까지 확인</dt>
            <dd>{completed.toLocaleString("ko-KR")}</dd>
          </div>
          <div>
            <dt>단계</dt>
            <dd>{graph.columnCount.toLocaleString("ko-KR")}</dd>
          </div>
        </dl>
      </header>

      {graph.nodes.length === 0 ? (
        <div className="trace-empty">
          <BranchFork fontSize={22} />
          <strong>확인된 실행 경로가 없습니다.</strong>
          <span>정적 분석이 순서를 확정한 경로만 여기에 그려집니다.</span>
        </div>
      ) : (
        <div
          className={cx("trace-plot", canvas.panning && "panning")}
          ref={canvas.viewRef}
          style={canvas.gridStyle as CSSProperties}
          {...canvas.handlers}
        >
          <div className="trace-stage" style={canvas.stageStyle as CSSProperties}>
            <svg className="trace-lines" width={world.width} height={world.height} aria-hidden="true">
              {/*
                The line itself carries how sure the engine is of the target.
                A virtual or dynamic call reached *a* target, not necessarily
                the only one, and a solid line for both would say otherwise.
              */}
              {graph.edges.map((edge) => {
                const from = nodeByKey.get(edge.fromKey);
                const to = nodeByKey.get(edge.toKey);
                if (!from || !to) return null;
                return (
                  <path
                    key={edge.key}
                    className={cx(
                      "trace-line",
                      `dispatch-${edge.hop?.dispatch ?? "unreported"}`,
                      litTraces && !overlaps(edge.traceIds, litTraces) && "muted",
                    )}
                    d={connector(from, to)}
                  />
                );
              })}
              {graph.terminals.map((terminal) => {
                const owner = nodeByKey.get(terminal.nodeKey);
                if (!owner) return null;
                const startX = columnX(owner.column) + NODE_WIDTH;
                const startY = laneY(owner.lane) + NODE_HEIGHT / 2;
                const endY = laneY(terminal.lane) + NODE_HEIGHT / 2;
                return (
                  <path
                    key={terminal.key}
                    className={`trace-line stop ${terminal.state}`}
                    d={connector(
                      { column: owner.column, lane: owner.lane },
                      { column: owner.column, lane: terminal.lane },
                      startX,
                      startY,
                      startX + TERMINAL_LENGTH,
                      endY,
                    )}
                  />
                );
              })}
            </svg>

            <div className="trace-columns" aria-hidden="true">
              {graph.columns.map((column) => (
                <span key={column.index} className="trace-column-head" style={{ left: columnX(column.index) }}>
                  <b>{column.index + 1}</b>
                  {column.role ? roleLabel(column.role) : `단계 ${column.index + 1}`}
                </span>
              ))}
            </div>

            {graph.nodes.map((item) => (
              <button
                type="button"
                key={item.key}
                className={cx(
                  "trace-node",
                  selectedId === item.node.id && "selected",
                  litTraces && !overlaps(item.traceIds, litTraces) && "muted",
                )}
                style={{ left: columnX(item.column), top: laneY(item.lane), width: NODE_WIDTH, height: NODE_HEIGHT }}
                onClick={() => onSelect(item.node.id)}
              >
                <span className="trace-node-icon" aria-hidden="true">
                  <NodeIcon role={item.node.role} />
                </span>
                <span className="trace-node-copy">
                  <strong>{item.node.name}</strong>
                  {/*
                    Where the symbol is written, never where something called
                    it — a caller's line under a callee's name would be a lie
                    the reader has no way to catch. Without one, say the kind.
                  */}
                  {item.node.definition ? (
                    <small className="definition">{evidenceLabel(item.node.definition)}</small>
                  ) : (
                    <small>{item.node.kind}</small>
                  )}
                </span>
                {item.traceIds.length > 1 ? <em title="이 단계를 지나는 경로 수">{item.traceIds.length}</em> : null}
              </button>
            ))}

            {/*
              Hop facts only for the path being followed. Every line carrying
              its call site at once is the clutter the overview already taught
              us to avoid; asked for, it is exactly what the reader wants.
            */}
            {litTraces
              ? graph.edges.map((edge) => {
                  const from = nodeByKey.get(edge.fromKey);
                  const to = nodeByKey.get(edge.toKey);
                  if (!from || !to || !edge.hop || !overlaps(edge.traceIds, litTraces)) return null;
                  const flags = controlFlags(edge.hop.execution);
                  const callSite = edge.hop.execution?.callSite ?? edge.hop.evidence[0] ?? null;
                  if (flags.length === 0 && !callSite) return null;
                  return (
                    <span
                      key={`${edge.key}-hop`}
                      className="trace-hop-chip"
                      style={{ left: midpointX(from, to), top: midpointY(from, to) }}
                      title={dispatchNote(edge.hop.dispatch) ?? undefined}
                    >
                      {flags.map((flag) => (
                        <b key={flag}>{flag}</b>
                      ))}
                      {callSite ? <small>{evidenceLabel(callSite)}</small> : null}
                    </span>
                  );
                })
              : null}

            {graph.terminals.map((terminal) => (
              <span
                key={`${terminal.key}-chip`}
                className={`trace-stop-chip ${terminal.state}`}
                style={{ left: columnX(terminal.column) - 6, top: laneY(terminal.lane) + NODE_HEIGHT / 2 - 13 }}
                title={terminalReason(terminal.state)}
              >
                {traceStateLabel(terminal.state)}
              </span>
            ))}
          </div>

          <div className="trace-legend">
            <span>
              <i className="confirmed" /> 대상 확정
            </span>
            <span>
              <i className="loose" /> 런타임에 대상 결정
            </span>
            <span>
              <i className="stop" /> 여기서 멈춤
            </span>
          </div>
        </div>
      )}
    </section>
  );
}

function worldSize(graph: TraceGraph) {
  return {
    width: Math.max(640, STAGE_PADDING * 2 + graph.columnCount * COLUMN_PITCH + TERMINAL_LENGTH),
    height: Math.max(320, HEADER_BAND + STAGE_PADDING * 2 + graph.laneCount * LANE_PITCH),
  };
}

function columnX(column: number): number {
  return STAGE_PADDING + column * COLUMN_PITCH;
}

/** The empty run between two steps, where a chip sits on no card. */
function midpointX(from: { column: number }, to: { column: number }): number {
  return (columnX(from.column) + NODE_WIDTH + columnX(to.column)) / 2;
}

function midpointY(from: { lane: number }, to: { lane: number }): number {
  return (laneY(from.lane) + laneY(to.lane)) / 2 + NODE_HEIGHT / 2;
}

function laneY(lane: number): number {
  return HEADER_BAND + STAGE_PADDING + lane * LANE_PITCH;
}

/**
 * A line that leaves and arrives flat.
 *
 * The horizontal run at each end keeps a lane readable where several lines
 * meet the same step, and the curve between them only happens when the path
 * actually changes lane — a step that continues straight stays a straight
 * line, which is what makes the trunk obvious.
 */
function connector(
  from: { column: number; lane: number },
  to: { column: number; lane: number },
  startXOverride?: number,
  startYOverride?: number,
  endXOverride?: number,
  endYOverride?: number,
): string {
  const startX = startXOverride ?? columnX(from.column) + NODE_WIDTH;
  const startY = startYOverride ?? laneY(from.lane) + NODE_HEIGHT / 2;
  const endX = endXOverride ?? columnX(to.column);
  const endY = endYOverride ?? laneY(to.lane) + NODE_HEIGHT / 2;
  if (Math.abs(startY - endY) < 0.5) return `M${startX} ${startY} L${endX} ${endY}`;
  const bend = Math.min(EDGE_INSET, Math.max(12, (endX - startX) / 2));
  return `M${startX} ${startY} L${startX + bend} ${startY} C${endX - bend} ${startY} ${startX + bend} ${endY} ${endX - bend} ${endY} L${endX} ${endY}`;
}

/** The paths running through the selected step, or null when none is chosen. */
function litTraceIds(graph: TraceGraph, selectedId: string | null): Set<string> | null {
  if (!selectedId) return null;
  const lit = new Set<string>();
  for (const item of graph.nodes) {
    if (item.node.id === selectedId) for (const id of item.traceIds) lit.add(id);
  }
  return lit.size > 0 ? lit : null;
}

function overlaps(traceIds: string[], lit: Set<string>): boolean {
  return traceIds.some((id) => lit.has(id));
}
