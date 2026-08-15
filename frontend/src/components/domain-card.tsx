import type { CSSProperties } from 'react'
import type { DomainNode } from '../domain'
import { trustLabel } from '../domain'

export function DomainCard({ domain, selected, onSelect }: { domain: DomainNode; selected: boolean; onSelect: () => void }) {
  const style = { left: domain.x, top: domain.y, '--domain-color': domain.color } as CSSProperties
  return <button className={`domain-card ${selected ? 'selected' : ''}`} style={style} onClick={onSelect}>
    <span className="card-index">{String(domain.x + domain.y).padStart(3, '0')}</span>
    <span className="domain-orb" />
    <strong>{domain.name}</strong>
    <p>{domain.summary}</p>
    <div className="domain-card-meta"><span><b>{domain.features}</b> features</span><span><b>{domain.entrypoints}</b> entrypoints</span><span><b>{domain.confidence}%</b> 신뢰</span></div>
    <div className="card-foot"><span className={`confidence-dot ${domain.status}`} />{trustLabel(domain.status)}<span className="card-arrow">↗</span></div>
  </button>
}
