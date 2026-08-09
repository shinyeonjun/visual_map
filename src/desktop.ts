import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AiProviderAvailability,
  AnalyzeWorkspaceResult,
  EngineRegistry,
  FactGraphStatus,
  ProviderKind,
  ReasoningEffort,
  Workspace,
} from "./contracts";
import type { MapView, Selection } from "./map/types";

export function hasDesktopRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function listWorkspaces(): Promise<Workspace[]> {
  if (!hasDesktopRuntime()) return [];
  return invoke<Workspace[]>("list_workspaces");
}

export async function listProviders(): Promise<AiProviderAvailability[]> {
  if (!hasDesktopRuntime()) {
    return [browserProvider("codex", "Codex"), browserProvider("claude", "Claude")];
  }
  return invoke<AiProviderAvailability[]>("list_ai_providers");
}

export async function getEngineRegistry(): Promise<EngineRegistry | null> {
  if (!hasDesktopRuntime()) return null;
  return invoke<EngineRegistry>("get_engine_availability");
}

export async function createWorkspace(name: string, repoPath: string): Promise<Workspace> {
  return invoke<Workspace>("create_workspace", {
    request: { name, repoPath },
  });
}

export async function setWorkspaceProvider(
  workspaceId: string,
  kind: ProviderKind,
  model: string,
  effort: ReasoningEffort,
): Promise<Workspace> {
  return invoke<Workspace>("set_workspace_provider", {
    request: { workspaceId, kind, model, effort },
  });
}

export async function getFactGraphStatus(workspaceId: string): Promise<FactGraphStatus> {
  return invoke<FactGraphStatus>("get_fact_graph_status", { workspaceId });
}

export async function analyzeWorkspace(workspaceId: string): Promise<AnalyzeWorkspaceResult> {
  return invoke<AnalyzeWorkspaceResult>("analyze_workspace", {
    request: { workspaceId },
  });
}

export async function getMapView(workspaceId: string): Promise<MapView | null> {
  return invoke<MapView | null>("get_map_view", { workspaceId });
}

export async function getMapSelection(workspaceId: string, selectedId: string): Promise<Selection | null> {
  return invoke<Selection | null>("get_map_selection", { workspaceId, selectedId });
}

export async function chooseRepositoryFolder(): Promise<string | null> {
  if (!hasDesktopRuntime()) return null;
  const result = await open({
    directory: true,
    multiple: false,
    title: "분석할 프로젝트 폴더 선택",
  });
  return typeof result === "string" ? result : null;
}

function browserProvider(kind: ProviderKind, label: string): AiProviderAvailability {
  return {
    kind,
    label,
    installed: false,
    executable: null,
    version: null,
    error: "데스크톱 앱에서 설치 상태를 확인합니다",
  };
}
