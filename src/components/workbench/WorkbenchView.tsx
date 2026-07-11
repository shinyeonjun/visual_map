import type { ReactNode } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { EngineRegistry } from "../../types/engine";
import { codeInventoryItemCount } from "../../types/workspace";
import { AtlasCanvas } from "../atlas/AtlasCanvas";
import { ContextRibbon } from "../common/ContextRibbon";
import type { View } from "../common/ViewSwitch";
import { WorkbenchLeftPanel } from "./WorkbenchLeftPanel";
import { WorkbenchRightPanel } from "./WorkbenchRightPanel";
import { WorkbenchStatusBar } from "./WorkbenchStatusBar";
import { WorkbenchTopBar } from "./WorkbenchTopBar";

export function WorkbenchView({
  view,
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
  engineError,
  devSlot,
}: {
  view: View;
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  devSlot?: ReactNode;
}) {
  const hasAnswerSource =
    codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);

  return (
    <div className="app-shell" data-view="workbench">
      <WorkbenchTopBar
        view={view}
        setView={setView}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />
      {hasAnswerSource && <ContextRibbon workspaceControls={workspaceControls} visualMapControls={visualMapControls} />}
      <div className="workspace">
        <WorkbenchLeftPanel
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
        />
        <AtlasCanvas
          setView={setView}
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
        />
        {hasAnswerSource && (
          <WorkbenchRightPanel
            workspaceControls={workspaceControls}
            dbProfileControls={dbProfileControls}
            visualMapControls={visualMapControls}
          />
        )}
      </div>
      {hasAnswerSource && (
        <WorkbenchStatusBar
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
          engineRegistry={engineRegistry}
          engineError={engineError}
          devSlot={devSlot}
        />
      )}
    </div>
  );
}
