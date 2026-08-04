import { confirm } from "@tauri-apps/plugin-dialog";
import { hasTauriRuntime } from "./tauriRuntime";

export async function confirmAction(message: string): Promise<boolean> {
  if (!hasTauriRuntime()) {
    return browserConfirm(message);
  }
  try {
    return await confirm(message, { title: "Backend Visual Map", kind: "warning" });
  } catch {
    // Keep destructive actions guarded when an older installed binary has not
    // refreshed its dialog capability yet.
    return browserConfirm(message);
  }
}

function browserConfirm(message: string): boolean {
  try {
    return window.confirm(message);
  } catch {
    return false;
  }
}
