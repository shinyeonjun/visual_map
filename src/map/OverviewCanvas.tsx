import {
  AddRegular as Plus,
  CubeRegular as Boxes,
  FullScreenMaximizeRegular as Maximize,
  SubtractRegular as Minus,
} from "@fluentui/react-icons";
import { useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { AreaBox } from "./AreaBox";
import { cx } from "./classNames";
import { detailForScale } from "./detail";
import { defaultPositions, elbow, mapWorld } from "./layout";
import { detailLabel } from "./presentation";
import { drawableRelations, relationsTouching } from "./types";
import type { DispatchKind, MapDispatchTally, MapView } from "./types";
import { useAreaRects } from "./useAreaRects";
import { useCanvasViewport } from "./useCanvasViewport";

/**
 * The least certain dispatch a bundled relation contains, or null when every
 * call in it was pinned exactly.
 *
 * One line stands for many edges, so it has to report the weakest claim among
 * them: drawing forty resolved calls and two dynamic ones as fully resolved
 * would hide exactly the two worth knowing about.
 */
function loosestDispatch(dispatches: MapDispatchTally[] | undefined): DispatchKind | null {
  const order: DispatchKind[] = ["dynamic", "unknown", "interface", "virtual"];
  return order.find((kind) => dispatches?.some((item) => item.dispatch === kind && item.count > 0)) ?? null;
}

/** Beyond this many lines at once, the labels stop being readable. */
const MAX_LABELLED_RELATIONS = 4;

interface OverviewCanvasProps {
  view: MapView;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onOpenTrace: (id: string) => void;
}

export function OverviewCanvas({ view, selectedId, onSelect, onOpenTrace }: OverviewCanvasProps) {
  const fallbackPositions = useMemo(() => defaultPositions(view.areas), [view.areas]);
  const world = useMemo(() => mapWorld(view, fallbackPositions), [fallbackPositions, view]);
  const canvas = useCanvasViewport(world.width, world.height);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const detail = detailForScale(canvas.view.scale);
  const rects = useAreaRects(stageRef, view, detail);
  const relations = useMemo(() => drawableRelations(view), [view]);
  /*
    Pointing beats having selected. Hovering previews a neighbourhood without
    disturbing the selection the panel is describing, and releasing the
    pointer returns the map to that selection.
  */
  const focused = useMemo(() => relationsTouching(view, hoveredId ?? selectedId), [hoveredId, selectedId, view]);
  const labelled = useMemo(() => relationsTouching(view, hoveredId), [hoveredId, view]);
  const routedRelations = useMemo(
    () =>
      relations.flatMap((relation) => {
        const path = elbow(rects.get(relation.from), rects.get(relation.to));
        return path ? [{ relation, path }] : [];
      }),
    [rects, relations],
  );

  return (
    <div
      className={cx("map-canvas", `detail-${detail}`, canvas.panning && "panning")}
      ref={canvas.viewRef}
      style={canvas.gridStyle as CSSProperties}
      {...canvas.handlers}
      onPointerLeave={() => setHoveredId(null)}
    >
      <div className="map-mode">
        <Boxes fontSize={15} aria-hidden="true" />
        {detailLabel(detail)} · {view.areas.length.toLocaleString("ko-KR")}개 영역
        {/*
          Gaps the engine could not assign to any one area belong to the map,
          not to whichever card they were nearest. Hiding them because they
          have no home is exactly how unmeasured work disappears.
        */}
        {view.unattributedAnalysisGapCount > 0 ? (
          <em title="특정 영역에 배정할 수 없는 분석 공백">
            전역 공백 {view.unattributedAnalysisGapCount.toLocaleString("ko-KR")}
          </em>
        ) : null}
      </div>

      <div className="map-stage" ref={stageRef} style={canvas.stageStyle as CSSProperties}>
        <svg className="map-wires" viewBox={`0 0 ${world.width} ${world.height}`} aria-hidden="true">
          <defs>
            {(["verified", "structural", "candidate"] as const).map((truth) => (
              <marker
                key={truth}
                id={`map-arrow-${truth}`}
                markerWidth="7"
                markerHeight="7"
                refX="6.2"
                refY="3.5"
                orient="auto"
              >
                <path d="M0 0 L7 3.5 L0 7z" className={`map-arrowhead ${truth}`} />
              </marker>
            ))}
          </defs>
          {routedRelations.map(({ relation, path }) => (
            <path
              key={relation.id}
              className={cx(
                "map-wire",
                relation.truth,
                // A bundle whose calls are mostly resolved at runtime is not
                // the same claim as one the engine pinned exactly, so the two
                // must not share a line style here either.
                loosestDispatch(relation.dispatches) && `dispatch-${loosestDispatch(relation.dispatches)}`,
                focused && !focused.has(relation.id) && "muted",
              )}
              d={path.d}
              markerEnd={`url(#map-arrow-${relation.truth})`}
            />
          ))}
        </svg>

        {/*
          A count on every line is a count on none of them: the reader has not
          asked about any of these yet, and at a distance the pills land on the
          cards they belong to. Pointing at an area answers for that area.
        */}
        {routedRelations.map(({ relation, path }) =>
          labelled && labelled.size <= MAX_LABELLED_RELATIONS && labelled.has(relation.id) ? (
            <span
              key={`${relation.id}-count`}
              className={`map-wire-count ${relation.truth}`}
              style={{ left: path.midX, top: path.midY }}
            >
              {relation.label} {relation.count.toLocaleString("ko-KR")}
            </span>
          ) : null,
        )}

        {view.areas.map((area, index) => (
          <AreaBox
            key={area.id}
            area={area}
            fallbackPosition={fallbackPositions[index]}
            detail={detail}
            selectedId={selectedId}
            onSelect={onSelect}
            onHover={setHoveredId}
            onOpenTrace={onOpenTrace}
          />
        ))}
      </div>

      <div className="map-legend">
        <span>
          <i className="map-swatch verified" />
          확인된 관계
        </span>
        <span>
          <i className="map-swatch structural" />
          구조 관계
        </span>
        <span>
          <i className="map-swatch candidate" />
          후보
        </span>
      </div>

      <div className="map-zoom">
        <button type="button" onClick={canvas.zoomOut} aria-label="축소">
          <Minus fontSize={14} />
        </button>
        <span className="map-zoom-value">{Math.round(canvas.view.scale * 100)}%</span>
        <button type="button" onClick={canvas.zoomIn} aria-label="확대">
          <Plus fontSize={14} />
        </button>
        <button type="button" onClick={canvas.fit} aria-label="화면에 맞추기">
          <Maximize fontSize={14} />
        </button>
      </div>
    </div>
  );
}
