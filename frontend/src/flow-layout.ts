import type { DomainFlow, DomainFlowEdge, DomainFlowNode } from './domain'
import { FLOW_TRACK } from './map-layout'

export const FLOW_BODY_PADDING_Y = 24
export const FLOW_LOOP_EXTRA = 36
export const MAX_FLOW_NODES = 24
const FLOW_ENTRY_SPACE = FLOW_TRACK.terminalHeight + 28
const FLOW_EXIT_SPACE = FLOW_TRACK.terminalHeight + 28

function rowGapForDepth(rowCount: number): number {
  if (rowCount <= 3) return 52
  if (rowCount <= 6) return 40
  if (rowCount <= 12) return 32
  return 24
}

function laneGapForWidth(laneCount: number): number {
  if (laneCount <= 2) return 36
  if (laneCount <= 3) return 20
  return 12
}

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
  graphCenterX: number
  entrySpace: number
  exitSpace: number
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

function edgeKey(edge: DomainFlowEdge): string {
  return `${edge.sourceNodeId}->${edge.targetNodeId}:${edge.kind}`
}

function selectVisibleNodes(flow: DomainFlow): DomainFlowNode[] {
  if (flow.nodes.length <= MAX_FLOW_NODES) return flow.nodes

  const byId = new Map(flow.nodes.map((node) => [node.id, node]))
  const successors = new Map<string, string[]>()
  for (const edge of flow.edges) {
    if (isBackEdge(edge.kind)) continue
    const next = successors.get(edge.sourceNodeId) ?? []
    next.push(edge.targetNodeId)
    successors.set(edge.sourceNodeId, next)
  }

  const ordered: DomainFlowNode[] = []
  const seen = new Set<string>()
  const queue = [flow.entryNodeId]
  while (queue.length > 0 && ordered.length < MAX_FLOW_NODES) {
    const id = queue.shift()!
    if (seen.has(id)) continue
    seen.add(id)
    const node = byId.get(id)
    if (node) ordered.push(node)
    for (const nextId of successors.get(id) ?? []) {
      if (!seen.has(nextId)) queue.push(nextId)
    }
  }

  for (const node of flow.nodes) {
    if (ordered.length >= MAX_FLOW_NODES) break
    if (seen.has(node.id)) continue
    seen.add(node.id)
    ordered.push(node)
  }

  return ordered
}

function forwardEdgesForLayout(
  visibleIds: Set<string>,
  edges: DomainFlowEdge[],
  entryNodeId: string,
): DomainFlowEdge[] {
  const candidates = edges.filter(
    (edge) =>
      !isBackEdge(edge.kind)
      && visibleIds.has(edge.targetNodeId)
      && (visibleIds.has(edge.sourceNodeId) || edge.sourceNodeId === entryNodeId),
  )

  const successors = new Map<string, DomainFlowEdge[]>()
  for (const edge of candidates) {
    const next = successors.get(edge.sourceNodeId) ?? []
    next.push(edge)
    successors.set(edge.sourceNodeId, next)
  }

  const UNVISITED = 0
  const VISITING = 1
  const VISITED = 2
  const state = new Map<string, number>()
  const cyclic = new Set<string>()

  function walk(id: string) {
    state.set(id, VISITING)
    for (const edge of successors.get(id) ?? []) {
      const targetState = state.get(edge.targetNodeId) ?? UNVISITED
      if (targetState === VISITING) {
        cyclic.add(edgeKey(edge))
        continue
      }
      if (targetState === UNVISITED) walk(edge.targetNodeId)
    }
    state.set(id, VISITED)
  }

  walk(entryNodeId)
  for (const id of visibleIds) {
    if ((state.get(id) ?? UNVISITED) === UNVISITED) walk(id)
  }

  return candidates.filter((edge) => !cyclic.has(edgeKey(edge)))
}

function compactRanks(values: Map<string, number>): Map<string, number> {
  const unique = [...new Set(values.values())].sort((left, right) => left - right)
  const rank = new Map(unique.map((value, index) => [value, index]))
  return new Map([...values].map(([id, value]) => [id, rank.get(value) ?? 0]))
}

const ENTRY_COLUMN = -1

type FlowPredRef = { sourceId: string; edge: DomainFlowEdge }

function predecessorColumn(
  sourceId: string,
  entryNodeId: string,
  columns: Map<string, number>,
): number {
  if (sourceId === entryNodeId) return ENTRY_COLUMN
  return columns.get(sourceId) ?? ENTRY_COLUMN
}

function assignNodeColumns(
  visibleNodes: DomainFlowNode[],
  entryNodeId: string,
  predecessors: Map<string, FlowPredRef[]>,
): Map<string, number> {
  const columns = new Map<string, number>()
  for (const node of visibleNodes) {
    columns.set(node.id, 0)
  }

  for (let pass = 0; pass < visibleNodes.length; pass++) {
    let changed = false
    for (const node of visibleNodes) {
      const incoming = predecessors.get(node.id) ?? []
      const nextColumn = incoming.length === 0
        ? 0
        : Math.max(
          ...incoming.map(({ sourceId }) => predecessorColumn(sourceId, entryNodeId, columns) + 1),
        )

      if ((columns.get(node.id) ?? 0) !== nextColumn) {
        columns.set(node.id, nextColumn)
        changed = true
      }
    }
    if (!changed) break
  }

  return columns
}

function assignNodeLanes(
  visibleNodes: DomainFlowNode[],
  entryNodeId: string,
  predecessors: Map<string, FlowPredRef[]>,
): Map<string, number> {
  const lanes = new Map<string, number>()
  for (const node of visibleNodes) {
    lanes.set(node.id, 0)
  }

  for (let pass = 0; pass < visibleNodes.length; pass++) {
    let changed = false
    for (const node of visibleNodes) {
      const incoming = predecessors.get(node.id) ?? []
      if (incoming.length === 0) continue

      const nextLane = incoming.length > 1
        ? 0
        : (() => {
          const { sourceId, edge } = incoming[0]
          const sourceLane = sourceId === entryNodeId ? 0 : (lanes.get(sourceId) ?? 0)
          return sourceLane + laneOffsetForEdge(edge.kind)
        })()

      if ((lanes.get(node.id) ?? 0) !== nextLane) {
        lanes.set(node.id, nextLane)
        changed = true
      }
    }
    if (!changed) break
  }

  return lanes
}

export function layoutFlowGraph(flow: DomainFlow): FlowGraphLayout {
  const visibleNodes = selectVisibleNodes(flow)
  const visibleIds = new Set(visibleNodes.map((node) => node.id))
  const edges = flow.edges.filter(
    (edge) =>
      visibleIds.has(edge.targetNodeId)
      && (visibleIds.has(edge.sourceNodeId) || edge.sourceNodeId === flow.entryNodeId),
  )
  const layoutEdges = forwardEdgesForLayout(visibleIds, edges, flow.entryNodeId)

  const predecessors = new Map<string, FlowPredRef[]>()
  for (const node of visibleNodes) {
    predecessors.set(node.id, [])
  }

  for (const edge of layoutEdges) {
    predecessors.get(edge.targetNodeId)?.push({ sourceId: edge.sourceNodeId, edge })
  }

  const columns = compactRanks(assignNodeColumns(visibleNodes, flow.entryNodeId, predecessors))
  const lanes = compactRanks(assignNodeLanes(visibleNodes, flow.entryNodeId, predecessors))

  const laneValues = [...lanes.values()]
  const laneMin = laneValues.length > 0 ? Math.min(...laneValues) : 0
  const laneMax = laneValues.length > 0 ? Math.max(...laneValues) : 0
  const laneCount = Math.max(1, laneMax - laneMin + 1)
  const maxColumn = Math.max(0, ...columns.values())
  const rowCount = maxColumn + 1
  const rowHeight = FLOW_TRACK.stepHeight + rowGapForDepth(rowCount)
  const laneWidth = FLOW_TRACK.stepWidth + laneGapForWidth(laneCount) * 2
  const entrySpace = rowCount <= 4 ? FLOW_TRACK.terminalHeight + 40 : FLOW_ENTRY_SPACE
  const exitSpace = rowCount <= 4 ? FLOW_TRACK.terminalHeight + 40 : FLOW_EXIT_SPACE

  const graphContentWidth = laneCount * laneWidth
  const graphCenterX = FLOW_TRACK.paddingX + graphContentWidth / 2

  const nodes: FlowNodeLayout[] = visibleNodes.map((node) => {
    const column = columns.get(node.id) ?? 0
    const lane = lanes.get(node.id) ?? 0
    const laneIndex = lane - laneMin
    const laneCenterOffset = (laneIndex - (laneCount - 1) / 2) * laneWidth
    return {
      node,
      column,
      lane,
      x: graphCenterX + laneCenterOffset - FLOW_TRACK.stepWidth / 2,
      y: FLOW_TRACK.paddingY + entrySpace + column * rowHeight,
    }
  })

  const hasLoopBack = edges.some((edge) => isBackEdge(edge.kind))
  const nodeBottom = nodes.reduce(
    (max, item) => Math.max(max, item.y + FLOW_TRACK.stepHeight),
    FLOW_TRACK.paddingY + entrySpace + FLOW_TRACK.stepHeight,
  )

  const bodyHeight = nodeBottom + exitSpace + (hasLoopBack ? FLOW_LOOP_EXTRA : 0)
  const totalWidth = FLOW_TRACK.paddingX * 2 + graphContentWidth

  return {
    nodes,
    edges,
    width: Math.max(totalWidth, 420),
    bodyHeight,
    height: bodyHeight,
    maxColumn,
    laneCount,
    graphCenterX,
    entrySpace,
    exitSpace,
  }
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
      width: 800,
      height: 600,
    }
  }

  const maxWidth = Math.max(...layouts.map((layout) => layout.width))
  const maxHeight = Math.max(...layouts.map((layout) => layout.height))

  return {
    width: maxWidth,
    height: maxHeight,
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
