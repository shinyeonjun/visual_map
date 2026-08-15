import { useEffect, useMemo, useState } from 'react'
import type { CSSProperties } from 'react'
import type { DomainFlow, DomainFlowEdge } from '../domain'
import { trustLabel } from '../domain'
import {
  edgeKindLabel,
  edgeKindTone,
  isBackEdge,
  isExceptionEdge,
  isTerminalKind,
  stepKindLabel,
  stepKindSymbol,
  stepKindTone,
} from '../flow-presentation'
import {
  flowMapCanvasSize,
  flowTrackTops,
  layoutFlowGraph,
  layoutNodeById,
  terminalNodes,
  type FlowGraphLayout,
  type FlowNodeLayout,
} from '../flow-layout'
import { FLOW_TRACK, shortUnitName } from '../map-layout'

function connectorPath(fromX: number, fromY: number, toX: number, toY: number): string {
  if (Math.abs(fromY - toY) < 0.5) return `M${fromX} ${fromY} L${toX} ${toY}`
  const bend = Math.min(28, Math.max(12, (toX - fromX) / 2))
  return `M${fromX} ${fromY} L${fromX + bend} ${fromY} C${toX - bend} ${fromY} ${fromX + bend} ${toY} ${toX - bend} ${toY} L${toX} ${toY}`
}

function loopBackPath(fromX: number, fromY: number, toX: number, toY: number): string {
  const drop = Math.max(24, Math.abs(fromX - toX) / 4)
  const bottom = Math.max(fromY, toY) + drop
  return `M${fromX} ${fromY} C${fromX} ${bottom} ${toX} ${bottom} ${toX} ${toY}`
}

function edgePath(
  edge: DomainFlowEdge,
  source: FlowNodeLayout | undefined,
  target: FlowNodeLayout,
  entryX: number,
  entryY: number,
): string {
  const fromX = source ? source.x + FLOW_TRACK.stepWidth : entryX
  const fromY = source ? source.y + FLOW_TRACK.stepHeight / 2 : entryY
  const toX = target.x
  const toY = target.y + FLOW_TRACK.stepHeight / 2

  if (isBackEdge(edge.kind)) {
    return loopBackPath(fromX, fromY, toX, toY)
  }
  return connectorPath(fromX, fromY, toX, toY)
}

function FlowNodeButton({
  layout,
  displayIndex,
  color,
  selected,
  dimmed,
  onSelect,
}: {
  layout: FlowNodeLayout
  displayIndex: number
  color: string
  selected: boolean
  dimmed: boolean
  onSelect: () => void
}) {
  const { node } = layout
  const tone = stepKindTone(node.kind)
  const style = {
    left: layout.x,
    top: layout.y,
    width: FLOW_TRACK.stepWidth,
    height: FLOW_TRACK.stepHeight,
    '--domain-color': color,
  } as CSSProperties

  return (
    <button
      type="button"
      className={`flow-step-node tone-${tone}${selected ? ' selected' : ''}${dimmed ? ' dimmed' : ''}${node.status === 'candidate' ? ' candidate' : ''}`}
      style={style}
      onClick={(event) => {
        event.stopPropagation()
        onSelect()
      }}
      aria-label={`${displayIndex + 1}단계 ${node.label}, ${stepKindLabel(node.kind)}`}
    >
      <span className="flow-step-index">{String(displayIndex + 1).padStart(2, '0')}</span>
      <span className="flow-step-kind" aria-hidden="true">{stepKindSymbol(node.kind)}</span>
      <span className="flow-step-copy">
        <strong title={node.label}>{node.label}</strong>
        <small>{stepKindLabel(node.kind)}</small>
      </span>
      <span className={`flow-step-trust ${node.status}`} title={trustLabel(node.status)} />
    </button>
  )
}

function FlowEdgeLabel({
  edge,
  source,
  target,
  entryX,
  entryY,
}: {
  edge: DomainFlowEdge
  source?: FlowNodeLayout
  target: FlowNodeLayout
  entryX: number
  entryY: number
}) {
  const label = edgeKindLabel(edge.kind, edge.label)
  if (!label || edge.kind === 'sequential' || edge.kind === 'loopBody') return null

  const fromX = source ? source.x + FLOW_TRACK.stepWidth : entryX
  const fromY = source ? source.y + FLOW_TRACK.stepHeight / 2 : entryY
  const toX = target.x
  const toY = target.y + FLOW_TRACK.stepHeight / 2
  const x = (fromX + toX) / 2
  const y = (fromY + toY) / 2 - 10

  return (
    <text
      className={`flow-edge-label tone-${edgeKindTone(edge.kind)}`}
      x={x}
      y={y}
    >
      {label}
    </text>
  )
}

function FlowTrack({
  flow,
  layout,
  index,
  top,
  color,
  selected,
  activeNodeId,
  onSelectFlow,
  onSelectNode,
}: {
  flow: DomainFlow
  layout: FlowGraphLayout
  index: number
  top: number
  color: string
  selected: boolean
  activeNodeId: string | null
  onSelectFlow: () => void
  onSelectNode: (nodeId: string) => void
}) {
  const hidden = flow.nodes.length - layout.nodes.length
  const bodyHeight = layout.bodyHeight
  const trackStyle = {
    top,
    height: layout.height,
    '--domain-color': color,
  } as CSSProperties
  const positions = layoutNodeById(layout)
  const entryX = FLOW_TRACK.paddingX + FLOW_TRACK.startWidth - 8
  const entryY = bodyHeight / 2
  const displayIndexById = new Map(layout.nodes.map((item, nodeIndex) => [item.node.id, nodeIndex]))
  const terminals = terminalNodes(layout)

  return (
    <article
      className={`flow-track${selected ? ' selected' : ''}${!selected && activeNodeId ? ' muted' : ''}`}
      style={trackStyle}
      onClick={onSelectFlow}
    >
      <header className="flow-track-head">
        <span className="flow-track-index">{String(index + 1).padStart(2, '0')}</span>
        <div className="flow-track-title">
          <strong title={flow.owner}>{shortUnitName(flow.owner)}</strong>
          <small>{layout.nodes.length}노드 · {trustLabel(flow.status)}</small>
        </div>
        {hidden > 0 && <span className="flow-track-more">+{hidden}</span>}
      </header>

      <div className="flow-track-body" style={{ height: bodyHeight }}>
        <svg className="flow-track-lines" width={layout.width} height={bodyHeight} aria-hidden="true">
          {layout.nodes[0] && (
            <path
              className="flow-line flow-line-start"
              d={connectorPath(entryX, entryY, layout.nodes[0].x, layout.nodes[0].y + FLOW_TRACK.stepHeight / 2)}
            />
          )}

          {layout.edges.map((edge) => {
            const target = positions.get(edge.targetNodeId)
            if (!target) return null
            const source = positions.get(edge.sourceNodeId)
            const dimmed = Boolean(activeNodeId && target.column > (positions.get(activeNodeId)?.column ?? -1))
            return (
              <g key={`${edge.sourceNodeId}-${edge.targetNodeId}-${edge.kind}`}>
                <path
                  className={[
                    'flow-line',
                    `tone-${edgeKindTone(edge.kind)}`,
                    isBackEdge(edge.kind) ? 'back' : '',
                    isExceptionEdge(edge.kind) ? 'exception' : '',
                    edge.status === 'candidate' ? 'candidate' : '',
                    dimmed ? 'dimmed' : '',
                  ].filter(Boolean).join(' ')}
                  d={edgePath(edge, source, target, entryX, entryY)}
                  markerEnd={isBackEdge(edge.kind) ? 'url(#flow-loop-arrow)' : 'url(#flow-arrow)'}
                />
                <FlowEdgeLabel edge={edge} source={source} target={target} entryX={entryX} entryY={entryY} />
              </g>
            )
          })}
        </svg>

        <span className="flow-start-badge" style={{ left: FLOW_TRACK.paddingX, top: (bodyHeight - 28) / 2 }}>
          진입
        </span>

        {layout.nodes.map((item) => (
          <FlowNodeButton
            key={item.node.id}
            layout={item}
            displayIndex={displayIndexById.get(item.node.id) ?? 0}
            color={color}
            selected={selected && activeNodeId === item.node.id}
            dimmed={Boolean(activeNodeId && item.column > (positions.get(activeNodeId)?.column ?? -1))}
            onSelect={() => onSelectNode(item.node.id)}
          />
        ))}

        {terminals.filter((item) => isTerminalKind(item.node.kind)).map((item) => (
          <span
            key={`${flow.id}-${item.node.id}-terminal`}
            className={`flow-end-badge tone-${stepKindTone(item.node.kind)}`}
            style={{
              left: item.x + FLOW_TRACK.stepWidth + 14,
              top: item.y + (FLOW_TRACK.stepHeight - 28) / 2,
            }}
          >
            {stepKindLabel(item.node.kind)}
          </span>
        ))}
      </div>
    </article>
  )
}

export function FlowMap({
  flows,
  color,
}: {
  flows: DomainFlow[]
  color: string
}) {
  const [selectedFlowId, setSelectedFlowId] = useState<string | null>(flows[0]?.id ?? null)
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null)
  const layouts = useMemo(() => flows.map((flow) => layoutFlowGraph(flow)), [flows])
  const trackTops = useMemo(() => flowTrackTops(layouts), [layouts])
  const canvas = flowMapCanvasSize(layouts)

  useEffect(() => {
    setSelectedFlowId(flows[0]?.id ?? null)
    setActiveNodeId(null)
  }, [flows])

  return (
    <div className="flow-map" style={{ width: canvas.width, height: canvas.height }}>
      <svg className="flow-map-defs" aria-hidden="true">
        <defs>
          <marker id="flow-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-head" />
          </marker>
          <marker id="flow-loop-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-head loop" />
          </marker>
        </defs>
      </svg>

      {flows.map((flow, index) => (
        <FlowTrack
          key={flow.id}
          flow={flow}
          layout={layouts[index] ?? layoutFlowGraph(flow)}
          index={index}
          top={trackTops[index] ?? FLOW_TRACK.paddingY}
          color={color}
          selected={selectedFlowId === flow.id}
          activeNodeId={selectedFlowId === flow.id ? activeNodeId : null}
          onSelectFlow={() => {
            setSelectedFlowId(flow.id)
            setActiveNodeId(null)
          }}
          onSelectNode={(nodeId) => {
            setSelectedFlowId(flow.id)
            setActiveNodeId(nodeId)
          }}
        />
      ))}
    </div>
  )
}
