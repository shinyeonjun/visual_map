import { EyeOff, TriangleAlert } from "lucide-react";
import type { CodeAnalysisLanguage, CodeInventory } from "../../types/workspace";
import { codeInventoryAnalysisQuality } from "../../types/workspace";
import { languageCoverageText } from "../../components/common/analysisLabels";

/**
 * A map drawn from a partial index is not wrong — it is a correct map of
 * something smaller than the project. That difference is invisible once the
 * canvas renders, so it has to be stated before the canvas is trusted.
 */
export type CoverageNoticeModel = {
  severity: "blocked" | "partial";
  headline: string;
  /** The whole-project ratio, so a reader never has to add the rows up. */
  summary: string;
  basis: string | null;
  gaps: CoverageGap[];
};

export type CoverageGap = {
  language: CodeAnalysisLanguage;
  missing: number;
  /**
   * A language can be "indexed" and still be absent from the map in every way
   * that matters — 8 of 3,312 files is not partial coverage, it is a hole.
   */
  effectivelyAbsent: boolean;
};

/** Missing this share of the project's files means the map is not the project. */
const BLOCKING_SHARE = 0.5;
/** Below this indexed share a language is treated as absent, not partial. */
const PRESENT_SHARE = 0.1;

export function analysisCoverageNotice(codeInventory: CodeInventory | null | undefined): CoverageNoticeModel | null {
  const quality = codeInventoryAnalysisQuality(codeInventory);
  if (!quality) {
    return null;
  }

  const gaps: CoverageGap[] = quality.languages
    .filter((language) => language.filesFound > language.filesIndexed)
    .map((language) => ({
      language,
      missing: language.filesFound - language.filesIndexed,
      effectivelyAbsent: language.filesIndexed === 0 || language.filesIndexed / language.filesFound < PRESENT_SHARE,
    }))
    .sort((left, right) => right.missing - left.missing);
  if (gaps.length === 0) {
    return null;
  }

  const missing = quality.filesFound - quality.filesIndexed;
  const blocked = quality.filesFound > 0 && missing / quality.filesFound >= BLOCKING_SHARE;
  const covered = quality.languages
    .filter((language) => language.filesIndexed > 0)
    .sort((left, right) => right.filesIndexed - left.filesIndexed);

  return {
    severity: blocked ? "blocked" : "partial",
    headline: blocked
      ? "이 지도는 프로젝트 전체가 아닙니다"
      : gaps.some((gap) => gap.effectivelyAbsent)
        ? "일부 언어가 지도에 없습니다"
        : "일부 파일이 지도에 없습니다",
    summary: `파일 ${quality.filesFound.toLocaleString("ko-KR")}개 중 ${quality.filesIndexed.toLocaleString("ko-KR")}개만 지도에 있습니다 (${percentText(quality.filesIndexed, quality.filesFound)})`,
    basis:
      covered.length > 0
        ? `지도의 근거: ${covered
            .map((language) => `${language.name} ${language.filesIndexed.toLocaleString("ko-KR")}개`)
            .join(" · ")}`
        : "분석된 파일이 없어 지도를 근거로 쓸 수 없습니다",
    gaps,
  };
}

function percentText(part: number, whole: number): string {
  if (whole <= 0) return "0%";
  const share = (part / whole) * 100;
  return share > 0 && share < 1 ? "1% 미만" : `${Math.round(share)}%`;
}

export function CoverageNotice({
  codeInventory,
  onOpenSources,
}: {
  codeInventory: CodeInventory | null | undefined;
  onOpenSources?: () => void;
}) {
  const model = analysisCoverageNotice(codeInventory);
  if (!model) {
    return null;
  }

  return (
    <section
      className="analysis-coverage-notice"
      data-coverage-severity={model.severity}
      role={model.severity === "blocked" ? "alert" : "status"}
      aria-label="분석 범위 경고"
    >
      <header>
        {model.severity === "blocked" ? <TriangleAlert size={15} /> : <EyeOff size={15} />}
        <span className="analysis-coverage-heading">
          <strong>{model.headline}</strong>
          <small>{model.summary}</small>
        </span>
        {onOpenSources ? (
          <button
            className="outline-action compact"
            type="button"
            onClick={onOpenSources}
            data-coverage-action="sources"
          >
            분석 설정
          </button>
        ) : null}
      </header>
      <ul className="analysis-coverage-list">
        {model.gaps.map(({ language, effectivelyAbsent }) => (
          <li key={language.id} data-coverage-kind={effectivelyAbsent ? "absent" : "partial"}>
            <b>{language.name}</b>
            <span>{languageCoverageText(language)}</span>
          </li>
        ))}
      </ul>
      {model.basis ? <small className="analysis-coverage-basis">{model.basis}</small> : null}
    </section>
  );
}
