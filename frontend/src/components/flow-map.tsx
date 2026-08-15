import { useEffect, useMemo, useState } from 'react'
import type { CSSProperties } from 'react'
import type { DomainFlow, DomainFlowStep } from '../domain'
import { trustLabel } from '../domain'
import { isTerminalKind, stepKindLabel, stepKindSymbol, stepKindTone } from '../flow-presentation'
import {
  FLOW_TRACK,
  MAX_FLOW_STEPS,
  flowCanvasSize,
  flowLaneTop,
  flowStepLeftInLane,
  flowStepTopInLane,
  shortUnitName,
} from '../map-layout'

function connectorPath(fromX: number, fromY: number, toX: number, toY: number): string {
  if (Math.abs(fromY - toY) < 0.5) return `M${fromX} ${fromY} L${toX} ${toY}`
  const bend = Math.min(28, Math.max(12, (toX - fromX) / 2))
  return `M${fromX} ${fromY} L${fromX + bend} ${fromY} C${toX - bend} ${fromY} ${fromX + bend} ${toY} ${toX - bend} ${toY} L${toX} ${toY}`
}

function FlowStepNode({
  step,
  stepIndex,
  color,
  selected,
  dimmed,
  onSelect,
}: {
  step: DomainFlowStep
  stepIndex: number
  color: string
  selected: boolean
  dimmed: boolean
  onSelect: () => void
}) {
  const tone = stepKindTone(step.kind)
  const style = {
    left: flowStepLeftInLane(stepIndex),
    top: flowStepTopInLane(),
    width: FLOW_TRACK.stepWidth,
    height: FLOW_TRACK.stepHeight,
    '--domain-color': color,
  } as CSSProperties

  return (
    <button
      type="button"
      className={`flow-step-node tone-${tone}${selected ? ' selected' : ''}${dimmed ? ' dimmed' : ''}${step.status === 'candidate' ? ' candidate' : ''}`}
      style={style}
      onClick={(event) => {
        event.stopPropagation()
        onSelect()
      }}
      aria-label={`${stepIndex + 1}단계 ${step.label}, ${stepKindLabel(step.kind)}`}
    >
      <span className="flow-step-index">{String(stepIndex + 1).padStart(2, '0')}</span>
      <span className="flow-step-kind" aria-hidden="true">{stepKindSymbol(step.kind)}</span>
      <span className="flow-step-copy">
        <strong title={step.label}>{step.label}</strong>
        <small>{stepKindLabel(step.kind)}</small>
      </span>
      <span className={`flow-step-trust ${step.status}`} title={trustLabel(step.status)} />
    </button>
  )
}

function FlowTrack({
  flow,
  index,
  color,
  selected,
  activeStepIndex,
  onSelectFlow,
  onSelectStep,
}: {
  flow: DomainFlow
  index: number
  color: string
  selected: boolean
  activeStepIndex: number | null
  onSelectFlow: () => void
  onSelectStep: (stepIndex: number) => void
}) {
  const steps = flow.steps.slice(0, MAX_FLOW_STEPS)
  const hidden = flow.steps.length - steps.length
  const laneTop = flowLaneTop(index)
  const laneHeight = FLOW_TRACK.header + FLOW_TRACK.body
  const lineY = FLOW_TRACK.header + FLOW_TRACK.body / 2
  const startX = FLOW_TRACK.paddingX + FLOW_TRACK.startWidth - 8
  const lastStep = steps[steps.length - 1]
  const trackStyle = {
    top: laneTop,
    height: laneHeight,
    '--domain-color': color,
  } as CSSProperties
  const lineWidth = flowCanvasSize(1, Math.max(steps.length, 1)).width

  return (
    <article
      className={`flow-track${selected ? ' selected' : ''}${!selected && activeStepIndex !== null ? ' muted' : ''}`}
      style={trackStyle}
      onClick={onSelectFlow}
    >
      <header className="flow-track-head">
        <span className="flow-track-index">{String(index + 1).padStart(2, '0')}</span>
        <div className="flow-track-title">
          <strong title={flow.owner}>{shortUnitName(flow.owner)}</strong>
          <small>{steps.length}단계 · {trustLabel(flow.status)}</small>
        </div>
        {hidden > 0 && <span className="flow-track-more">+{hidden}</span>}
      </header>

      <div className="flow-track-body">
        <svg className="flow-track-lines" width={lineWidth} height={FLOW_TRACK.body} aria-hidden="true">
          <path
            className="flow-line flow-line-start"
            d={connectorPath(startX, lineY, flowStepLeftInLane(0), lineY)}
          />
          {steps.slice(0, -1).map((step, stepIndex) => {
            const fromX = flowStepLeftInLane(stepIndex) + FLOW_TRACK.stepWidth
            const toX = flowStepLeftInLane(stepIndex + 1)
            const dimmed = activeStepIndex !== null && stepIndex >= activeStepIndex
            return (
              <path
                key={`${flow.id}-${stepIndex}`}
                className={`flow-line${dimmed ? ' dimmed' : ''}${step.status === 'candidate' ? ' candidate' : ''}`}
                d={connectorPath(fromX, lineY, toX, lineY)}
                markerEnd="url(#flow-arrow)"
              />
            )
          })}
        </svg>

        <span className="flow-start-badge" style={{ left: FLOW_TRACK.paddingX, top: (FLOW_TRACK.body - 28) / 2 }}>
          진입
        </span>

        {steps.map((step, stepIndex) => (
          <FlowStepNode
            key={`${flow.id}-${stepIndex}`}
            step={step}
            stepIndex={stepIndex}
            color={color}
            selected={selected && activeStepIndex === stepIndex}
            dimmed={activeStepIndex !== null && stepIndex > activeStepIndex}
            onSelect={() => onSelectStep(stepIndex)}
          />
        ))}

        {lastStep && isTerminalKind(lastStep.kind) && (
          <span
            className={`flow-end-badge tone-${stepKindTone(lastStep.kind)}`}
            style={{
              left: flowStepLeftInLane(steps.length - 1) + FLOW_TRACK.stepWidth + 14,
              top: (FLOW_TRACK.body - 28) / 2,
            }}
          >
            {stepKindLabel(lastStep.kind)}
          </span>
        )}
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
  const [activeStepIndex, setActiveStepIndex] = useState<number | null>(null)

  useEffect(() => {
    setSelectedFlowId(flows[0]?.id ?? null)
    setActiveStepIndex(null)
  }, [flows])

  const maxSteps = useMemo(
    () => Math.max(1, ...flows.map((flow) => Math.min(flow.steps.length, MAX_FLOW_STEPS))),
    [flows],
  )
  const canvas = flowCanvasSize(flows.length, maxSteps)

  return (
    <div className="flow-map" style={{ width: canvas.width, height: canvas.height }}>
      <svg className="flow-map-defs" aria-hidden="true">
        <defs>
          <marker id="flow-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
            <path d="M0,0 L8,4 L0,8 Z" className="flow-arrow-head" />
          </marker>
        </defs>
      </svg>

      {flows.map((flow, index) => (
        <FlowTrack
          key={flow.id}
          flow={flow}
          index={index}
          color={color}
          selected={selectedFlowId === flow.id}
          activeStepIndex={selectedFlowId === flow.id ? activeStepIndex : null}
          onSelectFlow={() => {
            setSelectedFlowId(flow.id)
            setActiveStepIndex(null)
          }}
          onSelectStep={(stepIndex) => {
            setSelectedFlowId(flow.id)
            setActiveStepIndex(stepIndex)
          }}
        />
      ))}
    </div>
  )
}
