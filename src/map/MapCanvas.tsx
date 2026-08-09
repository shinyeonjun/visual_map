import {
  AddRegular as Plus,
  CodeRegular as Code2,
  CubeRegular as Boxes,
  DatabaseRegular as Database,
  FlashRegular as Zap,
  FullScreenMaximizeRegular as Maximize,
  GlobeRegular as Globe,
  LayerRegular as Layers,
  PersonRegular as UserRound,
  SubtractRegular as Minus,
  TableRegular as Table2,
} from "@fluentui/react-icons";
import { useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { drawableRelations, flattenAreas } from "./types";
import type { MapArea, MapNode, MapView, NodeRole, TraceState } from "./types";
import { useCanvasViewport } from "./useCanvasViewport";

/** The smallest useful canvas. Larger maps expand from their actual layout. */
const MIN_WORLD_WIDTH = 1440;
const MIN_WORLD_HEIGHT = 1080;
/** Where a top-level area sits when the reader has not moved it yet. */
const DEFAULT_AREA_WIDTH = 232;
const CANVAS_PADDING = 72;
/** Below this scale the canvas becomes a responsibility overview. */
const DETAIL_SCALE = 0.55;

type Rect = { x: number; y: number; width: number; height: number };

export function MapCanvas({
  view,
  selectedId,
  onSelect,
}: {
  view: MapView;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const fallbackPositions = useMemo(() => defaultPositions(view.areas), [view.areas]);
  const world = useMemo(() => mapWorld(view, fallbackPositions), [fallbackPositions, view]);
  const canvas = useCanvasViewport(world.width, world.height);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const detailed = canvas.view.scale >= DETAIL_SCALE;
  const rects = useAreaRects(stageRef, view, detailed);
  const relations = useMemo(() => drawableRelations(view), [view]);
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
      className={panelClass("map-canvas", !detailed && "overview", canvas.panning && "panning")}
      ref={canvas.viewRef}
      style={canvas.gridStyle as CSSProperties}
      {...canvas.handlers}
    >
      <div className="map-mode">
        <Boxes fontSize={15} aria-hidden="true" />
        {detailed ? "상세 구조" : "전체 구조"} · {view.areas.length.toLocaleString("ko-KR")}개 영역
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
          {routedRelations.map(({ relation, path }) => {
            return (
              <path
                key={relation.id}
                className={`map-wire ${relation.truth}`}
                d={path.d}
                markerEnd={`url(#map-arrow-${relation.truth})`}
              />
            );
          })}
        </svg>

        {detailed
          ? routedRelations.map(({ relation, path }) => (
              <span
                key={`${relation.id}-count`}
                className={`map-wire-count ${relation.truth}`}
                style={{ left: path.midX, top: path.midY }}
              >
                {relation.label} {relation.count.toLocaleString("ko-KR")}
              </span>
            ))
          : null}

        {view.areas.map((area, index) => (
          <AreaBox
            key={area.id}
            area={area}
            fallbackPosition={fallbackPositions[index]}
            detailed={detailed}
            selectedId={selectedId}
            onSelect={onSelect}
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

function AreaBox({
  area,
  fallbackPosition,
  detailed,
  selectedId,
  onSelect,
}: {
  area: MapArea;
  fallbackPosition: { x: number; y: number };
  detailed: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const open = area.areas.length > 0 || area.nodes.length > 0;
  const position = area.position ?? fallbackPosition;
  return (
    <article
      className={panelClass("map-area", open && "open", !detailed && "overview", selectedId === area.id && "selected")}
      data-area-id={area.id}
      style={{ left: position.x, top: position.y, width: area.width ?? DEFAULT_AREA_WIDTH }}
    >
      <button type="button" className="map-area-head" onClick={() => onSelect(area.id)}>
        <span className="map-area-name">{area.name}</span>
        {area.originalName ? <span className="map-area-origin">{area.originalName}</span> : null}
        <span className="map-area-level">L{area.depth}</span>
      </button>
      {detailed ? (
        <>
          <p className="map-area-summary">{area.summary}</p>
          {area.trace ? <TraceReceipt state={area.trace.state} stepCount={area.trace.stepCount} /> : null}

          {area.areas.length > 0 ? (
            <div className="map-subareas">
              {area.areas.map((child) => (
                <section className="map-subarea" key={child.id}>
                  <button type="button" className="map-subarea-head" onClick={() => onSelect(child.id)}>
                    <span className="map-subarea-name">{child.name}</span>
                    <span className="map-area-level">L{child.depth}</span>
                  </button>
                  <p className="map-subarea-summary">{child.summary}</p>
                  {child.trace ? <TraceReceipt state={child.trace.state} stepCount={child.trace.stepCount} /> : null}
                  <NodeChain nodes={child.nodes} selectedId={selectedId} onSelect={onSelect} />
                  {child.hiddenNodeCount > 0 ? (
                    <button type="button" className="map-more">
                      +{child.hiddenNodeCount.toLocaleString("ko-KR")} 더 보기
                    </button>
                  ) : null}
                </section>
              ))}
            </div>
          ) : null}

          {area.areas.length === 0 && area.nodes.length > 0 ? (
            <NodeChain nodes={area.nodes} selectedId={selectedId} onSelect={onSelect} />
          ) : null}
        </>
      ) : (
        <p className="map-area-overview-count">
          {area.areas.length > 0 ? `하위 영역 ${area.areas.length.toLocaleString("ko-KR")}` : "하위 영역 없음"}
          <span>확인 항목 {areaItemCount(area).toLocaleString("ko-KR")}</span>
        </p>
      )}
    </article>
  );
}

/**
 * The members of an area, in the order the engine resolved them.
 *
 * The arrows between them carry no number. Each hop stands for a verified
 * call the engine traced, and the counts live in the inspector — five labelled
 * arrows in a column read as five numbers to compare rather than one flow.
 */
function NodeChain({
  nodes,
  selectedId,
  onSelect,
}: {
  nodes: MapNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  if (nodes.length === 0) return null;
  return (
    <div className="map-chain">
      {nodes.map((node, index) => (
        <div key={`${node.id}:${index}`}>
          {index > 0 ? <i className="map-hop" aria-hidden="true" /> : null}
          <button
            type="button"
            className={panelClass("map-node", selectedId === node.id && "selected")}
            onClick={() => onSelect(node.id)}
          >
            <span className="map-node-icon" aria-hidden="true">
              <NodeIcon role={node.role} />
            </span>
            <span className="map-node-id">
              <span className="map-node-name">{node.name}</span>
              <span className="map-node-kind">{node.kind}</span>
            </span>
          </button>
        </div>
      ))}
    </div>
  );
}

function TraceReceipt({ state, stepCount }: { state: TraceState; stepCount: number }) {
  return (
    <span className={`map-trace-receipt ${state}`} title="정적 분석으로 확인한 실행 순서">
      실행 경로 {Math.max(0, stepCount - 1).toLocaleString("ko-KR")}홉 · {traceStateLabel(state)}
    </span>
  );
}

function traceStateLabel(state: TraceState): string {
  if (state === "complete") return "경로 끝 확인";
  if (state === "partial") return "일부만 확인";
  if (state === "gap") return "분석 공백 있음";
  if (state === "cycle") return "순환 감지";
  return "표시 깊이 제한";
}

function NodeIcon({ role }: { role: NodeRole }) {
  const size = 15;
  if (role === "endpoint") return <Globe fontSize={size} />;
  if (role === "controller") return <UserRound fontSize={size} />;
  if (role === "service") return <Layers fontSize={size} />;
  if (role === "repository") return <Database fontSize={size} />;
  if (role === "table") return <Table2 fontSize={size} />;
  if (role === "event") return <Zap fontSize={size} />;
  return <Code2 fontSize={size} />;
}

/**
 * Where every area actually landed.
 *
 * Areas size themselves from their contents, so a relation cannot be routed
 * from stored coordinates alone — an area holding three members is a different
 * shape from one holding twelve. Measured after layout and re-measured when
 * anything resizes.
 */
function useAreaRects(stageRef: React.RefObject<HTMLDivElement | null>, view: MapView, detailed: boolean) {
  const [rects, setRects] = useState<Map<string, Rect>>(new Map());
  const signature =
    `${detailed ? "detail" : "overview"}:` +
    flattenAreas(view.areas)
      .map((area) => area.id)
      .join("|");

  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    let animationFrame: number | null = null;
    const measure = () => {
      const origin = stage.getBoundingClientRect();
      const scale = origin.width / stage.offsetWidth || 1;
      const next = new Map<string, Rect>();
      for (const element of stage.querySelectorAll<HTMLElement>("[data-area-id]")) {
        const id = element.dataset.areaId;
        if (!id) continue;
        const box = element.getBoundingClientRect();
        next.set(id, {
          x: (box.left - origin.left) / scale,
          y: (box.top - origin.top) / scale,
          width: box.width / scale,
          height: box.height / scale,
        });
      }
      setRects((current) => (rectMapsEqual(current, next) ? current : next));
    };

    const scheduleMeasure = () => {
      if (typeof requestAnimationFrame === "undefined") {
        measure();
        return;
      }
      if (animationFrame !== null) return;
      animationFrame = requestAnimationFrame(() => {
        animationFrame = null;
        measure();
      });
    };

    scheduleMeasure();
    if (typeof ResizeObserver === "undefined") {
      return () => {
        if (animationFrame !== null && typeof cancelAnimationFrame !== "undefined") {
          cancelAnimationFrame(animationFrame);
        }
      };
    }
    const observer = new ResizeObserver(scheduleMeasure);
    observer.observe(stage);
    for (const element of stage.querySelectorAll("[data-area-id]")) observer.observe(element);
    return () => {
      observer.disconnect();
      if (animationFrame !== null && typeof cancelAnimationFrame !== "undefined") {
        cancelAnimationFrame(animationFrame);
      }
    };
  }, [signature, stageRef]);

  return rects;
}

/** An orthogonal route between two boxes, plus where to hang its count. */
function elbow(from: Rect | undefined, to: Rect | undefined) {
  if (!from || !to) return null;
  const fromCenterY = from.y + from.height / 2;
  const toCenterY = to.y + to.height / 2;

  if (to.x >= from.x + from.width) {
    const startX = from.x + from.width;
    const endX = to.x;
    const midX = (startX + endX) / 2;
    return {
      d: `M${startX} ${fromCenterY} L${midX} ${fromCenterY} L${midX} ${toCenterY} L${endX} ${toCenterY}`,
      midX,
      midY: Math.min(fromCenterY, toCenterY) + Math.abs(fromCenterY - toCenterY) / 2,
    };
  }

  const startY = from.y + from.height;
  const endY = to.y;
  const fromCenterX = from.x + from.width / 2;
  const toCenterX = to.x + to.width / 2;
  const midY = (startY + endY) / 2;
  return {
    d: `M${fromCenterX} ${startY} L${fromCenterX} ${midY} L${toCenterX} ${midY} L${toCenterX} ${endY}`,
    midX: (fromCenterX + toCenterX) / 2,
    midY,
  };
}

function rectMapsEqual(left: Map<string, Rect>, right: Map<string, Rect>): boolean {
  if (left.size !== right.size) return false;
  for (const [id, next] of right) {
    const current = left.get(id);
    if (!current) return false;
    if (
      Math.abs(current.x - next.x) > 0.25 ||
      Math.abs(current.y - next.y) > 0.25 ||
      Math.abs(current.width - next.width) > 0.25 ||
      Math.abs(current.height - next.height) > 0.25
    ) {
      return false;
    }
  }
  return true;
}

function areaItemCount(area: MapArea): number {
  return area.nodes.length + area.hiddenNodeCount + area.areas.reduce((sum, child) => sum + areaItemCount(child), 0);
}

function estimateAreaHeight(area: MapArea): number {
  const traceHeight = area.trace ? 24 : 0;
  const hiddenHeight = area.hiddenNodeCount > 0 ? 36 : 0;
  if (area.areas.length === 0) {
    return 88 + traceHeight + hiddenHeight + area.nodes.length * 58;
  }
  return 112 + traceHeight + Math.max(0, ...area.areas.map(estimateAreaHeight));
}

function defaultColumnCount(areaCount: number): number {
  if (areaCount <= 4) return Math.max(1, areaCount);
  if (areaCount <= 12) return 4;
  if (areaCount <= 30) return 5;
  if (areaCount <= 60) return 6;
  if (areaCount <= 112) return 7;
  return 8;
}

/** A deterministic, non-overlapping grid when stored positions are absent. */
function defaultPositions(areas: MapArea[]): Array<{ x: number; y: number }> {
  const positions: Array<{ x: number; y: number }> = [];
  const columns = defaultColumnCount(areas.length);
  let y = 96;
  for (let start = 0; start < areas.length; start += columns) {
    const row = areas.slice(start, start + columns);
    let x = 48;
    let rowHeight = 0;
    row.forEach((area, offset) => {
      positions[start + offset] = { x, y };
      x += (area.width ?? DEFAULT_AREA_WIDTH) + 56;
      rowHeight = Math.max(rowHeight, estimateAreaHeight(area));
    });
    y += rowHeight + 64;
  }
  return positions;
}

function mapWorld(view: MapView, fallbackPositions: Array<{ x: number; y: number }>) {
  let maxX = MIN_WORLD_WIDTH - CANVAS_PADDING;
  let maxY = MIN_WORLD_HEIGHT - CANVAS_PADDING;
  view.areas.forEach((area, index) => {
    const position = area.position ?? fallbackPositions[index] ?? { x: CANVAS_PADDING, y: CANVAS_PADDING };
    maxX = Math.max(maxX, position.x + (area.width ?? DEFAULT_AREA_WIDTH));
    maxY = Math.max(maxY, position.y + estimateAreaHeight(area));
  });
  return {
    width: Math.ceil(maxX + CANVAS_PADDING),
    height: Math.ceil(maxY + CANVAS_PADDING),
  };
}

function panelClass(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
