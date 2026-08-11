export type ProviderKind = "codex" | "claude";
export type ReasoningEffort = "low" | "medium" | "high" | "xhigh" | "max" | "ultra";

export interface WorkspaceProvider {
  kind: ProviderKind;
  model: string;
  /** Passed to the selected CLI, not just displayed in settings. */
  effort: ReasoningEffort;
}

export interface Workspace {
  schemaVersion: number;
  id: string;
  name: string;
  repoPath: string;
  provider: WorkspaceProvider | null;
  createdAt: number;
  updatedAt: number;
}

export interface AiProviderAvailability {
  kind: ProviderKind;
  label: string;
  installed: boolean;
  executable: string | null;
  version: string | null;
  error: string | null;
}

export interface EngineAvailability {
  id: string;
  label: string;
  role: string;
  available: boolean;
  integrity: string;
  version?: string;
  error: string | null;
}

export interface EngineRegistry {
  mode: "dev" | "internal" | "production";
  engineDir: string;
  engines: EngineAvailability[];
}

export interface FactGraphStatus {
  schemaVersion: number;
  snapshotId: string | null;
  sourceRevision: string | null;
  nodeCount: number;
  edgeCount: number;
  evidenceCount: number;
  coverageCount: number;
}

/** One result from the complete canonical Fact graph, not only the overview map. */
export interface FactNodeSearchResult {
  id: string;
  name: string;
  qualifiedName: string;
  kind: string;
  language: string | null;
}

export interface AnalyzeWorkspaceResult {
  factGraph: FactGraphStatus;
  semanticRevisionId: string | null;
  /** Static facts stay published even when the configured AI provider fails. */
  semanticError: string | null;
}

export interface AnalysisProgressEvent {
  workspaceId: string;
  stage: string;
  completed: number;
  total: number;
  label: string;
}

export interface CommandError {
  code?: string;
  message?: string;
  detail?: string;
}

export type SourceEditor = "vscode" | "cursor";

export interface SourceActionResult {
  path: string;
  line: number | null;
  column: number | null;
  action: string;
}
