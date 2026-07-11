import type { OperationStatus, UserError } from "../types/operation";

const actionLabels: Record<string, string> = {
  "workspace-create": "프로젝트 열기",
  "workspace-clone": "GitHub 저장소 복제",
  "workspace-open": "프로젝트 열기",
  "code-index": "코드 읽기",
  "code-load": "코드 불러오기",
  "db-save": "DB 연결 저장",
  "db-test": "DB 구조 테스트",
  "db-index": "DB 구조 읽기",
  "db-load": "테이블 불러오기",
};

export function runningOperation(action: string | null): OperationStatus | null {
  if (!action) {
    return null;
  }

  const label = actionLabels[action] ?? "작업";
  return {
    phase: "running",
    label,
    message: `${label} 진행 중`,
  };
}

export function idleOperation(): OperationStatus {
  return {
    phase: "idle",
    label: "작업 없음",
    message: "실행 중인 작업 없음",
  };
}

export function toUserError(error: unknown, fallback: string): UserError {
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
