import type { CSSProperties } from 'react'
import type { DomainNode } from '../domain'
import { trustLabel } from '../domain'

export function DomainCard({
  domain,
  index,
  x,
  y,
  selected,
  onSelect,
  onOpen,
}: {
  domain: DomainNode
  index: number
  x: number
  y: number
  selected: boolean
  onSelect: () => void
  onOpen: () => void
}) {
  const style = { left: x, top: y, '--domain-color': domain.color } as CSSProperties
  return (
    <button
      type="button"
      className={`domain-card ${selected ? 'selected' : ''}`}
      style={style}
      onClick={onSelect}
      onDoubleClick={(event) => {
        event.preventDefault()
        onOpen()
      }}
    >
      <span className="card-index">{String(index + 1).padStart(2, '0')}</span>
      <span className="domain-orb" />
      <strong>{domain.name}</strong>
      <p>{domain.summary}</p>
      <div className="domain-card-meta">
        <span><b>{domain.features}</b> 기능</span>
        <span><b>{domain.entrypoints}</b> 진입</span>
        <span><b>{domain.confidence}%</b> 신뢰</span>
      </div>
      <div className="card-foot">
        <span className={`confidence-dot ${domain.status}`} />
        {trustLabel(domain.status)}
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
