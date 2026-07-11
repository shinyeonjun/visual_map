import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import { InspectorPanel } from "./InspectorPanel";
import { ModePanel } from "./ModePanel";

export function WorkbenchRightPanel({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasSelection = Boolean(
    visualMapControls.selectedNode ||
      visualMapControls.selectedEdge ||
      workspaceControls.selectedCodeItem,
  );
  const inspector = hasWorkspace ? (
    <InspectorPanel
      workspaceControls={workspaceControls}
      dbProfileControls={dbProfileControls}
      visualMapControls={visualMapControls}
    />
  ) : null;
  const modePanel = (
    <ModePanel
      workspaceControls={workspaceControls}
      dbProfileControls={dbProfileControls}
      visualMapControls={visualMapControls}
    />
  );

  return (
    <aside className={`side side-right ${hasWorkspace ? "" : "setup-pending"} ${hasSelection ? "answer-first" : ""}`}>
      {hasSelection ? inspector : modePanel}
      {hasSelection ? modePanel : inspector}
    </aside>
  );
}
