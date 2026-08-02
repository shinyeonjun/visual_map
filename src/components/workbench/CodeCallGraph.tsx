import { useMemo } from "react";
import type { CSSProperties } from "react";
import type { VisualEdge, VisualNode } from "../../types/visual-map";
import {
  buildCodeCallGraphLayout,
  callGraphDataLabelStyle,
  callGraphDataPath,
  callGraphSideLabelStyle,
  callGraphSidePath,
  type CallGraphPlaced,
  type CodeCallGraphModel,
} from "./codeCallGraphModel";
import {
  isCandidateRelationEdge,
  relationEdgeShortLabel,
  relationLaneMeta,
  relationNodeLane,
  relationSourceLabel,
} from "./relationMeta";

export function CodeCallGraph({
  model,
  selectedNodeId,
  selectedEdgeId,
  onSelectNode,
  onSelectEdge,
}: {
  model: CodeCallGraphModel;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  onSelectNode: (node: VisualNode) => void;
  onSelectEdge: (edge: VisualEdge) => void;
}) {
  const layout = useMemo(() => buildCodeCallGraphLayout(model), [model]);
  const connections: Array<{ placed: CallGraphPlaced; path: string; labelStyle: CSSProperties }> = [
    ...layout.callers.map((placed) => ({
      placed,
      path: callGraphSidePath("caller", placed, layout.focus),
      labelStyle: callGraphSideLabelStyle("caller", placed, layout.focus),
    })),
    ...layout.callees.map((placed) => ({
      placed,
      path: callGraphSidePath("callee", placed, layout.focus),
      labelStyle: callGraphSideLabelStyle("callee", placed, layout.focus),
    })),
    ...layout.dataTargets.map((placed) => ({
      placed,
      path: callGraphDataPath(layout.focus, placed),
      labelStyle: callGraphDataLabelStyle(layout.focus, placed),
    })),
  ];

  return (
    <section className="code-call-view" aria-label={`${model.focus.title} 호출 지도`}>
      <div className="code-call-canvas" style={{ width: layout.width, height: layout.height }}>
        <svg className="api-connection-lines" viewBox={`0 0 ${layout.width} ${layout.height}`} aria-label="호출 관계선">
          <defs>
            <marker id="ccg-arrow-confirmed" markerHeight="7" markerWidth="7" orient="auto" refX="6" refY="3.5">
              <path d="M0,0 L7,3.5 L0,7 Z" />
            </marker>
            <marker id="ccg-arrow-candidate" markerHeight="7" markerWidth="7" orient="auto" refX="6" refY="3.5">
              <path d="M0,0 L7,3.5 L0,7 Z" />
            </marker>
          </defs>
          {connections.map(({ placed, path }) => {
            const { edge } = placed.connection;
            const candidate = isCandidateRelationEdge(edge);
            return (
              <path
                className={`api-edge-line ${candidate ? "candidate" : "confirmed"} primary${selectedEdgeId === edge.id ? " selected" : ""}`}
                d={path}
                markerEnd={candidate ? "url(#ccg-arrow-candidate)" : "url(#ccg-arrow-confirmed)"}
                role="button"
                tabIndex={0}
                aria-label={`${placed.connection.node.title} ${relationEdgeShortLabel(edge)} 근거 보기`}
                onClick={() => onSelectEdge(edge)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelectEdge(edge);
                  }
                }}
                key={edge.id}
              />
            );
          })}
        </svg>

        {layout.headings.map(({ x, label }) => (
          <span className="code-call-heading" style={{ left: x }} key={label}>{label}</span>
        ))}

        {connections.map(({ placed, labelStyle }) => {
          const { edge } = placed.connection;
          return (
            <button
              className={`api-edge-label ${isCandidateRelationEdge(edge) ? "candidate" : "confirmed"}${selectedEdgeId === edge.id ? " selected" : ""}`}
              style={labelStyle}
              type="button"
              data-edge-id={edge.id}
              title={edge.evidence[0]?.text ?? relationEdgeShortLabel(edge)}
              aria-label={`${placed.connection.node.title} ${relationEdgeShortLabel(edge)} 근거 보기`}
              onClick={() => onSelectEdge(edge)}
              key={`label-${edge.id}`}
            >
              {relationEdgeShortLabel(edge)}
            </button>
          );
        })}

        <CallGraphNode
          node={model.focus}
          focus
          selected={selectedNodeId === model.focus.id}
          style={{ left: layout.focus.x, top: layout.focus.y }}
          onSelect={() => onSelectNode(model.focus)}
        />
        {[...layout.callers, ...layout.callees, ...layout.dataTargets].map(({ connection, x, y }) => (
          <CallGraphNode
            node={connection.node}
            selected={selectedNodeId === connection.node.id}
            style={{ left: x, top: y }}
            onSelect={() => onSelectNode(connection.node)}
            key={`${connection.edge.id}-${connection.node.id}`}
          />
        ))}
      </div>
      {model.hiddenCallers + model.hiddenCallees + model.hiddenDataTargets > 0 ? (
        <p className="code-call-hidden">
          표시 상한을 넘은 연결 {model.hiddenCallers + model.hiddenCallees + model.hiddenDataTargets}개는 아래 목록에서 확인할 수 있습니다.
        </p>
      ) : null}
    </section>
  );
}

function CallGraphNode({
  node,
  focus = false,
  selected,
  style,
  onSelect,
}: {
  node: VisualNode;
  focus?: boolean;
  selected: boolean;
  style: CSSProperties;
  onSelect: () => void;
}) {
  const meta = relationLaneMeta(relationNodeLane(node));
  const NodeIcon = meta.icon;
  const location = relationSourceLabel(node.location) ?? node.subtitle ?? "위치 정보 없음";
  return (
    <button
      className={`api-diagram-node ${meta.tone}${focus ? " code-call-focus" : ""}${selected ? " selected" : ""}`}
      style={style}
      type="button"
      data-node-id={node.id}
      aria-pressed={selected}
      aria-label={`${meta.label} ${node.title} 선택`}
      onClick={onSelect}
    >
      <span className="api-node-kind">
        <NodeIcon size={14} />
        {meta.label}
      </span>
      <strong>{node.title}</strong>
      <small title={location}>{location}</small>
    </button>
  );
}
