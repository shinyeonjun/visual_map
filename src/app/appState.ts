import { idleOperation, runningOperation } from "./operationStatus";
import type { OperationStatus } from "../types/operation";
import type { RepoSourceMode } from "../types/workspace";

export function currentOperationStatus({
  busyAction,
  workspaceStatus,
  workspaceError,
  codeStatus,
  codeError,
  codeErrorDetail,
  dbStatus,
  dbError,
  dbErrorDetail,
  mapStatus,
  mapLoading,
  mapError,
  mapErrorDetail,
}: {
  busyAction: string | null;
  workspaceStatus: string | null;
  workspaceError: string | null;
  codeStatus: string | null;
  codeError: string | null;
  codeErrorDetail: string | null;
  dbStatus: string | null;
  dbError: string | null;
  dbErrorDetail: string | null;
  mapStatus: string | null;
  mapLoading: boolean;
  mapError: string | null;
  mapErrorDetail: string | null;
}): OperationStatus {
  const running = runningOperation(busyAction);
  if (running) {
    return running;
  }
  if (mapLoading) {
    return { phase: "running", label: "캔버스", message: "캔버스 준비 중" };
  }
  if (workspaceError) {
    return { phase: "error", label: "프로젝트", message: workspaceError };
  }
  if (codeError) {
    return { phase: "error", label: "코드", message: codeError, details: codeErrorDetail };
  }
  if (dbError) {
    return { phase: "error", label: "DB", message: dbError, details: dbErrorDetail };
  }
  if (mapError) {
    return { phase: "error", label: "캔버스", message: mapError, details: mapErrorDetail };
  }
  if (mapStatus && !isNonSuccessStatus(mapStatus)) {
    return { phase: "success", label: "캔버스", message: mapStatus };
  }
  if (codeStatus && !isNonSuccessStatus(codeStatus)) {
    return { phase: "success", label: "코드", message: codeStatus };
  }
  if (dbStatus && !isNonSuccessStatus(dbStatus)) {
    return { phase: "success", label: "DB", message: dbStatus };
  }
  if (workspaceStatus) {
    return { phase: "success", label: "프로젝트", message: workspaceStatus };
  }
  return idleOperation();
}

function isNonSuccessStatus(value: string): boolean {
  return value.includes("목록이 비어 있음") || value.includes("코드/DB 읽기 결과 필요") || value.includes("캔버스 항목 없음");
}

export function repoPathErrorFor(value: string, sourceMode: RepoSourceMode): string | null {
  const path = value.trim();
  if (!path) {
    return null;
  }
  if (sourceMode === "github") {
    return githubRepoName(path) ? null : "https://github.com/owner/repo 형식의 GitHub URL을 입력하세요.";
  }
  if (isRemoteUrl(path)) {
    return "GitHub URL 모드로 전환하세요.";
  }
  if (/^[a-z]:[\\/]/i.test(path) || path.startsWith("\\\\") || path.startsWith("/")) {
    return null;
  }
  return "로컬 저장소 폴더의 전체 경로를 입력하세요.";
}

function isRemoteUrl(value: string): boolean {
  return /^https?:\/\//i.test(value) || /^git@/i.test(value);
}

function githubRepoName(value: string): string | null {
  const trimmed = value.trim().replace(/\/$/, "");
  const path =
    trimmed.match(/^https:\/\/github\.com\/(.+)$/i)?.[1] ?? trimmed.match(/^git@github\.com:(.+)$/i)?.[1];
  if (!path) {
    return null;
  }

  const parts = path.split("/");
  if (parts.length !== 2) {
    return null;
  }

  const repo = parts[1].replace(/\.git$/i, "");
  return /^[a-z0-9._-]+$/i.test(parts[0]) && /^[a-z0-9._-]+$/i.test(repo) ? repo : null;
}
