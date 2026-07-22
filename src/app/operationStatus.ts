import type { CommandErrorPayload, OperationStatus, UserError } from "../types/operation";

export type OperationSource = "workspace" | "code" | "db" | "map";

const actions: Record<string, { label: string; source: OperationSource }> = {
  "workspace-create": { label: "프로젝트 열기", source: "workspace" },
  "workspace-clone": { label: "GitHub 저장소 복제", source: "workspace" },
  "workspace-open": { label: "프로젝트 열기", source: "workspace" },
  "workspace-refresh": { label: "GitHub 업데이트", source: "workspace" },
  "workspace-repair": { label: "프로젝트 복구", source: "workspace" },
  "workspace-delete": { label: "프로젝트 제거", source: "workspace" },
  "snapshot-restore": { label: "저장 결과 확인", source: "map" },
  "code-index": { label: "코드 읽기", source: "code" },
  "db-save": { label: "DB 연결 저장", source: "db" },
  "db-index": { label: "DB 구조 읽기", source: "db" },
  "db-delete": { label: "DB 연결 삭제", source: "db" },
  "map-load": { label: "캔버스", source: "map" },
};

export function runningOperation(action: string | null): OperationStatus | null {
  if (!action) {
    return null;
  }

  const label = actions[action]?.label ?? "작업";
  return {
    phase: "running",
    label,
    message: `${label} 진행 중`,
  };
}

export function operationSourceForAction(action: string | null): OperationSource | null {
  return action ? (actions[action]?.source ?? null) : null;
}

export function idleOperation(): OperationStatus {
  return {
    phase: "idle",
    label: "작업 없음",
    message: "실행 중인 작업 없음",
  };
}

export function toUserError(error: unknown, fallback: string): UserError {
  const commandError = commandErrorPayload(error);
  if (commandError) {
    return {
      message: `${fallback}: ${commandError.message}`,
      details: commandError.detail,
      code: commandError.code,
    };
  }
  const details = String(error);
  const lower = details.toLowerCase();

  if (lower.includes("timed out") || details.includes("시간")) {
    return { message: `${fallback}: 시간이 초과되었습니다`, details };
  }
  if (details.includes("읽기 도구가 없습니다")) {
    return { message: `${fallback}: 필요한 읽기 도구를 찾지 못했습니다`, details };
  }
  if (details.includes("최신이 아닙니다") || lower.includes("stale")) {
    return { message: `${fallback}: 코드/DB 읽기 결과가 최신이 아닙니다`, details };
  }
  if (details.includes("코드/DB 읽기 결과") || lower.includes("inventory snapshot") || (lower.includes("source") && lower.includes("snapshot"))) {
    return { message: `${fallback}: 코드 또는 DB 목록을 불러온 뒤 다시 시도하세요`, details };
  }

  return { message: fallback, details };
}

export function commandErrorCode(error: unknown): string | null {
  return commandErrorPayload(error)?.code ?? null;
}

function commandErrorPayload(error: unknown): CommandErrorPayload | null {
  if (isCommandErrorPayload(error)) {
    return error;
  }
  if (typeof error !== "string" || !error.trim().startsWith("{")) {
    return null;
  }
  try {
    const parsed: unknown = JSON.parse(error);
    return isCommandErrorPayload(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function isCommandErrorPayload(value: unknown): value is CommandErrorPayload {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Partial<CommandErrorPayload>;
  return (
    typeof candidate.code === "string" &&
    typeof candidate.message === "string" &&
    typeof candidate.detail === "string" &&
    typeof candidate.retryable === "boolean"
  );
}
