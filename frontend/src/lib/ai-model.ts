import type { CodexModelCatalog } from '../domain'

export function resolveSelectedModel(current: string, catalog: CodexModelCatalog): string {
  if (catalog.models.some((item) => item.slug === current)) return current
  if (catalog.selectedModel && catalog.models.some((item) => item.slug === catalog.selectedModel)) {
    return catalog.selectedModel
  }
  return catalog.models[0]?.slug ?? ''
}
