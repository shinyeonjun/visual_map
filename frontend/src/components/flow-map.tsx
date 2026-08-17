import { useMemo } from 'react'
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
  layoutFlowGraph,
  layoutNodeById,
  terminalNodes,
  type FlowNodeLayout,
} from '../flow-layout'
import { FLOW_TRACK, shortUnitName } from '../map-layout'

function verticalConnectorPath(fromX: number, fromY: number, toX: number, toY: number): string {
  if (Math.abs(fromX - toX) < 0.5) return `M${fromX} ${fromY} L${toX} ${toY}`
  const bend = Math.min(32, Math.max(14, (toY - fromY) / 2))
  return `M${fromX} ${fromY} L${fromX} ${fromY + bend} C${fromX} ${toY - bend} ${toX} ${fromY + bend} ${toX} ${toY - bend} L${toX} ${toY}`
}

function verticalLoopBackPath(fromX: number, fromY: number, toX: number, toY: number): string {
  const offset = Math.max(40, Math.abs(fromY - toY) / 4)
  const right = Math.max(fromX, toX) + offset
  return `M${fromX} ${fromY} C${right} ${fromY} ${right} ${toY} ${toX} ${toY}`
}

function edgePath(
  edge: DomainFlowEdge,
  source: FlowNodeLayout | undefined,
  target: FlowNodeLayout,
  entryX: number,
  entryY: number,
): string {
  const fromX = source ? source.x + FLOW_TRACK.stepWidth / 2 : entryX
  const fromY = source ? source.y + FLOW_TRACK.stepHeight : entryY
  const toX = target.x + FLOW_TRACK.stepWidth / 2
  const toY = target.y

  if (isBackEdge(edge.kind)) {
    return verticalLoopBackPath(fromX, fromY, toX, toY)
  }
  return verticalConnectorPath(fromX, fromY, toX, toY)
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
      <span className="flow-step-kind" aria-hidden="true">{stepKindSymbol(node.kind)}</span>
      <span className="flow-step-copy">
        <strong title={node.label}>{node.label}</strong>
        <small>{stepKindLabel(node.kind)}</small>
      </span>
      <span className="flow-step-right">
        <span className={`flow-step-trust ${node.status}`} title={trustLabel(node.status)} />
        <span className="flow-step-index">#{displayIndex + 1}</span>
      </span>
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

  const fromX = source ? source.x + FLOW_TRACK.stepWidth / 2 : entryX
  const fromY = source ? source.y + FLOW_TRACK.stepHeight : entryY
  const toX = target.x + FLOW_TRACK.stepWidth / 2
  const toY = target.y
  const x = (fromX + toX) / 2 + 12
  const y = (fromY + toY) / 2

  return (
    <text
      className={`flow-edge-label tone-${edgeKindTone(edge.kind)}`}
      x={x}
      y={y}
      textAnchor="start"
    >
      {label}
    </text>
  )
}

export function FlowListPanel({
  flows,
  selectedFlowId,
  color,
  onSelectFlow,
}: {
  flows: DomainFlow[]
  selectedFlowId: string | null
  color: string
  onSelectFlow: (id: string) => void
}) {
  return (
    <div className="flow-list-panel" style={{ '--domain-color': color } as CSSProperties}>
      <div className="flow-list-header">
        <span className="flow-list-count">실행 요소 {flows.length}개</span>
      </div>
      <div className="flow-list-items">
        {flows.map((flow, index) => (
          <button
            key={flow.id}
            type="button"
            className={`flow-list-item${selectedFlowId === flow.id ? ' selected' : ''}`}
            onClick={(event) => {
              event.stopPropagation()
              onSelectFlow(flow.id)
            }}
          >
            <span className={`flow-list-dot ${flow.status}`} />
            <span className="flow-list-text">
              <strong>{shortUnitName(flow.owner)}</strong>
              <small>{trustLabel(flow.status)} · {flow.nodes.length}노드</small>
            </span>
            <span className="flow-list-index">{String(index + 1).padStart(2, '0')}</span>
          </button>
        ))}
      </div>
    </div>
  )
}

export function FlowMap({
  flow,
  color,
  activeNodeId,
  onSelectNode,
}: {
  flow: DomainFlow
  color: string
  activeNodeId: string | null
  onSelectNode: (nodeId: string) => void
}) {
  const layout = useMemo(() => layoutFlowGraph(flow), [flow])
  const positions = layoutNodeById(layout)
  const entryX = layout.graphCenterX
  const entryY = FLOW_TRACK.paddingY + FLOW_TRACK.terminalHeight
  const displayIndexById = new Map(layout.nodes.map((item, nodeIndex) => [item.node.id, nodeIndex]))
  const terminals = terminalNodes(layout)

  const lastNodeBottom = layout.nodes.reduce(
    (max, item) => Math.max(max, item.y + FLOW_TRACK.stepHeight),
    entryY,
  )
  const exitY = lastNodeBottom + (layout.exitSpace - FLOW_TRACK.terminalHeight)
  const exitX = layout.graphCenterX

  return (
    <div className="flow-map" style={{ width: layout.width, height: layout.height }}>
      <svg
        className="flow-graph-lines"
        width={layout.width}
        height={layout.height}
        aria-hidden="true"
        style={{ '--domain-color': color } as CSSProperties}
      >
        <defs>
          <marker id="flow-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-head" />
          </marker>
          <marker id="flow-loop-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-head loop" />
          </marker>
        </defs>

        {layout.edges.map((edge) => {
          const target = positions.get(edge.targetNodeId)
          if (!target) return null
          const source = positions.get(edge.sourceNodeId)
          const fromEntry = edge.sourceNodeId === flow.entryNodeId
          const dimmed = Boolean(activeNodeId && target.column > (positions.get(activeNodeId)?.column ?? -1))
          return (
            <g key={`${edge.sourceNodeId}-${edge.targetNodeId}-${edge.kind}`}>
              <path
                className={[
                  'flow-line',
                  fromEntry ? 'flow-line-start' : '',
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

        {terminals.filter((item) => isTerminalKind(item.node.kind)).map((item) => (
          <line
            key={`terminal-line-${item.node.id}`}
            className="flow-line flow-line-exit"
            x1={item.x + FLOW_TRACK.stepWidth / 2}
            y1={item.y + FLOW_TRACK.stepHeight}
            x2={exitX}
            y2={exitY}
            markerEnd="url(#flow-arrow)"
          />
        ))}
      </svg>

      <span
        className="flow-terminal-badge flow-entry-badge"
        style={{
          left: entryX - FLOW_TRACK.terminalWidth / 2,
          top: FLOW_TRACK.paddingY,
        }}
      >
        Entry
      </span>

      {layout.nodes.map((item) => (
        <FlowNodeButton
          key={item.node.id}
          layout={item}
          displayIndex={displayIndexById.get(item.node.id) ?? 0}
          color={color}
          selected={activeNodeId === item.node.id}
          dimmed={Boolean(activeNodeId && activeNodeId !== item.node.id && item.column > (positions.get(activeNodeId)?.column ?? -1))}
          onSelect={() => onSelectNode(item.node.id)}
        />
      ))}

      {terminals.filter((item) => isTerminalKind(item.node.kind)).map((item) => (
        <span
          key={`exit-${item.node.id}`}
          className={`flow-terminal-badge flow-exit-badge tone-${stepKindTone(item.node.kind)}`}
          style={{
            left: exitX - FLOW_TRACK.terminalWidth / 2,
            top: exitY,
          }}
        >
          Exit
        </span>
      ))}

      {terminals.length === 0 && (
        <span
          className="flow-terminal-badge flow-exit-badge"
          style={{
            left: exitX - FLOW_TRACK.terminalWidth / 2,
            top: exitY,
          }}
        >
          Exit
        </span>
      )}
    </div>
  )
}
