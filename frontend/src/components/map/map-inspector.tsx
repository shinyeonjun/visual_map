import type { DomainNode, DomainFeature, Project } from '../../domain'
import { trustLabel } from '../../domain'
import { featureKindLabel, featureSummaryText } from '../../lib/feature-presentation'
import { stepKindLabel } from '../../flow-presentation'
import { Icon } from '../icon'
import type { MapLayer } from '../../types/map'

interface MapInspectorProps {
  project: Project
  mapLayer: MapLayer
  selectedDomain: DomainNode | undefined
  selectedFeature: DomainFeature | null
  selectedFeatureId: string
  onResetSelection: () => void
  onOpenDomain: (domainId: string) => void
  onOpenFeature: (featureId: string) => void
  onSelectFeature: (featureId: string) => void
  onSelectDomain: (domainId: string) => void
  onGoToDomains: () => void
}

export function MapInspector({
  project,
  mapLayer,
  selectedDomain,
  selectedFeature,
  selectedFeatureId,
  onResetSelection,
  onOpenDomain,
  onOpenFeature,
  onSelectFeature,
  onSelectDomain,
  onGoToDomains,
}: MapInspectorProps) {
  return (
    <aside className="inspector">
      <div className="inspector-head">
        <div>
          <span className="eyebrow">{mapLayer === 'flow' ? 'SELECTED FEATURE' : 'SELECTED DOMAIN'}</span>
          <h2>{mapLayer === 'flow' ? selectedFeature?.name ?? '기능 선택' : selectedDomain?.name ?? '도메인 선택'}</h2>
        </div>
        <button className="icon-button" onClick={onResetSelection} aria-label="선택 해제"><Icon name="close" size={18} /></button>
      </div>
      {selectedDomain ? (
        <>
          <p className="inspector-summary">
            {mapLayer === 'flow' && selectedFeature
              ? featureSummaryText(selectedFeature)
              : selectedDomain.summary}
          </p>
          <div className="confidence">
            <div><span>분석 신뢰도</span><strong>{selectedDomain.confidence}%</strong></div>
            <div className="confidence-track"><span style={{ width: `${selectedDomain.confidence}%`, background: selectedDomain.color }} /></div>
            <small>정적 근거 {trustLabel(selectedDomain.status)} · {selectedDomain.units} units</small>
          </div>
          {mapLayer === 'domains' && selectedDomain.featureItems.length > 0 && (
            <button type="button" className="open-code-button" onClick={() => onOpenDomain(selectedDomain.id)}>
              기능 지도 열기
              <span>↗</span>
            </button>
          )}
          <div className="inspector-section">
            <div className="section-title"><span>구성된 기능</span><b>{selectedDomain.features}</b></div>
            {selectedDomain.featureItems.length > 0 ? (
              <div className="fake-list">
                {selectedDomain.featureItems.map((feature) => (
                  <button
                    type="button"
                    className={`fake-row feature-row${selectedFeatureId === feature.id ? ' selected' : ''}`}
                    key={feature.id}
                    onClick={() => {
                      onSelectFeature(feature.id)
                      if (mapLayer === 'features' || mapLayer === 'flow') onOpenFeature(feature.id)
                    }}
                    onDoubleClick={() => onOpenFeature(feature.id)}
                  >
                    <span className="mini-icon" style={{ color: selectedDomain.color }}>↗</span>
                    <span>
                      <strong>{feature.name}</strong>
                      <small>{featureSummaryText(feature)}</small>
                    </span>
                    <span className="feature-meta">
                      <span className={`trust-badge ${feature.status}`}>{trustLabel(feature.status)}</span>
                      <em>{featureKindLabel(feature.kind)}</em>
                    </span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="muted-copy">구성된 기능이 없습니다.</p>
            )}
          </div>
          {selectedFeature && (
            <div className="inspector-section flow-section">
              <div className="section-title">
                <span>실행 길</span>
                <b>{selectedFeature.flows.length}</b>
              </div>
              {mapLayer !== 'flow' && (
                <button type="button" className="open-code-button" onClick={() => onOpenFeature(selectedFeature.id)}>
                  실행 길 지도 열기
                  <span>↗</span>
                </button>
              )}
              {selectedFeature.flows.length > 0 ? (
                <div className="flow-list">
                  {selectedFeature.flows.slice(0, mapLayer === 'flow' ? 8 : 4).map((flow) => (
                    <div className="flow-block" key={flow.id}>
                      <div className="flow-owner">
                        <span>{flow.owner}</span>
                        <span className={`trust-badge ${flow.status}`}>{trustLabel(flow.status)}</span>
                      </div>
                      {flow.steps.length > 0 ? (
                        <ol className="flow-steps">
                          {flow.steps.slice(0, 6).map((step, index) => (
                            <li key={`${flow.id}-${index}`} className={step.status === 'candidate' ? 'candidate-step' : undefined}>
                              <span className="flow-step-label">{step.label}</span>
                              <span className="flow-step-meta">
                                <span className={`trust-dot ${step.status}`} aria-label={trustLabel(step.status)} />
                                <em>{stepKindLabel(step.kind)}</em>
                              </span>
                            </li>
                          ))}
                        </ol>
                      ) : (
                        <p className="muted-copy">표시할 단계가 없습니다.</p>
                      )}
                    </div>
                  ))}
                  {selectedFeature.flows.length > (mapLayer === 'flow' ? 8 : 4) && (
                    <p className="muted-copy">나머지 {selectedFeature.flows.length - (mapLayer === 'flow' ? 8 : 4)}개는 실행 길 지도에서 확인하세요.</p>
                  )}
                </div>
              ) : (
                <p className="muted-copy">이 기능에 연결된 실행 길이 없습니다.</p>
              )}
            </div>
          )}
          <div className="inspector-section">
            <div className="section-title"><span>연결된 도메인</span><b>{selectedDomain.dependencies.length}</b></div>
            {selectedDomain.dependencies.length > 0 ? (
              <div className="linked-domains">
                {selectedDomain.dependencies.map((id) => {
                  const linked = project.domains.find((item) => item.id === id)
                  return linked ? (
                    <button key={id} onClick={() => { onSelectDomain(id); onGoToDomains() }}>
                      <span className="linked-dot" style={{ background: linked.color }} />
                      {linked.name}
                      <span>↗</span>
                    </button>
                  ) : null
                })}
              </div>
            ) : (
              <p className="muted-copy">직접 연결된 도메인이 없습니다.</p>
            )}
          </div>
          <div className="inspector-section evidence">
            <div className="section-title"><span>분석 근거</span><b>{selectedDomain.signals.length}</b></div>
            <div className="tag-list">{selectedDomain.signals.map((signal) => <span key={signal}>{signal}</span>)}</div>
          </div>
        </>
      ) : (
        <div className="inspector-empty">
          <div className="empty-ring">+</div>
          <strong>지도를 탐색해보세요</strong>
          <span>도메인 카드를 더블클릭하면 기능이, 기능을 클릭하면 실행 길이 열립니다.</span>
        </div>
      )}
    </aside>
  )
}
