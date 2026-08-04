import type { CodeAnalysisLanguage } from "../../types/workspace";

export function analysisStatusLabel(status: string): string {
  switch (status) {
    case "indexed":
    case "detected":
      return "정상";
    case "indexed-partial":
      return "부분";
    case "excluded":
      return "제외";
    default:
      return status || "확인 필요";
  }
}

export function analysisReasonLabel(reason: string): string {
  switch (reason) {
    case "test-only":
      return "테스트 전용";
    case "missing-dependency":
      return "의존성 메타데이터 없음";
    case "missing-compile-context":
      return "컴파일 정보 없음";
    case "unsupported-framework":
      return "지원되지 않는 프레임워크";
    case "runtime-reachability-unknown":
      return "실행 경로 확인 불가";
    default:
      return `제외 사유: ${reason}`;
  }
}

export function analysisScopeLabel(scope: string): string {
  switch (scope) {
    case "language":
      return "언어 범위";
    case "file":
      return "파일 범위";
    case "fact":
      return "근거 범위";
    default:
      return `범위: ${scope}`;
  }
}

/**
 * The one line a reader needs to judge whether the map covers this language:
 * how much of what was found actually reached the index, and why not.
 */
export function languageCoverageText(language: CodeAnalysisLanguage): string {
  const counts = `${language.filesFound.toLocaleString("ko-KR")}개 중 ${language.filesIndexed.toLocaleString("ko-KR")}개 분석`;
  const reason = language.exclusionReason ? analysisReasonLabel(language.exclusionReason) : null;
  return [counts, reason].filter(Boolean).join(" · ");
}
