import type { ReactNode } from "react";
import type { EngineRegistry } from "../../types/engine";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { View } from "../common/ViewSwitch";
import { AtlasDatabasePanel } from "./AtlasDatabasePanel";
import { AtlasRepositoryPanel } from "./AtlasRepositoryPanel";

export function AtlasLeftPanel({
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
  engineError,
  devSlot,
}: {
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  devSlot?: ReactNode;
}) {
  return (
    <aside className="side side-left at-side">
      <AtlasRepositoryPanel
        setView={setView}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
      />
      <AtlasDatabasePanel
        setView={setView}
        workspaceControls={workspaceControls}
        dbProfileControls={dbProfileControls}
        visualMapControls={visualMapControls}
        engineRegistry={engineRegistry}
        engineError={engineError}
        devSlot={devSlot}
      />
    </aside>
  );
}
