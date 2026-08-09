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
import { useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { drawableRelations, flattenAreas } from "./types";
import type { MapArea, MapNode, MapView, NodeRole, TraceState } from "./types";
import { useCanvasViewport } from "./useCanvasViewport";

/** The world the areas are placed in. Larger than any first view, so it pans. */
const WORLD_WIDTH = 1440;
const WORLD_HEIGHT = 1080;
/** Where a top-level area sits when the reader has not moved it yet. */
const DEFAULT_AREA_WIDTH = 232;

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
  const canvas = useCanvasViewport(WORLD_WIDTH, WORLD_HEIGHT);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const rects = useAreaRects(stageRef, view);
  const relations = drawableRelations(view);

  return (
    <div
      className={panelClass("map-canvas", canvas.panning && "panning")}
      ref={canvas.viewRef}
      style={canvas.gridStyle as CSSProperties}
      {...canvas.handlers}
    >
      <div className="map-mode">
        <Boxes fontSize={15} aria-hidden="true" />
        무한 캔버스
      </div>

      <div className="map-stage" ref={stageRef} style={canvas.stageStyle as CSSProperties}>
        <svg className="map-wires" viewBox={`0 0 ${WORLD_WIDTH} ${WORLD_HEIGHT}`} aria-hidden="true">
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
          {relations.map((relation) => {
            const path = elbow(rects.get(relation.from), rects.get(relation.to));
            if (!path) return null;
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

        {relations.map((relation) => {
          const path = elbow(rects.get(relation.from), rects.get(relation.to));
          if (!path) return null;
          return (
            <span
              key={`${relation.id}-count`}
              className={`map-wire-count ${relation.truth}`}
              style={{ left: path.midX, top: path.midY }}
            >
              {relation.label} {relation.count.toLocaleString("ko-KR")}
            </span>
          );
        })}

        {view.areas.map((area, index) => (
          <AreaBox key={area.id} area={area} fallbackIndex={index} selectedId={selectedId} onSelect={onSelect} />
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
  fallbackIndex,
  selectedId,
  onSelect,
}: {
  area: MapArea;
  fallbackIndex: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const open = area.areas.length > 0 || area.nodes.length > 0;
  const position = area.position ?? defaultPosition(fallbackIndex);
  return (
    <article
      className={panelClass("map-area", open && "open", selectedId === area.id && "selected")}
      data-area-id={area.id}
      style={{ left: position.x, top: position.y, width: area.width ?? DEFAULT_AREA_WIDTH }}
    >
      <button type="button" className="map-area-head" onClick={() => onSelect(area.id)}>
        <span className="map-area-name">{area.name}</span>
        {area.originalName ? <span className="map-area-origin">{area.originalName}</span> : null}
        <span className="map-area-level">L{area.depth}</span>
      </button>
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
function useAreaRects(stageRef: React.RefObject<HTMLDivElement | null>, view: MapView) {
  const [rects, setRects] = useState<Map<string, Rect>>(new Map());
  const signature = flattenAreas(view.areas)
    .map((area) => area.id)
    .join("|");

  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

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
      setRects(next);
    };

    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    for (const element of stage.querySelectorAll("[data-area-id]")) observer.observe(element);
    return () => observer.disconnect();
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

/** A readable grid for areas the reader has not placed yet. */
function defaultPosition(index: number): { x: number; y: number } {
  const columns = 4;
  return { x: 48 + (index % columns) * 288, y: 72 + Math.floor(index / columns) * 260 };
}

function panelClass(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
