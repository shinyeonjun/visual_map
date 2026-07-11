const STORAGE_PREFIX = "backend-visual-map:map-context:v1:";
const MODES = new Set([
  "atlas",
  "explore",
  "api-flow",
  "table-usage",
  "column-impact",
  "search-focus",
]);

export type MapContext = {
  mode: string;
  focusId: string | null;
};

export function savedMapContext(workspaceId: string): MapContext {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(`${STORAGE_PREFIX}${workspaceId}`) ?? "null");
    if (!value || typeof value !== "object") {
      return atlasContext();
    }
    const { mode, focusId } = value as { mode?: unknown; focusId?: unknown };
    if (typeof mode !== "string" || !MODES.has(mode)) {
      return atlasContext();
    }
    return { mode, focusId: typeof focusId === "string" && focusId.length <= 512 ? focusId : null };
  } catch {
    return atlasContext();
  }
}

export function saveMapContext(workspaceId: string, mode: string, focusId?: string | null) {
  if (!MODES.has(mode)) {
    return;
  }
  try {
    window.localStorage.setItem(
      `${STORAGE_PREFIX}${workspaceId}`,
      JSON.stringify({ mode, focusId: typeof focusId === "string" && focusId.length <= 512 ? focusId : null }),
    );
  } catch {
    // Local storage can be disabled; the current investigation still works for this session.
  }
}

function atlasContext(): MapContext {
  return { mode: "atlas", focusId: null };
}
