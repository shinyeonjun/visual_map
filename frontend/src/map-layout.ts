const ORIGIN_X = 70
const ORIGIN_Y = 70
const GAP_X = 24
const GAP_Y = 22
const MIN_WIDTH = 1040
const MIN_HEIGHT = 700

export const DOMAIN_CARD = { width: 204, height: 164 }
export const FEATURE_CARD = { width: 236, height: 148 }

export const FLOW_TRACK = {
  paddingX: 52,
  paddingY: 58,
  header: 48,
  body: 96,
  laneGap: 24,
  stepWidth: 184,
  stepHeight: 76,
  stepGap: 56,
  startWidth: 40,
}

export const MAX_FLOW_STEPS = 24

export function gridPosition(
  index: number,
  columns: number,
  cardWidth: number,
  cardHeight: number,
) {
  const column = index % columns
  const row = Math.floor(index / columns)
  return {
    x: ORIGIN_X + column * (cardWidth + GAP_X),
    y: ORIGIN_Y + row * (cardHeight + GAP_Y),
  }
}

export function gridCanvasSize(count: number, columns: number, cardWidth: number, cardHeight: number) {
  const rows = Math.max(1, Math.ceil(Math.max(count, 1) / columns))
  return {
    width: Math.max(MIN_WIDTH, ORIGIN_X * 2 + columns * cardWidth + (columns - 1) * GAP_X),
    height: Math.max(MIN_HEIGHT, ORIGIN_Y * 2 + rows * cardHeight + (rows - 1) * GAP_Y),
  }
}

function laneBlockHeight() {
  return FLOW_TRACK.header + FLOW_TRACK.body
}

export function flowCanvasSize(flowCount: number, maxSteps: number) {
  const lanes = Math.max(flowCount, 1)
  const steps = Math.max(maxSteps, 1)
  const width = FLOW_TRACK.paddingX * 2
    + FLOW_TRACK.startWidth
    + steps * (FLOW_TRACK.stepWidth + FLOW_TRACK.stepGap)
  const height = FLOW_TRACK.paddingY * 2
    + lanes * laneBlockHeight()
    + Math.max(0, lanes - 1) * FLOW_TRACK.laneGap
  return {
    width: Math.max(MIN_WIDTH, width),
    height: Math.max(MIN_HEIGHT, height),
  }
}

export function flowLaneTop(index: number) {
  return FLOW_TRACK.paddingY + index * (laneBlockHeight() + FLOW_TRACK.laneGap)
}

export function flowStepLeftInLane(stepIndex: number) {
  return FLOW_TRACK.paddingX + FLOW_TRACK.startWidth + stepIndex * (FLOW_TRACK.stepWidth + FLOW_TRACK.stepGap)
}

export function flowStepTopInLane() {
  return FLOW_TRACK.header + (FLOW_TRACK.body - FLOW_TRACK.stepHeight) / 2
}

export function shortUnitName(value: string) {
  const parts = value.split('::').filter(Boolean)
  if (parts.length <= 2) return value
  return parts.slice(-2).join('::')
}
