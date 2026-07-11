import type { ReactNode } from "react";
import type { EngineRegistry } from "../../types/engine";
import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import type { View } from "../common/ViewSwitch";
import { ContextRibbon } from "../common/ContextRibbon";
import { AtlasCanvas } from "./AtlasCanvas";
import { AtlasLeftPanel } from "./AtlasLeftPanel";
import { AtlasRightPanel } from "./AtlasRightPanel";
import { AtlasTopBar } from "./AtlasTopBar";
import { WorkbenchStatusBar } from "../workbench/WorkbenchStatusBar";

export function AtlasView({
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
  return (
    <div className="app-shell" data-view="atlas">
      <AtlasTopBar
        view={view}
        setView={setView}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />
      <ContextRibbon workspaceControls={workspaceControls} visualMapControls={visualMapControls} />
      <div className="workspace">
        <AtlasLeftPanel
          setView={setView}
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
          engineRegistry={engineRegistry}
          engineError={engineError}
          devSlot={devSlot}
        />
        <AtlasCanvas
          setView={setView}
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
        />
        <AtlasRightPanel
          setView={setView}
          workspaceControls={workspaceControls}
          dbProfileControls={dbProfileControls}
          visualMapControls={visualMapControls}
        />
      </div>
      <WorkbenchStatusBar
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
        engineRegistry={engineRegistry}
        engineError={engineError}
        devSlot={devSlot}
      />
    </div>
  );
}
