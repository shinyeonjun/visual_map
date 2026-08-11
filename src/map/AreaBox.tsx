import { ArrowRoutingRegular as FlowRight, CubeRegular as Boxes } from "@fluentui/react-icons";
import { cx } from "./classNames";
import { MAP_DETAIL_POLICY } from "./detail";
import type { MapDetail } from "./detail";
import { DEFAULT_AREA_WIDTH, areaItemCount } from "./layout";
import { categoryLabel, fallbackReasonLabel, traceStateLabel } from "./presentation";
import type { MapArea, TraceState } from "./types";

interface AreaBoxProps {
  area: MapArea;
  fallbackPosition: { x: number; y: number };
  detail: MapDetail;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onHover: (id: string | null) => void;
  /** Opens the area's confirmed execution paths as their own view. */
  onOpenTrace: (id: string) => void;
}

export function AreaBox({ area, fallbackPosition, detail, selectedId, onSelect, onHover, onOpenTrace }: AreaBoxProps) {
  const policy = MAP_DETAIL_POLICY[detail];
  const open = area.areas.length > 0 || area.nodes.length > 0;
  const position = area.position ?? fallbackPosition;
  return (
    <article
      className={cx(
        "map-area",
        `detail-${detail}`,
        `category-${area.category}`,
        open && "open",
        selectedId === area.id && "selected",
      )}
      data-area-id={area.id}
      style={{ left: position.x, top: position.y, width: area.width ?? DEFAULT_AREA_WIDTH }}
      onPointerEnter={() => onHover(area.id)}
      onPointerLeave={() => onHover(null)}
    >
      <button type="button" className="map-area-head" onClick={() => onSelect(area.id)}>
        <span className="map-area-icon" aria-hidden="true">
          <Boxes fontSize={17} />
        </span>
        <span className="map-area-title">
          {/*
            A name the analysis copied from the code's own structure is marked
            as one. Reading it the same as an evidence-derived name is how a
            reader ends up trusting a label nothing actually proved.
          */}
          <span
            className={cx("map-area-name", area.labelSource === "structural" && "structural")}
            title={area.fallbackReason ? fallbackReasonLabel(area.fallbackReason) : undefined}
          >
            {area.name}
          </span>
          {area.originalName ? <span className="map-area-origin">{area.originalName}</span> : null}
        </span>
        <span className="map-area-category">{categoryLabel(area.category)}</span>
        <span className="map-area-level">L{area.depth}</span>
      </button>
      {policy.summary ? <p className="map-area-summary">{area.summary}</p> : null}
      {policy.members && area.trace ? <TraceReceipt state={area.trace.state} stepCount={area.trace.stepCount} /> : null}

      {policy.subareas && area.areas.length > 0 ? (
        <div className="map-feature-list">
          {area.areas.slice(0, 4).map((child) => (
            <button type="button" className="map-feature-row" onClick={() => onSelect(child.id)} key={child.id}>
              <span>
                <strong>{child.name}</strong>
                <small>{child.summary}</small>
              </span>
              <em>{child.nodes.length.toLocaleString("ko-KR")}단계</em>
            </button>
          ))}
          {area.areas.length > 4 ? <span className="map-feature-more">+{area.areas.length - 4}개 더 있음</span> : null}
        </div>
      ) : null}

      {policy.summary ? (
        <footer className="map-area-footer">
          {/*
            Relations that leave this area, split by how far the engine got
            with them, plus the gaps that reach it. Kept apart from the member
            count above: one says how big the area is, this says how much of
            its edge is actually known.
          */}
          <span className="map-area-boundary" title="이 영역 경계를 넘는 관계">
            <b className="verified">{area.boundaryRelationCounts.verified.toLocaleString("ko-KR")}</b>
            <b className="structural">{area.boundaryRelationCounts.structural.toLocaleString("ko-KR")}</b>
            <b className="candidate">{area.boundaryRelationCounts.candidate.toLocaleString("ko-KR")}</b>
            {area.affectingAnalysisGapCount > 0 ? (
              <em title="이 영역에 영향을 주는 분석 공백">
                공백 {area.affectingAnalysisGapCount.toLocaleString("ko-KR")}
              </em>
            ) : null}
          </span>
          {/*
            Drilling in is a deliberate act, not something a click on the card
            does by accident: selecting an area describes it in the panel, and
            this is the separate step that leaves the map for its flow.
          */}
          <button type="button" className="map-area-flow" onClick={() => onOpenTrace(area.id)}>
            흐름 보기 <FlowRight fontSize={13} />
          </button>
        </footer>
      ) : (
        <p className="map-area-overview-count">
          {categoryLabel(area.category)}
          <span>항목 {areaItemCount(area).toLocaleString("ko-KR")}</span>
        </p>
      )}
    </article>
  );
}

function TraceReceipt({ state, stepCount }: { state: TraceState; stepCount: number }) {
  return (
    <span className={`map-trace-receipt ${state}`} title="정적 분석으로 확인한 실행 순서">
      실행 경로 {Math.max(0, stepCount - 1).toLocaleString("ko-KR")}홉 · {traceStateLabel(state)}
    </span>
  );
}
