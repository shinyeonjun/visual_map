import type { VisualMapControls } from "../../types/controls";

export function focusGlobalSearch(visualMapControls: VisualMapControls) {
  visualMapControls.openSearchPopover();
  window.requestAnimationFrame(() => {
    const target = document.getElementById("global-inventory-search") as HTMLInputElement | null;
    target?.focus();
    target?.select();
  });
}
