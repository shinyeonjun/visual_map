import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AiProviderAvailability,
  AnalyzeWorkspaceResult,
  EngineRegistry,
  FactGraphStatus,
  FactNodeSearchResult,
  ProviderKind,
  ReasoningEffort,
  SourceActionResult,
  SourceEditor,
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

/** @public Search read-model boundary reserved for the separately implemented search UI. */
export async function searchFactNodes(workspaceId: string, query: string, limit = 40): Promise<FactNodeSearchResult[]> {
  if (!hasDesktopRuntime() || !query.trim()) return [];
  return invoke<FactNodeSearchResult[]>("search_fact_nodes", { workspaceId, query, limit });
}

type AnalysisCachePolicy = "reuse" | "fresh";

export async function analyzeWorkspace(
  workspaceId: string,
  cachePolicy: AnalysisCachePolicy,
): Promise<AnalyzeWorkspaceResult> {
  return invoke<AnalyzeWorkspaceResult>("analyze_workspace", {
    request: { workspaceId, cachePolicy },
  });
}

export async function cancelWorkspaceAnalysis(workspaceId: string): Promise<boolean> {
  return invoke<boolean>("cancel_workspace_analysis", { workspaceId });
}

export async function deleteWorkspace(workspaceId: string): Promise<void> {
  return invoke<void>("delete_workspace", { workspaceId });
}

export async function openSourceLocation(
  workspaceId: string,
  path: string,
  line: number | null,
  column: number | null = null,
  editor: SourceEditor = "vscode",
): Promise<SourceActionResult> {
  return invoke<SourceActionResult>("open_source_location", {
    request: { workspaceId, path, line, column, editor },
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
