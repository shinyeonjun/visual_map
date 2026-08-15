import type { DomainFlow, DomainFlowEdge, DomainFlowNode } from './domain'
import { FLOW_TRACK } from './map-layout'

export const FLOW_BODY_PADDING_Y = 24
export const FLOW_LOOP_EXTRA = 40
export const FLOW_LANE_GAP = 16
export const FLOW_LANE_HEIGHT = FLOW_TRACK.stepHeight + FLOW_LANE_GAP
export const MAX_FLOW_NODES = 24

export type FlowNodeLayout = {
  node: DomainFlowNode
  column: number
  lane: number
  x: number
  y: number
}

export type FlowGraphLayout = {
  nodes: FlowNodeLayout[]
  edges: DomainFlowEdge[]
  width: number
  height: number
  bodyHeight: number
  maxColumn: number
  laneCount: number
}

function laneOffsetForEdge(kind: string): number {
  switch (kind) {
    case 'trueBranch':
      return -1
    case 'falseBranch':
      return 1
    case 'exception':
      return 2
    default:
      return 0
  }
}

function isBackEdge(kind: string): boolean {
  return kind === 'loopBack'
}

export function layoutFlowGraph(flow: DomainFlow): FlowGraphLayout {
  const visibleNodes = flow.nodes.slice(0, MAX_FLOW_NODES)
  const visibleIds = new Set(visibleNodes.map((node) => node.id))
  const edges = flow.edges.filter(
    (edge) =>
      visibleIds.has(edge.targetNodeId)
      && (visibleIds.has(edge.sourceNodeId) || edge.sourceNodeId === flow.entryNodeId),
  )

  const predecessors = new Map<string, Array<{ sourceId: string; edge: DomainFlowEdge }>>()
  const successors = new Map<string, DomainFlowEdge[]>()
  for (const node of visibleNodes) {
    predecessors.set(node.id, [])
    successors.set(node.id, [])
  }

  for (const edge of edges) {
    if (!isBackEdge(edge.kind)) {
      predecessors.get(edge.targetNodeId)?.push({ sourceId: edge.sourceNodeId, edge })
      if (visibleIds.has(edge.sourceNodeId)) {
        successors.get(edge.sourceNodeId)?.push(edge)
      }
    }
  }

  const columns = new Map<string, number>()
  const lanes = new Map<string, number>()
  const queue: Array<{ id: string; column: number; lane: number }> = []

  for (const node of visibleNodes) {
    const incoming = predecessors.get(node.id) ?? []
    if (incoming.length === 0) {
      queue.push({ id: node.id, column: 0, lane: 0 })
    }
  }

  if (queue.length === 0 && visibleNodes[0]) {
    queue.push({ id: visibleNodes[0].id, column: 0, lane: 0 })
  }

  const visited = new Set<string>()
  while (queue.length > 0) {
    const current = queue.shift()!
    const previousColumn = columns.get(current.id)
    const previousLane = lanes.get(current.id)
    const nextColumn = previousColumn === undefined
      ? current.column
      : Math.max(previousColumn, current.column)
    const nextLane = previousLane === undefined
      ? current.lane
      : Math.abs(previousLane) > Math.abs(current.lane) ? previousLane : current.lane

    columns.set(current.id, nextColumn)
    lanes.set(current.id, nextLane)
    if (visited.has(current.id)) {
      continue
    }
    visited.add(current.id)

    const outgoing = successors.get(current.id) ?? []
    for (const edge of outgoing) {
      const childColumn = nextColumn + 1
      const childLane = nextLane + laneOffsetForEdge(edge.kind)
      queue.push({ id: edge.targetNodeId, column: childColumn, lane: childLane })
    }
  }

  let fallbackColumn = Math.max(0, ...columns.values()) + 1
  for (const node of visibleNodes) {
    if (!columns.has(node.id)) {
      columns.set(node.id, fallbackColumn)
      lanes.set(node.id, 0)
      fallbackColumn += 1
    }
  }

  const laneValues = [...lanes.values()]
  const laneMin = laneValues.length > 0 ? Math.min(...laneValues) : 0
  const laneMax = laneValues.length > 0 ? Math.max(...laneValues) : 0
  const laneCount = laneMax - laneMin + 1
  const maxColumn = Math.max(0, ...columns.values())
  const columnWidth = FLOW_TRACK.stepWidth + FLOW_TRACK.stepGap
  const laneSpan = laneCount * FLOW_LANE_HEIGHT

  const nodes: FlowNodeLayout[] = visibleNodes.map((node) => {
    const column = columns.get(node.id) ?? 0
    const lane = lanes.get(node.id) ?? 0
    const laneIndex = lane - laneMin
    return {
      node,
      column,
      lane,
      x: FLOW_TRACK.paddingX + FLOW_TRACK.startWidth + column * columnWidth,
      y: FLOW_BODY_PADDING_Y + laneIndex * FLOW_LANE_HEIGHT,
    }
  })

  const hasLoopBack = edges.some((edge) => isBackEdge(edge.kind))
  const nodeBottom = nodes.reduce(
    (max, item) => Math.max(max, item.y + FLOW_TRACK.stepHeight),
    FLOW_BODY_PADDING_Y + FLOW_TRACK.stepHeight,
  )
  const endBadgeRight = nodes.reduce(
    (max, item) => Math.max(max, item.x + FLOW_TRACK.stepWidth + 80),
    FLOW_TRACK.paddingX + FLOW_TRACK.startWidth + FLOW_TRACK.stepWidth,
  )

  const bodyHeight = Math.max(
    FLOW_TRACK.body,
    laneSpan + FLOW_BODY_PADDING_Y * 2,
    nodeBottom + FLOW_BODY_PADDING_Y + (hasLoopBack ? FLOW_LOOP_EXTRA : 0),
  )

  const width = Math.max(
    FLOW_TRACK.paddingX * 2 + FLOW_TRACK.startWidth + (maxColumn + 1) * columnWidth + FLOW_TRACK.stepWidth,
    endBadgeRight + FLOW_TRACK.paddingX,
  )

  return {
    nodes,
    edges,
    width,
    bodyHeight,
    height: FLOW_TRACK.header + bodyHeight,
    maxColumn,
    laneCount,
  }
}

export function flowTrackTops(layouts: FlowGraphLayout[]): number[] {
  let top = FLOW_TRACK.paddingY
  return layouts.map((layout) => {
    const current = top
    top += layout.height + FLOW_TRACK.laneGap
    return current
  })
}

export function flowTrackCanvasSize(layout: FlowGraphLayout) {
  return {
    width: layout.width,
    height: layout.height,
  }
}

export function flowMapCanvasSize(layouts: FlowGraphLayout[]) {
  if (layouts.length === 0) {
    return {
      width: FLOW_TRACK.paddingX * 2 + 400,
      height: FLOW_TRACK.paddingY * 2 + FLOW_TRACK.header + FLOW_TRACK.body,
    }
  }

  const tops = flowTrackTops(layouts)
  const lastLayout = layouts[layouts.length - 1]
  const lastTop = tops[tops.length - 1] ?? FLOW_TRACK.paddingY
  const maxWidth = Math.max(...layouts.map((layout) => layout.width))

  return {
    width: maxWidth,
    height: lastTop + lastLayout.height + FLOW_TRACK.paddingY,
  }
}

export function layoutNodeById(layout: FlowGraphLayout): Map<string, FlowNodeLayout> {
  return new Map(layout.nodes.map((item) => [item.node.id, item]))
}

export function terminalNodes(layout: FlowGraphLayout): FlowNodeLayout[] {
  const outgoing = new Set(layout.edges.filter((edge) => !isBackEdge(edge.kind)).map((edge) => edge.sourceNodeId))
  return layout.nodes.filter((item) => !outgoing.has(item.node.id))
}

export function summarizeFlowNodes(flow: DomainFlow): DomainFlowNode[] {
  return layoutFlowGraph(flow).nodes.map((item) => item.node)
}

export function flowLayoutSize(flow: DomainFlow) {
  return flowTrackCanvasSize(layoutFlowGraph(flow))
}
