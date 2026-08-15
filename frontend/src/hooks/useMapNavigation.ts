import { useCallback, useEffect, useState } from 'react'
import type { MapLayer } from '../types/map'

export function useMapNavigation() {
  const [mapLayer, setMapLayer] = useState<MapLayer>('domains')
  const [selectedId, setSelectedId] = useState('')
  const [selectedFeatureId, setSelectedFeatureId] = useState('')
  const [query, setQuery] = useState('')

  const openDomain = useCallback((domainId: string) => {
    setSelectedId(domainId)
    setSelectedFeatureId('')
    setQuery('')
    setMapLayer('features')
  }, [])

  const openFeature = useCallback((featureId: string) => {
    setSelectedFeatureId(featureId)
    setMapLayer('flow')
  }, [])

  const goToDomains = useCallback(() => {
    setMapLayer('domains')
    setSelectedFeatureId('')
    setQuery('')
  }, [])

  const goToFeatures = useCallback(() => {
    setMapLayer('features')
  }, [])

  const resetSelection = useCallback(() => {
    setSelectedId('')
    setSelectedFeatureId('')
    goToDomains()
  }, [goToDomains])

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== 'Escape') return
      if (mapLayer === 'flow') setMapLayer('features')
      else if (mapLayer === 'features') {
        setMapLayer('domains')
        setSelectedFeatureId('')
        setQuery('')
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [mapLayer])

  return {
    mapLayer,
    setMapLayer,
    selectedId,
    setSelectedId,
    selectedFeatureId,
    setSelectedFeatureId,
    query,
    setQuery,
    openDomain,
    openFeature,
    goToDomains,
    goToFeatures,
    resetSelection,
  }
}
