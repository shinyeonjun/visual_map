import type { DomainNode, DomainFeature } from '../../domain'
import { Icon } from '../icon'
import type { MapLayer } from '../../types/map'

interface MapToolbarProps {
  projectName: string
  mapLayer: MapLayer
  selectedDomain: DomainNode | undefined
  selectedFeature: DomainFeature | null
  query: string
  zoomPercent: number
  onGoToDomains: () => void
  onGoToFeatures: () => void
  onQueryChange: (value: string) => void
  onZoomIn: () => void
  onZoomOut: () => void
  onFit: () => void
}

export function MapToolbar({
  projectName,
  mapLayer,
  selectedDomain,
  selectedFeature,
  query,
  zoomPercent,
  onGoToDomains,
  onGoToFeatures,
  onQueryChange,
  onZoomIn,
  onZoomOut,
  onFit,
}: MapToolbarProps) {
  return (
    <header className="map-toolbar">
      <div className="map-toolbar-primary">
        <span className="eyebrow">
          {mapLayer === 'domains' ? 'BUSINESS MAP / 01' : mapLayer === 'features' ? 'FEATURE MAP / 02' : 'EXECUTION FLOW / 03'}
        </span>
        <div className="map-title-row">
          <h1 className="map-title">{projectName}</h1>
          <nav className="map-breadcrumb" aria-label="지도 경로">
            <button type="button" onClick={onGoToDomains}>도메인</button>
            {mapLayer !== 'domains' && selectedDomain && (
              <>
                <span aria-hidden="true">/</span>
                <button type="button" onClick={onGoToFeatures}>{selectedDomain.name}</button>
              </>
            )}
            {mapLayer === 'flow' && selectedFeature && (
              <>
                <span aria-hidden="true">/</span>
                <span className="map-breadcrumb-current">{selectedFeature.name}</span>
              </>
            )}
          </nav>
        </div>
        {mapLayer !== 'domains' && (
          <button type="button" className="map-back-link" onClick={mapLayer === 'flow' ? onGoToFeatures : onGoToDomains}>
            <Icon name="back" size={14} />
            {mapLayer === 'flow' ? '기능으로 돌아가기' : '도메인으로 돌아가기'}
          </button>
        )}
      </div>
      <div className="map-toolbar-legend">
        <div className="map-legend">
          {mapLayer === 'domains' ? (
            <>
              <span><i className="legend-line solid" />확인된 관계</span>
              <span><i className="legend-line dashed" />공유 경계</span>
              <span><i className="legend-node" />도메인</span>
            </>
          ) : mapLayer === 'features' ? (
            <>
              <span><i className="legend-node" />기능</span>
              <span>클릭하면 실행 길</span>
            </>
          ) : (
            <>
              <span><i className="legend-line solid" />실행 순서</span>
              <span>클릭하면 단계 강조</span>
            </>
          )}
          <span><i className="legend-dot verified" />확인됨</span>
          <span><i className="legend-dot candidate" />후보</span>
        </div>
      </div>
      <div className="map-toolbar-actions">
        {mapLayer !== 'flow' && (
          <label className="map-search">
            <Icon name="search" size={15} />
            <input
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder={mapLayer === 'features' ? '기능 검색' : '도메인 검색'}
            />
          </label>
        )}
        <div className="map-controls" aria-label="캔버스 확대/축소">
          <button type="button" onClick={onZoomOut} aria-label="축소">−</button>
          <span>{zoomPercent}%</span>
          <button type="button" onClick={onZoomIn} aria-label="확대">+</button>
          <button type="button" onClick={onFit} aria-label="화면 맞춤">⌖</button>
        </div>
      </div>
    </header>
  )
}
