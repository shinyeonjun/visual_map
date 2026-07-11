import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { focusDbProfileSetup, focusSourceSetup } from "../common/focusSourceSetup";
import type { View } from "../common/ViewSwitch";
import { InspectorPanel } from "../workbench/InspectorPanel";
import { AtlasModeList } from "./AtlasModeList";

export function AtlasRightPanel({
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const showDbSetup = () => {
    setView("workbench");
    window.requestAnimationFrame(() => focusDbProfileSetup(dbProfileControls));
  };
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasSelection = Boolean(
    visualMapControls.selectedNode ||
      visualMapControls.selectedEdge ||
      workspaceControls.selectedCodeItem,
  );
  const modePanel = (
    <section className="side-card" aria-label="찾을 답">
      <div className="at-panel-head">
        <span className="at-panel-title accent">찾을 답</span>
      </div>
      <AtlasModeList
        setView={setView}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />
    </section>
  );
  const inspector = hasWorkspace ? (
    <InspectorPanel
      showDbSetup={showDbSetup}
      showWorkspaceSetup={() => focusSourceSetup(setView, workspaceControls, dbProfileControls)}
      workspaceControls={workspaceControls}
      dbProfileControls={dbProfileControls}
      visualMapControls={visualMapControls}
    />
  ) : null;

  return (
    <aside className={`side side-right at-side ${hasWorkspace ? "" : "setup-pending"} ${hasSelection ? "answer-first" : ""}`}>
      {hasSelection ? inspector : modePanel}
      {hasSelection ? modePanel : inspector}
    </aside>
  );
}
