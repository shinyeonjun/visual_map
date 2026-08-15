import type { CSSProperties } from 'react'
import type { DomainFeature } from '../domain'
import { trustLabel } from '../domain'
import { featureKindLabel, featureSummaryText } from '../lib/feature-presentation'

export function FeatureCard({
  feature,
  color,
  x,
  y,
  selected,
  onSelect,
  onOpen,
}: {
  feature: DomainFeature
  color: string
  x: number
  y: number
  selected: boolean
  onSelect: () => void
  onOpen: () => void
}) {
  const style = { left: x, top: y, '--domain-color': color } as CSSProperties
  const flowCount = feature.flows.length
  return (
    <button
      type="button"
      className={`domain-card feature-card ${selected ? 'selected' : ''}`}
      style={style}
      onClick={() => {
        onSelect()
        onOpen()
      }}
    >
      <span className="card-index">{featureKindLabel(feature.kind)}</span>
      <span className="domain-orb" />
      <strong>{feature.name}</strong>
      <p>{featureSummaryText(feature)}</p>
      <div className="domain-card-meta">
        <span><b>{feature.entrypoints}</b> 진입</span>
        <span><b>{flowCount}</b> 실행길</span>
      </div>
      <div className="card-foot">
        <span className={`confidence-dot ${feature.status}`} />
        {trustLabel(feature.status)}
        <span
          className="card-arrow"
          onClick={(event) => {
            event.stopPropagation()
            onOpen()
          }}
        >
          ↗
        </span>
      </div>
    </button>
  )
}
