import type { CSSProperties } from 'react'
import type { DomainNode } from '../domain'

export function DomainCard({ domain, selected, onSelect }: { domain: DomainNode; selected: boolean; onSelect: () => void }) {
  const style = { left: domain.x, top: domain.y, '--domain-color': domain.color } as CSSProperties
  return <button className={`domain-card ${selected ? 'selected' : ''}`} style={style} onClick={onSelect}>
    <span className="card-index">{String(domain.x + domain.y).padStart(3, '0')}</span>
    <span className="domain-orb" />
    <strong>{domain.name}</strong>
    <p>{domain.summary}</p>
    <div className="domain-card-meta"><span><b>{domain.features}</b> features</span><span><b>{domain.entrypoints}</b> entrypoints</span></div>
    <div className="card-foot"><span className={`confidence-dot ${domain.status}`} />{domain.status === 'verified' ? '확인된 도메인' : domain.status === 'shared' ? '공유 경계' : '분석 후보'}<span className="card-arrow">↗</span></div>
  </button>
}
