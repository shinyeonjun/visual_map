import { useState } from "react";
import type { AnalysisCoverage, VisualMap } from "../types/visual-map";

/**
 * Runtime state owned by the visual-map request lifecycle.
 *
 * Keeping this state together makes the orchestration hook responsible for
 * transitions and cancellation, while this hook owns only committed data and
 * status fields. No request or rendering behavior lives here.
 */
export function useVisualMapState(currentWorkspaceId: string | null) {
  const [visualMap, setVisualMap] = useState<VisualMap | null>(null);
  const [visualMapLoading, setVisualMapLoading] = useState(false);
  const [visualMapEnriching, setVisualMapEnriching] = useState(false);
  const [visualMapStatus, setVisualMapStatus] = useState<string | null>(null);
  const [visualMapError, setVisualMapError] = useState<string | null>(null);
  const [visualMapErrorDetail, setVisualMapErrorDetail] = useState<string | null>(null);
  const [snapshotSavedAt, setSnapshotSavedAt] = useState<string | null>(null);
  const [snapshotStaleReasons, setSnapshotStaleReasons] = useState<string[]>([]);
  const [snapshotSourceSummary, setSnapshotSourceSummary] = useState<string | null>(null);
  const [analysisCoverage, setAnalysisCoverage] = useState<AnalysisCoverage | null>(null);
  const [snapshotWorkspaceId, setSnapshotWorkspaceId] = useState<string | null>(null);
  const [projectionElapsedMs, setProjectionElapsedMs] = useState<number | null>(null);
  const [visualStateWorkspaceId, setVisualStateWorkspaceId] = useState<string | null>(currentWorkspaceId);
  const [visualTargetKey, setVisualTargetKey] = useState<string | null>(null);
  const [visualMapKey, setVisualMapKey] = useState<string | null>(null);

  return {
    visualMap,
    setVisualMap,
    visualMapLoading,
    setVisualMapLoading,
    visualMapEnriching,
    setVisualMapEnriching,
    visualMapStatus,
    setVisualMapStatus,
    visualMapError,
    setVisualMapError,
    visualMapErrorDetail,
    setVisualMapErrorDetail,
    snapshotSavedAt,
    setSnapshotSavedAt,
    snapshotStaleReasons,
    setSnapshotStaleReasons,
    snapshotSourceSummary,
    setSnapshotSourceSummary,
    analysisCoverage,
    setAnalysisCoverage,
    snapshotWorkspaceId,
    setSnapshotWorkspaceId,
    projectionElapsedMs,
    setProjectionElapsedMs,
    visualStateWorkspaceId,
    setVisualStateWorkspaceId,
    visualTargetKey,
    setVisualTargetKey,
    visualMapKey,
    setVisualMapKey,
  };
}
