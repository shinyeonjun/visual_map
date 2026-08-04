import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { CodeSource } from "./CodeSource";
import { DatabaseSource } from "./DatabaseSource";
import { WorkspaceSource } from "./WorkspaceSource";

export function MapSourcePanel({
  workspaceControls,
  dbProfileControls,
  onEditDbConnection,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onEditDbConnection?: () => void;
}) {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);

  return (
    <div className="source-panel-content">
      <WorkspaceSource workspaceControls={workspaceControls} />
      {hasWorkspace ? <CodeSource workspaceControls={workspaceControls} /> : null}
      <DatabaseSource dbProfileControls={dbProfileControls} onEditDbConnection={onEditDbConnection} />
    </div>
  );
}
