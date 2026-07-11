import {
  Box,
  ChevronDown,
  Database,
  Layers3,
  Plug,
  Search,
} from "lucide-react";
import { useSearchHotkey } from "../../hooks/useSearchHotkey";
import { codeInventoryItemCount, dbProfileSourceLabel } from "../../types/workspace";
import { searchScopeText } from "../../visual/search";
import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import { focusSourceSetup } from "../common/focusSourceSetup";
import { SearchResultsPopover, focusFirstSearchResult } from "../common/SearchResultsPopover";
import type { View } from "../common/ViewSwitch";
import { ViewSwitch } from "../common/ViewSwitch";

export function AtlasTopBar({
  view,
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  view: View;
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const workspaceName = workspaceControls.currentWorkspace?.name ?? "프로젝트 열기";
  const dbSource = dbProfileControls.activeProfile?.source;
  const dbTables = dbProfileControls.inventory?.tables ?? null;
  const dbTableCount = dbTables?.length ?? null;
  const dbColumnCount = dbTables?.reduce((sum, table) => sum + table.columns.length, 0) ?? null;
  const dbMissingColumnTables = dbTables?.filter((table) => table.columns.length === 0).length ?? 0;
  const dbProfileName = dbProfileControls.activeProfile?.name ?? (dbTableCount !== null ? "저장된 구조" : "연결 전");
  const dbSourceLabel = dbSource ? dbProfileSourceLabel(dbSource) : dbTableCount !== null ? "저장된 구조" : null;
  const dbNeedsColumns = dbTableCount !== null && dbTableCount > 0 && (dbColumnCount ?? 0) === 0;
  const dbPartialColumns = dbMissingColumnTables > 0 && (dbColumnCount ?? 0) > 0;
  const dbReady = dbTableCount !== null && dbTableCount > 0 && !dbNeedsColumns && !dbPartialColumns;
  const searchInputRef = useSearchHotkey(visualMapControls.openSearchPopover);
  const hasCodeItems = codeInventoryItemCount(workspaceControls.codeInventory) > 0;
  const canSearch = hasCodeItems || Boolean(dbProfileControls.inventory?.tables.length);
  const searchScope = searchScopeText(workspaceControls.codeInventory, dbProfileControls.inventory);
  const searchPlaceholder = `${searchScope} 찾기`;
  const hasWorkspaceChoices = Boolean(workspaceControls.currentWorkspace) || workspaceControls.workspaces.length > 0;
  const workspaceSelectDisabled = workspaceControls.busy || (!workspaceControls.currentWorkspace && workspaceControls.workspaces.length === 0);
  const workspacePendingLabel = workspaceControls.canCreateWorkspace
    ? workspaceControls.repoSourceMode === "github"
      ? "복제 준비"
      : "열기 준비"
    : workspaceControls.repoSourceMode === "github"
      ? "URL 필요"
      : "폴더 필요";
  const workspaceSelectLabel = workspaceSelectDisabled ? workspacePendingLabel : workspaceName;
  const dbStateText =
    dbTableCount !== null
      ? dbReady
        ? `DB: ${dbProfileName} · 테이블 ${dbTableCount}개 · 컬럼 ${dbColumnCount ?? 0}개`
        : dbPartialColumns
          ? `DB: ${dbProfileName} · 컬럼 일부 대기 ${dbMissingColumnTables}/${dbTableCount}`
        : dbNeedsColumns
          ? `DB: ${dbProfileName} · 컬럼 대기 · 테이블 ${dbTableCount}개`
        : `DB: ${dbProfileName} · 테이블 없음`
      : dbSource
        ? `DB: 읽기 대기 · ${dbProfileName}`
        : hasCodeItems
          ? "DB: 연결하면 영향 범위"
          : "DB: 연결 전";
  const dbStateTitle = dbSourceLabel ? `${dbStateText} · 연결: ${dbSourceLabel}` : dbStateText;

  return (
    <header className="topbar">
      <button className="brand-home" type="button" onClick={() => setView("workbench")} aria-label="코드/DB 연결로 돌아가기">
        <span className="brand-mark">
          <Layers3 size={18} />
        </span>
        <strong className="product-name">백엔드 비주얼 맵</strong>
      </button>
      {hasWorkspaceChoices ? (
        <label
          className={`top-select labeled select-shell ${workspaceSelectDisabled ? "disabled" : ""}`}
          title={workspaceSelectDisabled ? "프로젝트를 열면 선택할 수 있습니다" : undefined}
        >
          <span className="select-label">프로젝트</span>
          <span className="select-value">
            <Box size={14} />
            <select
              aria-label="프로젝트"
              value={workspaceControls.currentWorkspace?.id ?? ""}
              disabled={workspaceSelectDisabled}
              onChange={(event) => workspaceControls.openWorkspace(event.currentTarget.value)}
            >
              <option value="" disabled>
                {workspaceSelectLabel}
              </option>
              {workspaceControls.workspaces.map((workspace) => (
                <option value={workspace.id} key={workspace.id}>
                  {workspace.name}
                </option>
              ))}
            </select>
          </span>
          <ChevronDown size={13} />
        </label>
      ) : (
        <span
          className="top-select labeled passive"
          title="프로젝트를 열면 이곳에 표시됩니다"
        >
          <span className="select-label">프로젝트</span>
          <span className="select-value">
            <Box size={14} />
            <span className="select-static">{workspacePendingLabel}</span>
          </span>
        </span>
      )}
      <span
        className={`scan-pill db-state-pill ${dbReady ? "ready" : "pending"}`}
        title={dbStateTitle}
      >
        <Database size={14} />
        <span className="scan-pill-text">{dbStateText}</span>
      </span>
      <ViewSwitch canOpenAtlas={Boolean(workspaceControls.currentWorkspace)} view={view} setView={setView} />
      <div
        className="search-shell"
        onBlur={(event) => {
          const nextTarget = event.relatedTarget instanceof Node ? event.relatedTarget : null;
          if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
            visualMapControls.closeSearchPopover();
          }
        }}
      >
        {canSearch ? (
          <label className="global-search">
            <Search size={14} />
            <input
              id="global-inventory-search"
              ref={searchInputRef}
              aria-label="프로젝트 항목 검색"
              value={visualMapControls.searchQuery}
              onFocus={visualMapControls.openSearchPopover}
              onChange={(event) => visualMapControls.setSearchQuery(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  const firstResult = visualMapControls.searchGroups[0]?.results[0];
                  firstResult ? visualMapControls.selectSearchResult(firstResult) : visualMapControls.runSearch();
                } else if (event.key === "ArrowDown" && visualMapControls.searchGroups.length > 0) {
                  event.preventDefault();
                  focusFirstSearchResult();
                } else if (event.key === "Escape") {
                  visualMapControls.closeSearchPopover();
                }
              }}
              placeholder={searchPlaceholder}
              title="Ctrl+K로 검색"
            />
            <kbd aria-hidden="true">Ctrl K</kbd>
          </label>
        ) : (
          <button
            className="global-search search-setup"
            type="button"
            onClick={() => focusSourceSetup(setView, workspaceControls, dbProfileControls)}
            aria-label="코드 또는 DB 연결 설정 열기"
            title="코드 목록 또는 DB 연결을 설정합니다"
          >
            <Plug size={14} />
            <span>코드/DB 연결</span>
            <strong>설정</strong>
          </button>
        )}
        <SearchResultsPopover
          visualMapControls={visualMapControls}
          searchScope={searchScope}
        />
      </div>
    </header>
  );
}
