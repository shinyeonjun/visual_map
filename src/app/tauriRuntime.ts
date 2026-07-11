declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

export const tauriUnavailableMessage = "데스크톱 앱에서 실행하면 사용할 수 있습니다";

