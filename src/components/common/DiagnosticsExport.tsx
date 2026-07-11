import { getVersion } from "@tauri-apps/api/app";
import { ClipboardList } from "lucide-react";
import { useState } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { EngineRegistry } from "../../types/engine";
import { codeInventoryItemCount } from "../../types/workspace";
import { copyValue } from "./copyValue";

export function DiagnosticsExport({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
}) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  async function copyDiagnostics() {
    const version = await getVersion().catch(() => "unknown");
    const map = visualMapControls.currentMap;
    const tables = dbProfileControls.inventory?.tables ?? [];
    const bundle = {
      schemaVersion: 1,
      generatedAt: new Date().toISOString(),
      product: { version, platform: "windows-desktop" },
      engines: {
        mode: engineRegistry?.mode ?? "unknown",
        items: (engineRegistry?.engines ?? []).map((engine) => ({
          id: engine.id,
          role: engine.role,
          expectedVersion: engine.expectedVersion,
          contractVersion: engine.contractVersion,
          available: engine.available,
          releasable: engine.releasable,
          integrity: engine.integrity,
        })),
      },
      snapshot: {
        present: Boolean(workspaceControls.codeInventory || dbProfileControls.inventory),
        savedAt: visualMapControls.snapshotSavedAt,
        codeItems: codeInventoryItemCount(workspaceControls.codeInventory),
        dbTables: tables.length,
        dbColumns: tables.reduce((sum, table) => sum + table.columns.length, 0),
      },
      projection: {
        mode: visualMapControls.mode,
        elapsedMs: visualMapControls.projectionElapsedMs,
        visibleNodes: map?.nodes.length ?? 0,
        visibleEdges: map?.edges.length ?? 0,
        warnings: map?.warnings.length ?? 0,
      },
      errorClasses: [
        workspaceControls.operationStatus.phase === "error" ? "operation-error" : null,
        (engineRegistry?.engines ?? []).some((engine) => !engine.available) ? "engine-unavailable" : null,
        map?.warnings.length ? "projection-warning" : null,
      ].filter(Boolean),
    };
    const copied = await copyValue(`${JSON.stringify(bundle, null, 2)}\n`);
    setState(copied ? "copied" : "failed");
    window.setTimeout(() => setState("idle"), 1200);
  }

  return (
    <button
      className="dev-diag-chip"
      type="button"
      data-diagnostics-action="copy"
      data-copy-state={state}
      onClick={() => void copyDiagnostics()}
      title="경로와 소스 내용 없이 진단 요약 복사"
      aria-label="개인정보를 제외한 진단 요약 복사"
    >
      <ClipboardList size={10} />
      {state === "copied" ? "복사됨" : state === "failed" ? "실패" : "진단"}
    </button>
  );
}
