import { dbProfileWorkStarted } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import { codeInventoryItemCount } from "../../types/workspace";

export function focusSourceSetup(
  openSourceManager: () => void,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
) {
  openSourceManager();
  window.requestAnimationFrame(() => {
    window.requestAnimationFrame(() => {
      const target = sourceSetupTarget(workspaceControls, dbProfileControls);
      if (target && !target.matches("button, input, select, textarea, a[href], [tabindex]")) {
        target.tabIndex = -1;
      }
      target?.closest("details")?.setAttribute("open", "");
      target?.scrollIntoView({ block: "center", inline: "nearest" });
      target?.focus();
    });
  });
}

export function focusDbProfileSetup(dbProfileControls: DbProfileControls) {
  window.requestAnimationFrame(() => {
    const target = dbProfileSetupTarget(dbProfileControls);
    target?.closest("details")?.setAttribute("open", "");
    target?.scrollIntoView({ block: "center", inline: "nearest" });
    target?.focus();
  });
}

export function focusWorkspaceSetup(workspaceControls: WorkspaceControls) {
  window.requestAnimationFrame(() => {
    const target = workspaceSetupTarget(workspaceControls);
    target?.closest("details")?.setAttribute("open", "");
    target?.scrollIntoView({ block: "center", inline: "nearest" });
    target?.focus();
  });
}

function sourceSetupTarget(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
): HTMLElement | null {
  if (!workspaceControls.currentWorkspace) {
    return workspaceSetupTarget(workspaceControls);
  }

  const hasCode = codeInventoryItemCount(workspaceControls.codeInventory) > 0;
  const dbStarted = dbProfileWorkStarted(dbProfileControls);
  if (!hasCode && !dbStarted && workspaceControls.canIndexCode) {
    return (
      document.querySelector<HTMLElement>(".code-source .source-next button") ??
      document.querySelector<HTMLElement>(".code-source .source-next")
    );
  }

  return (
    document.querySelector<HTMLElement>(".database-source .source-next button") ??
    dbProfileSetupTarget(dbProfileControls)
  );
}

function dbProfileSetupTarget(dbProfileControls: DbProfileControls): HTMLElement | null {
  return document.getElementById(dbProfileControls.profileName.trim() ? "db-profile-target-input" : "db-profile-name-input");
}

function workspaceSetupTarget(workspaceControls: WorkspaceControls): HTMLElement | null {
  return workspaceControls.canCreateWorkspace
    ? document.querySelector<HTMLElement>(".workspace-source .source-next button")
    : document.getElementById("workspace-repo-input");
}
