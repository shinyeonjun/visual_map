import type { SearchResult, VisualMapControls } from "../../types/controls";

export function focusFirstSearchResult() {
  window.requestAnimationFrame(() => {
    document.querySelector<HTMLButtonElement>(".search-popover .search-result")?.focus();
  });
}

export function SearchResultsPopover({
  visualMapControls,
  searchScope,
}: {
  visualMapControls: VisualMapControls;
  searchScope: string;
}) {
  const draft = visualMapControls.searchQuery.trim();
  const defaultResultId = visualMapControls.searchGroups[0]?.results[0]?.id ?? null;
  const resultCount = visualMapControls.searchGroups.reduce((sum, group) => sum + group.results.length, 0);
  const hasResults = resultCount > 0;
  if (!visualMapControls.searchPopoverOpen) {
    return null;
  }
  if (!visualMapControls.searchSummary && visualMapControls.searchGroups.length === 0 && !draft) {
    return null;
  }

  return (
    <div
      className="search-popover"
      role="dialog"
      aria-label="검색 결과"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          visualMapControls.closeSearchPopover();
          focusSearchInput();
        }
      }}
    >
      <div className="search-popover-head">
        <strong>
          {visualMapControls.searchSummary ??
            (draft.length < 2 ? "두 글자 이상 입력하면 대상이 좁혀집니다." : "일치하는 대상 없음")}
        </strong>
        {hasResults && <span>표시 {resultCount}개</span>}
      </div>
      {visualMapControls.searchGroups.map((group) => (
        <div className="search-group" key={group.title}>
          <span>
            {group.title} · {group.results.length}개
          </span>
          {group.results.map((result) => (
            <SearchResultButton
              key={result.id}
              result={result}
              isDefault={result.id === defaultResultId}
              onClick={() => visualMapControls.selectSearchResult(result)}
            />
          ))}
        </div>
      ))}
      {!hasResults && <small className="search-hint">범위: {searchScope} · API 경로, 파일 경로, 테이블명, 컬럼명 일부를 입력하세요.</small>}
    </div>
  );
}

function SearchResultButton({
  result,
  isDefault,
  onClick,
}: {
  result: SearchResult;
  isDefault: boolean;
  onClick: () => void;
}) {
  const kind = resultKindLabel(result);
  const action = resultActionLabel(result);
  const description = `${kind}: ${result.title}${result.subtitle ? ` · ${result.subtitle}` : ""}`;
  return (
    <button
      className={`search-result ${resultToneClass(result)} ${isDefault ? "default" : ""}`}
      type="button"
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          focusSiblingSearchResult(event.currentTarget, 1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          focusSiblingSearchResult(event.currentTarget, -1);
        }
      }}
      title={`${description} · ${action}`}
      aria-label={`${description}. ${action}`}
    >
      <em>{kind}</em>
      <span className="search-result-copy">
        <span>{result.title}</span>
        {result.subtitle && <small>{result.subtitle}</small>}
      </span>
      <b>
        <span>{action}</span>
      </b>
    </button>
  );
}

function resultKindLabel(result: SearchResult): string {
  if (result.id.startsWith("api:")) {
    return "API";
  }
  if (result.id.startsWith("table:")) {
    return "테이블";
  }
  if (result.id.startsWith("column:")) {
    return "컬럼";
  }
  if (result.id.startsWith("file:")) {
    return "파일";
  }
  return result.codeItem ? codeKindLabel(result.codeItem.kind) : "코드";
}

function codeKindLabel(kind: string): string {
  const key = kind.trim().toLowerCase();
  if (key === "function" || key === "method") return "함수";
  if (key === "class") return "클래스";
  if (key === "service") return "서비스";
  if (key === "repository") return "리포지토리";
  if (key === "handler" || key === "controller") return "핸들러";
  if (key === "module") return "모듈";
  if (key === "file") return "파일";
  return "코드";
}

function resultActionLabel(result: SearchResult): string {
  if (result.id.startsWith("api:")) {
    return "흐름 보기";
  }
  if (result.id.startsWith("table:")) {
    return "테이블 보기";
  }
  if (result.id.startsWith("column:")) {
    return "영향 보기";
  }
  return "근거 보기";
}

function resultToneClass(result: SearchResult): string {
  if (result.id.startsWith("api:")) return "api";
  if (result.id.startsWith("table:")) return "table";
  if (result.id.startsWith("column:")) return "column";
  if (result.id.startsWith("file:")) return "file";
  return "code";
}

function focusSiblingSearchResult(current: HTMLButtonElement, offset: number) {
  const results = Array.from(document.querySelectorAll<HTMLButtonElement>(".search-popover .search-result"));
  const currentIndex = results.indexOf(current);
  const next = results[currentIndex + offset];
  if (next) {
    next.focus();
    return;
  }
  if (offset < 0) {
    focusSearchInput();
  }
}

function focusSearchInput() {
  window.requestAnimationFrame(() => {
    document.getElementById("global-inventory-search")?.focus();
  });
}
