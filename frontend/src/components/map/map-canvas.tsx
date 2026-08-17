import { useEffect, useState } from 'react'
import type { CSSProperties, PointerEvent, RefCallback } from 'react'
import type { DomainNode, DomainFeature } from '../../domain'
import { DomainCard } from '../domain-card'
import { FeatureCard } from '../feature-card'
import { FlowListPanel, FlowMap } from '../flow-map'
import { Icon } from '../icon'
import { DOMAIN_CARD, FEATURE_CARD, gridPosition } from '../../map-layout'
import type { MapLayer } from '../../types/map'

interface DomainLayoutItem {
  domain: DomainNode
  x: number
  y: number
}

interface CanvasViewport {
  viewRef: RefCallback<HTMLDivElement>
  view: { scale: number }
  spaceHeld: boolean
  handlers: {
    onPointerDown: (event: PointerEvent<HTMLDivElement>) => void
    onPointerMove: (event: PointerEvent<HTMLDivElement>) => void
    onPointerUp: (event: PointerEvent<HTMLDivElement>) => void
    onPointerCancel: (event: PointerEvent<HTMLDivElement>) => void
  }
  gridStyle: CSSProperties
  stageStyle: CSSProperties
}

interface MapCanvasProps {
  mapLayer: MapLayer
  canvas: CanvasViewport
  canvasSize: { width: number; height: number }
  domainLayout: DomainLayoutItem[]
  visibleDomains: DomainNode[]
  visibleFeatures: DomainFeature[]
  selectedDomain: DomainNode | undefined
  selectedDomainId: string
  selectedFeatureId: string
  selectedFeature: DomainFeature | null
  selectedFlowId: string | null
  onSelectFlow: (id: string) => void
  onSelectDomain: (id: string) => void
  onOpenDomain: (id: string) => void
  onSelectFeature: (id: string) => void
  onOpenFeature: (id: string) => void
}

export function MapCanvas({
  mapLayer,
  canvas,
  canvasSize,
  domainLayout,
  visibleDomains,
  visibleFeatures,
  selectedDomain,
  selectedDomainId,
  selectedFeatureId,
  selectedFeature,
  selectedFlowId,
  onSelectFlow,
  onSelectDomain,
  onOpenDomain,
  onSelectFeature,
  onOpenFeature,
}: MapCanvasProps) {
  const flows = selectedFeature?.flows ?? []
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null)
  const selectedFlow = flows.find((flow) => flow.id === selectedFlowId) ?? flows[0] ?? null
  const color = selectedDomain?.color ?? '#3264d6'

  useEffect(() => {
    setActiveNodeId(null)
  }, [selectedFlowId])

  return (
    <div className="map-panel">
      {mapLayer === 'flow' && flows.length > 0 && (
        <FlowListPanel
          flows={flows}
          selectedFlowId={selectedFlowId}
          color={color}
          onSelectFlow={(id) => {
            onSelectFlow(id)
            setActiveNodeId(null)
          }}
        />
      )}
      <div
        ref={canvas.viewRef}
        className={`map-canvas${canvas.spaceHeld ? ' space-pan' : ''}`}
        {...canvas.handlers}
        onContextMenu={(event) => event.preventDefault()}
      >
        <div className="canvas-grid" style={canvas.gridStyle} />
        <div className="canvas-world" style={canvas.stageStyle}>
          {mapLayer === 'domains' && (
            <>
              <svg className="relation-lines" width={canvasSize.width} height={canvasSize.height} viewBox={`0 0 ${canvasSize.width} ${canvasSize.height}`} aria-hidden="true">
                {domainLayout.flatMap(({ domain, x, y }) => domain.dependencies.map((targetId) => {
                  const target = domainLayout.find((item) => item.domain.id === targetId)
                  if (!target) return null
                  return (
                    <line
                      key={`${domain.id}-${target.domain.id}`}
                      x1={x + DOMAIN_CARD.width}
                      y1={y + DOMAIN_CARD.height / 2}
                      x2={target.x}
                      y2={target.y + DOMAIN_CARD.height / 2}
                      className={domain.status === 'shared' ? 'relation shared' : 'relation'}
                    />
                  )
                }))}
              </svg>
              {domainLayout.map(({ domain, x, y }, index) => (
                <DomainCard
                  key={domain.id}
                  domain={domain}
                  index={index}
                  x={x}
                  y={y}
                  selected={domain.id === selectedDomainId}
                  onSelect={() => onSelectDomain(domain.id)}
                  onOpen={() => onOpenDomain(domain.id)}
                />
              ))}
            </>
          )}
          {mapLayer === 'features' && visibleFeatures.map((feature, index) => {
            const position = gridPosition(index, 4, FEATURE_CARD.width, FEATURE_CARD.height)
            return (
              <FeatureCard
                key={feature.id}
                feature={feature}
                color={color}
                x={position.x}
                y={position.y}
                selected={feature.id === selectedFeatureId}
                onSelect={() => onSelectFeature(feature.id)}
                onOpen={() => onOpenFeature(feature.id)}
              />
            )
          })}
          {mapLayer === 'flow' && selectedFlow && (
            <FlowMap
              flow={selectedFlow}
              color={color}
              activeNodeId={activeNodeId}
              onSelectNode={(nodeId) => setActiveNodeId(nodeId)}
            />
          )}
        </div>
        {mapLayer === 'domains' && visibleDomains.length === 0 && (
          <div className="empty-map"><Icon name="search" size={24} /><strong>찾는 도메인이 없습니다</strong><span>다른 검색어를 입력해보세요.</span></div>
        )}
        {mapLayer === 'features' && visibleFeatures.length === 0 && (
          <div className="empty-map"><Icon name="search" size={24} /><strong>이 도메인에 표시할 기능이 없습니다</strong><span>도메인으로 돌아가 다른 카드를 열어보세요.</span></div>
        )}
        {mapLayer === 'flow' && selectedFeature && selectedFeature.flows.length === 0 && (
          <div className="empty-map"><Icon name="route" size={24} /><strong>이 기능에 연결된 실행 길이 없습니다</strong><span>진입점에서 이어진 코드 일이 없으면 길을 그리지 않습니다.</span></div>
        )}
        <div className="map-hint">
          <span>
            {mapLayer === 'domains'
              ? `도메인 ${visibleDomains.length}개 · 더블클릭하면 기능`
              : mapLayer === 'features'
                ? `기능 ${visibleFeatures.length}개 · 클릭하면 실행 길`
                : `실행 길 ${flows.length}개 · Esc로 돌아가기`}
          </span>
          <span className="map-hint-divider" aria-hidden="true">·</span>
          <span>핀치 / Ctrl+휠 확대</span>
          <span className="map-hint-divider" aria-hidden="true">·</span>
          <span>드래그 이동</span>
        </div>
      </div>
    </div>
  )
}
