import type { ProviderKind, ReasoningEffort } from "./contracts";

/**
 * Exact values passed to each installed CLI.
 *
 * Codex names follow the GPT-5.6 family aliases published by OpenAI. Claude
 * uses the aliases exposed by `claude --help`, so the CLI remains responsible
 * for resolving them to the newest model available to the signed-in account.
 */
interface ProviderModel {
  id: string;
  label: string;
  note: string;
  family: string;
  efforts: ReasoningEffort[];
}

interface EffortOption {
  id: ReasoningEffort;
  label: string;
  note: string;
}

export const DEFAULT_REASONING_EFFORT: ReasoningEffort = "high";

const GPT_56_EFFORTS: ReasoningEffort[] = ["low", "medium", "high", "xhigh", "max", "ultra"];
const GPT_55_EFFORTS: ReasoningEffort[] = ["low", "medium", "high", "xhigh"];
const CLAUDE_EFFORTS: ReasoningEffort[] = ["low", "medium", "high", "xhigh", "max"];

const CODEX_MODELS: ProviderModel[] = [
  {
    id: "gpt-5.6-sol",
    label: "GPT-5.6 Sol",
    family: "5.6",
    note: "가장 높은 분석 품질",
    efforts: GPT_56_EFFORTS,
  },
  {
    id: "gpt-5.6-terra",
    label: "GPT-5.6 Terra",
    family: "5.6",
    note: "품질과 처리량의 균형",
    efforts: GPT_56_EFFORTS,
  },
  {
    id: "gpt-5.6-luna",
    label: "GPT-5.6 Luna",
    family: "5.6",
    note: "대규모 분석을 빠르게",
    efforts: GPT_56_EFFORTS,
  },
  {
    id: "gpt-5.5",
    label: "GPT-5.5",
    family: "5.5",
    note: "이전 세대 호환 모델",
    efforts: GPT_55_EFFORTS,
  },
];

const CLAUDE_MODELS: ProviderModel[] = [
  { id: "opus", label: "Claude Opus", family: "latest", note: "가장 깊은 분석", efforts: CLAUDE_EFFORTS },
  {
    id: "sonnet",
    label: "Claude Sonnet",
    family: "latest",
    note: "품질과 속도의 균형",
    efforts: CLAUDE_EFFORTS,
  },
  { id: "fable", label: "Claude Fable", family: "latest", note: "빠른 반복 분석", efforts: CLAUDE_EFFORTS },
];

const EFFORT_OPTIONS: EffortOption[] = [
  { id: "low", label: "낮음", note: "속도 우선" },
  { id: "medium", label: "보통", note: "일상 작업" },
  { id: "high", label: "높음", note: "기본값" },
  { id: "xhigh", label: "매우 높음", note: "복잡한 분석" },
  { id: "max", label: "최대", note: "최고 난도" },
  { id: "ultra", label: "울트라", note: "CLI 자동 위임" },
];

export function modelsFor(kind: ProviderKind): ProviderModel[] {
  return kind === "claude" ? CLAUDE_MODELS : CODEX_MODELS;
}

export function modelFor(kind: ProviderKind, id: string | null | undefined): ProviderModel | null {
  return modelsFor(kind).find((model) => model.id === id) ?? null;
}

export function defaultModelFor(kind: ProviderKind, current: string | null | undefined): string {
  const models = modelsFor(kind);
  if (current && models.some((model) => model.id === current)) return current;
  return models[0]?.id ?? "";
}

export function defaultEffortFor(
  kind: ProviderKind,
  modelId: string,
  current: ReasoningEffort | null | undefined,
): ReasoningEffort {
  const efforts = modelFor(kind, modelId)?.efforts ?? [DEFAULT_REASONING_EFFORT];
  if (current && efforts.includes(current)) return current;
  return efforts.includes(DEFAULT_REASONING_EFFORT) ? DEFAULT_REASONING_EFFORT : (efforts[0] ?? "high");
}

export function effortOptionsFor(kind: ProviderKind, modelId: string): EffortOption[] {
  const supported = new Set(modelFor(kind, modelId)?.efforts ?? [DEFAULT_REASONING_EFFORT]);
  return EFFORT_OPTIONS.filter((option) => supported.has(option.id));
}

export function effortLabel(effort: ReasoningEffort): string {
  return EFFORT_OPTIONS.find((option) => option.id === effort)?.label ?? effort;
}
