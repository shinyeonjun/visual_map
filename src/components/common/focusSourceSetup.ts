import type { DbProfileControls } from "../../types/controls";

export function focusDbProfileSetup(dbProfileControls: DbProfileControls) {
  window.requestAnimationFrame(() => {
    const target = dbProfileSetupTarget(dbProfileControls);
    target?.closest("details")?.setAttribute("open", "");
    target?.scrollIntoView({ block: "center", inline: "nearest" });
    target?.focus();
  });
}

function dbProfileSetupTarget(dbProfileControls: DbProfileControls): HTMLElement | null {
  return document.getElementById(
    dbProfileControls.profileName.trim() ? "db-profile-target-input" : "db-profile-name-input",
  );
}
