import {
  ChevronDown,
  CircleCheck,
  Clock3,
  Folder,
  FolderCog,
  Network,
  RefreshCw,
  Search,
  TriangleAlert,
} from "lucide-react";
import { useSearchHotkey } from "../../hooks/useSearchHotkey";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { codeInventoryItemCount } from "../../types/workspace";
import { searchScopeText } from "../../visual/search";
import { SearchResultsPopover, focusFirstSearchResult } from "../common/SearchResultsPopover";

export function WorkbenchTopBar({
  sourceManagerOpen,
  onToggleSourceManager,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  sourceManagerOpen: boolean;
  onToggleSourceManager: () => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasInventory =
    codeInventoryItemCount(workspaceControls.codeInventory) > 0 ||
    Boolean(dbProfileControls.inventory?.tables.length);
  const searchScope = searchScopeText(workspaceControls.codeInventory, dbProfileControls.inventory);
  const { searchInputRef, queueSearch, flushSearch } = useSearchHotkey(
    visualMapControls.openSearchPopover,
    visualMapControls.searchQuery,
    visualMapControls.setSearchQuery,
  );
  const freshness = sourceFreshness(workspaceControls, visualMapControls, hasInventory);
  const FreshnessIcon = freshness.icon;
  const sourceManagerActive = sourceManagerOpen || (!hasWorkspace && workspaceControls.initialized);

  return (
    <header className="topbar product-topbar">
      <div className="product-identity">
        <span className="brand-mark" aria-hidden="true">
          <Network size={18} />
        </span>
        <strong className="product-name">백엔드 비주얼 맵</strong>
      </div>

      <label className={`top-select select-shell ${hasWorkspace ? "" : "empty"}`}>
        <Folder size={15} />
        <select
          aria-label="프로젝트"
          value={workspaceControls.currentWorkspace?.id ?? ""}
          disabled={workspaceControls.busy || workspaceControls.workspaces.length === 0}
          onChange={(event) => workspaceControls.openWorkspace(event.currentTarget.value)}
        >
          <option value="">
            {hasWorkspace
              ? workspaceControls.currentWorkspace?.name
              : workspaceControls.initialized
                ? "프로젝트 없음"
                : "프로젝트 확인 중"}
          </option>
          {workspaceControls.workspaces.map((workspace) => (
            <option value={workspace.id} key={workspace.id}>
              {workspace.name}
            </option>
          ))}
        </select>
        <ChevronDown size={13} />
      </label>

      <span className={`source-freshness ${freshness.tone}`} title={freshness.detail}>
        <FreshnessIcon size={14} className={freshness.spin ? "spin" : undefined} />
        <span>{freshness.label}</span>
      </span>

      <div
        className={`search-shell product-search ${hasInventory ? "" : "disabled"}`}
        onBlur={(event) => {
          const nextTarget = event.relatedTarget instanceof Node ? event.relatedTarget : null;
          if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
            flushSearch(searchInputRef.current?.value ?? visualMapControls.searchQuery);
            visualMapControls.closeSearchPopover();
          }
        }}
      >
        <label className="global-search">
          <Search size={15} />
          <input
            id="global-inventory-search"
            ref={searchInputRef}
            aria-label="API, 함수, 파일, 테이블, 컬럼 검색"
            defaultValue={visualMapControls.searchQuery}
            disabled={!hasInventory}
            onFocus={() => hasInventory && visualMapControls.openSearchPopover()}
            onChange={(event) => queueSearch(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                if (event.currentTarget.value !== visualMapControls.searchQuery) {
                  flushSearch(event.currentTarget.value);
                  return;
                }
                const firstResult = visualMapControls.searchGroups[0]?.results[0];
                firstResult ? visualMapControls.selectSearchResult(firstResult) : visualMapControls.runSearch();
              } else if (event.key === "ArrowDown" && visualMapControls.searchGroups.length > 0) {
                if (event.currentTarget.value !== visualMapControls.searchQuery) {
                  event.preventDefault();
                  flushSearch(event.currentTarget.value);
                  return;
                }
                event.preventDefault();
                focusFirstSearchResult();
              } else if (event.key === "Escape") {
                flushSearch(event.currentTarget.value);
                visualMapControls.closeSearchPopover();
              }
            }}
            placeholder={hasInventory ? `${searchScope} 검색` : "소스를 연결하면 검색할 수 있습니다"}
            title="Ctrl+K로 검색"
          />
          <kbd aria-hidden="true">Ctrl K</kbd>
        </label>
        {hasInventory && (
          <SearchResultsPopover visualMapControls={visualMapControls} searchScope={searchScope} />
        )}
      </div>

      <button
        className={`source-manager-trigger ${sourceManagerActive ? "active" : ""}`}
        type="button"
        aria-pressed={hasWorkspace ? sourceManagerOpen : undefined}
        aria-current={!hasWorkspace && workspaceControls.initialized ? "page" : undefined}
        disabled={!workspaceControls.initialized}
        onClick={() => {
          if (!hasWorkspace) {
            document.querySelector<HTMLInputElement>("#workspace-repo-input")?.focus();
            return;
          }
          onToggleSourceManager();
        }}
      >
        <FolderCog size={16} />
        <span>소스 관리</span>
      </button>
    </header>
  );
}

function sourceFreshness(
  workspaceControls: WorkspaceControls,
  visualMapControls: VisualMapControls,
  hasInventory: boolean,
): {
  label: string;
  detail: string;
  tone: "fresh" | "stale" | "busy" | "pending" | "error";
  icon: typeof CircleCheck;
  spin?: boolean;
} {
  if (!workspaceControls.initialized) {
    return { label: "준비 중", detail: "프로젝트 목록을 확인하고 있습니다.", tone: "busy", icon: RefreshCw, spin: true };
  }
  if (workspaceControls.busy) {
    return { label: "분석 중", detail: "프로젝트 소스를 읽고 있습니다.", tone: "busy", icon: RefreshCw, spin: true };
  }
  if (workspaceControls.operationStatus.phase === "error") {
    return {
      label: "확인 필요",
      detail: workspaceControls.operationStatus.message,
      tone: "error",
      icon: TriangleAlert,
    };
  }
  if (visualMapControls.snapshotStaleReasons.length > 0) {
    return {
      label: "오래됨",
      detail: visualMapControls.snapshotStaleReasons.join(" · "),
      tone: "stale",
      icon: TriangleAlert,
    };
  }
  if (visualMapControls.snapshotSavedAt || hasInventory) {
    return {
      label: "마지막 읽기",
      detail: visualMapControls.snapshotSourceSummary ?? "마지막으로 읽은 코드와 DB 구조를 표시합니다.",
      tone: "fresh",
      icon: CircleCheck,
    };
  }
  return {
    label: "분석 전",
    detail: "코드 또는 데이터베이스 소스를 연결하세요.",
    tone: "pending",
    icon: Clock3,
  };
}
