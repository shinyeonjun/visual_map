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

type StoredMapContext = MapContext & {
  focusByMode?: Record<string, string | null>;
};

export function savedMapContext(workspaceId: string): MapContext {
  const stored = readMapContext(workspaceId);
  return stored ? { mode: stored.mode, focusId: stored.focusId } : atlasContext();
}

export function savedModeMapContext(workspaceId: string, mode: string): MapContext | null {
  if (!MODES.has(mode)) {
    return null;
  }
  const stored = readMapContext(workspaceId);
  if (!stored) {
    return null;
  }
  if (stored.focusByMode && Object.prototype.hasOwnProperty.call(stored.focusByMode, mode)) {
    return { mode, focusId: stored.focusByMode[mode] ?? null };
  }
  return stored.mode === mode ? { mode, focusId: stored.focusId } : null;
}

export function saveMapContext(workspaceId: string, mode: string, focusId?: string | null) {
  if (!MODES.has(mode)) {
    return;
  }
  try {
    const normalizedFocus = validFocusId(focusId);
    const previous = readMapContext(workspaceId);
    window.localStorage.setItem(
      `${STORAGE_PREFIX}${workspaceId}`,
      JSON.stringify({
        mode,
        focusId: normalizedFocus,
        focusByMode: { ...previous?.focusByMode, [mode]: normalizedFocus },
      }),
    );
  } catch {
    // Local storage can be disabled; the current investigation still works for this session.
  }
}

export function resetMapContext(workspaceId: string) {
  try {
    window.localStorage.removeItem(`${STORAGE_PREFIX}${workspaceId}`);
  } catch {
    return;
  }
  saveMapContext(workspaceId, "atlas", null);
}

function readMapContext(workspaceId: string): StoredMapContext | null {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(`${STORAGE_PREFIX}${workspaceId}`) ?? "null");
    if (!value || typeof value !== "object") {
      return null;
    }
    const { mode, focusId, focusByMode } = value as {
      mode?: unknown;
      focusId?: unknown;
      focusByMode?: unknown;
    };
    if (typeof mode !== "string" || !MODES.has(mode)) {
      return null;
    }
    const stored: StoredMapContext = { mode, focusId: validFocusId(focusId) };
    if (focusByMode && typeof focusByMode === "object") {
      stored.focusByMode = Object.fromEntries(
        Object.entries(focusByMode)
          .filter(([storedMode]) => MODES.has(storedMode))
          .map(([storedMode, storedFocus]) => [storedMode, validFocusId(storedFocus)]),
      );
    }
    return stored;
  } catch {
    return null;
  }
}

function validFocusId(value: unknown): string | null {
  return typeof value === "string" && value.length <= 512 ? value : null;
}

function atlasContext(): MapContext {
  return { mode: "atlas", focusId: null };
}
