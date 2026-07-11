import {
  ChevronDown,
  CircleCheck,
  Database,
  Folder,
  Network,
  RefreshCw,
  Search,
} from "lucide-react";
import { useSearchHotkey } from "../../hooks/useSearchHotkey";
import { codeInventoryItemCount, dbProfileSourceLabel } from "../../types/workspace";
import { searchScopeText } from "../../visual/search";
import type { VisualMapControls } from "../../types/controls";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import { SearchResultsPopover, focusFirstSearchResult } from "../common/SearchResultsPopover";
import type { View } from "../common/ViewSwitch";
import { ViewSwitch } from "../common/ViewSwitch";

export function WorkbenchTopBar({
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
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const workspaceName = workspaceControls.currentWorkspace?.name ?? "프로젝트 열기";
  const activeSource = dbProfileControls.activeProfile?.source;
  const dbSourceLabel = activeSource ? dbProfileSourceLabel(activeSource) : dbProfileControls.inventory ? "저장된 구조" : null;
  const dbTables = dbProfileControls.inventory?.tables ?? null;
  const dbTableCount = dbTables?.length ?? null;
  const dbColumnCount = dbTables?.reduce((sum, table) => sum + table.columns.length, 0) ?? null;
  const dbMissingColumnTables = dbTables?.filter((table) => table.columns.length === 0).length ?? 0;
  const dbProfileName = dbProfileControls.activeProfile?.name ?? (dbTableCount !== null ? "저장된 구조" : "연결 전");
  const dbNeedsColumns = dbTableCount !== null && dbTableCount > 0 && (dbColumnCount ?? 0) === 0;
  const dbPartialColumns = dbMissingColumnTables > 0 && (dbColumnCount ?? 0) > 0;
  const dbReady = dbTableCount !== null && dbTableCount > 0 && !dbNeedsColumns && !dbPartialColumns;
  const updatedAt = workspaceControls.currentWorkspace
    ? formatWorkspaceUpdatedAt(workspaceControls.currentWorkspace.updatedAt)
    : null;
  const searchInputRef = useSearchHotkey(visualMapControls.openSearchPopover);
  const hasCodeItems = codeInventoryItemCount(workspaceControls.codeInventory) > 0;
  const canSearch = hasCodeItems || Boolean(dbProfileControls.inventory?.tables.length);
  const showDbState = hasCodeItems || Boolean(activeSource) || dbTableCount !== null;
  const searchScope = searchScopeText(workspaceControls.codeInventory, dbProfileControls.inventory);
  const searchPlaceholder = `${searchScope} 찾기`;
  const hasWorkspaceChoices = hasWorkspace || workspaceControls.workspaces.length > 0;
  const workspaceSelectDisabled = workspaceControls.busy || (!hasWorkspace && workspaceControls.workspaces.length === 0);
  const workspacePendingLabel = workspaceControls.canCreateWorkspace
    ? workspaceControls.repoSourceMode === "github"
      ? "복제 준비"
      : "열기 준비"
    : workspaceControls.repoSourceMode === "github"
      ? "URL 필요"
      : "폴더 필요";
  const workspaceSelectLabel = workspaceSelectDisabled ? workspacePendingLabel : workspaceName;
  const workspaceState = workspaceControls.busy ? "busy" : workspaceControls.currentWorkspace ? "ready" : "pending";
  const WorkspaceStateIcon = workspaceState === "ready" ? CircleCheck : workspaceState === "busy" ? RefreshCw : Folder;
  const workspaceRequiredText = workspaceControls.repoSourceMode === "github" ? "GitHub URL 필요" : "로컬 폴더 필요";
  const workspaceStateText =
    workspaceState === "busy"
      ? "작업 진행 중"
      : workspaceState === "ready"
        ? "프로젝트 열림"
        : workspaceControls.canCreateWorkspace
          ? workspaceControls.repoSourceMode === "github"
            ? "저장소 복제 준비"
            : "프로젝트 열기 준비"
          : workspaceRequiredText;
  const dbStateText =
    dbTableCount !== null
      ? dbReady
        ? `DB: ${dbProfileName} · 테이블 ${dbTableCount}개 · 컬럼 ${dbColumnCount ?? 0}개`
        : dbPartialColumns
          ? `DB: ${dbProfileName} · 컬럼 일부 대기 ${dbMissingColumnTables}/${dbTableCount}`
        : dbNeedsColumns
          ? `DB: ${dbProfileName} · 컬럼 대기 · 테이블 ${dbTableCount}개`
        : `DB: ${dbProfileName} · 테이블 없음`
      : activeSource
        ? `DB: 읽기 대기 · ${dbProfileName}`
        : hasCodeItems
          ? "DB: 연결하면 영향 범위"
          : "DB: 연결 전";
  const dbStateTitle = dbSourceLabel ? `${dbStateText} · 연결: ${dbSourceLabel}` : dbStateText;

  return (
    <header className="topbar">
      <div className="brand-mark">
        <Network size={18} />
      </div>
      <strong className="product-name">백엔드 비주얼 맵</strong>
      {hasWorkspaceChoices && (
        <label
          className={`top-select select-shell ${workspaceSelectDisabled ? "disabled" : ""}`}
          title={workspaceSelectDisabled ? "프로젝트를 열면 선택할 수 있습니다" : undefined}
        >
          <Folder size={15} />
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
          <ChevronDown size={13} />
        </label>
      )}
      {(hasWorkspace || workspaceControls.busy) && (
        <span className={`scan-pill workspace-state-pill ${workspaceState}`} title={workspaceStateText}>
          <WorkspaceStateIcon size={14} className={workspaceState === "busy" ? "spin" : undefined} />
          <span className="scan-pill-text">{workspaceStateText}</span>
        </span>
      )}
      {showDbState && (
        <span
          className={`scan-pill db-state-pill ${dbReady ? "ready" : "pending"}`}
          title={dbStateTitle}
        >
          <Database size={14} />
          <span className="scan-pill-text">{dbStateText}</span>
        </span>
      )}
      {updatedAt && (
        <span className="top-time">
          {updatedAt}
          <RefreshCw size={12} className={workspaceControls.busy ? "spin" : undefined} />
        </span>
      )}
      {canSearch && (
        <>
          <ViewSwitch canOpenAtlas={hasWorkspace} view={view} setView={setView} />
          <div
            className="search-shell"
            onBlur={(event) => {
              const nextTarget = event.relatedTarget instanceof Node ? event.relatedTarget : null;
              if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
                visualMapControls.closeSearchPopover();
              }
            }}
          >
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
            <SearchResultsPopover
              visualMapControls={visualMapControls}
              searchScope={searchScope}
            />
          </div>
        </>
      )}
    </header>
  );
}

function formatWorkspaceUpdatedAt(value?: string | null) {
  if (!value) {
    return null;
  }

  const trimmed = value.trim();
  const numericTimestamp = /^\d+$/.test(trimmed) ? Number(trimmed) : null;
  const date =
    numericTimestamp !== null && Number.isFinite(numericTimestamp)
      ? new Date(trimmed.length <= 10 ? numericTimestamp * 1000 : numericTimestamp)
      : new Date(trimmed);

  if (Number.isNaN(date.getTime())) {
    return "업데이트 시간 확인 필요";
  }

  return new Intl.DateTimeFormat("ko-KR", { dateStyle: "short", timeStyle: "short" }).format(date);
}
