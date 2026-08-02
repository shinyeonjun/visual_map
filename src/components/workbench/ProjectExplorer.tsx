import { Braces, ChevronDown, Code2, Database, FileCode2, Folder, GitBranch, Network, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { useSearchHotkey } from "../../hooks/useSearchHotkey";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { searchScopeText } from "../../visual/search";
import { SearchResultsPopover, focusFirstSearchResult } from "../common/SearchResultsPopover";
import {
  apiPathSegments,
  buildApiTree,
  buildCodeTree,
  buildTargetCatalog,
  countCodeTreeItems,
  type ApiTreeNode,
  type CodeTreeNode,
  type TargetCatalog,
  type TargetItem,
} from "./targetModel";

type ExplorerTab = "project" | "api" | "code" | "db";

const TABS: Array<{ id: ExplorerTab; label: string; icon: typeof Folder }> = [
  { id: "project", label: "프로젝트", icon: Folder },
  { id: "api", label: "API", icon: Braces },
  { id: "code", label: "코드", icon: Code2 },
  { id: "db", label: "DB", icon: Database },
];
const API_TREE_DISPLAY_LIMIT = 500;
const CODE_TREE_DISPLAY_LIMIT = 500;

export function ProjectExplorer({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onOpenDatabase,
  surface = "answers",
  onShowAnswers,
  onShowAdvanced,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onOpenDatabase: () => void;
  surface?: "answers" | "advanced";
  onShowAnswers?: () => void;
  onShowAdvanced?: (mode: "atlas") => void;
}) {
  const [tab, setTab] = useState<ExplorerTab>("project");
  const catalog = useMemo(
    () => buildTargetCatalog(workspaceControls.codeInventory, dbProfileControls.inventory),
    [workspaceControls.codeInventory, dbProfileControls.inventory],
  );
  const hasInventory = catalog.api.length > 0 || catalog.code.length > 0 || catalog.table.length > 0;
  const searchScope = searchScopeText(workspaceControls.codeInventory, dbProfileControls.inventory);
  const { searchInputRef, queueSearch, cancelQueuedSearch, flushSearch } = useSearchHotkey(
    visualMapControls.openSearchPopover,
    visualMapControls.searchQuery,
    visualMapControls.setSearchQuery,
  );
  const query = visualMapControls.searchQuery ?? "";
  const normalizedQuery = query.trim().toLocaleLowerCase("ko-KR");
  const currentWorkspace = workspaceControls.currentWorkspace;
  const activeFocusIds = [visualMapControls.focusId ?? visualMapControls.currentMap?.focus].filter(
    (value): value is string => Boolean(value),
  );

  return (
    <aside className="project-explorer" aria-label="프로젝트 탐색">
      <nav className="project-explorer-tabs" role="tablist" aria-label="프로젝트 정보 종류">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            className={tab === id ? "active" : ""}
            data-explorer-tab={id}
            data-target-kind={id === "project" ? undefined : id === "db" ? "table" : id}
            key={id}
            role="tab"
            type="button"
            aria-selected={tab === id}
            aria-label={id === "project" ? label : `${label} ${countForTab(id, catalog)}`}
            onClick={() => setTab(id)}
            onKeyDown={(event) => moveTab(event, id)}
          >
            <Icon size={16} />
            <span>{label}</span>
            {id !== "project" ? <small>{countForTab(id, catalog)}</small> : null}
          </button>
        ))}
      </nav>

      <div className="project-explorer-search">
        <Search size={14} aria-hidden="true" />
        <input
          ref={searchInputRef}
          id="global-inventory-search"
          defaultValue={visualMapControls.searchQuery}
          disabled={!hasInventory}
          onFocus={() => visualMapControls.openSearchPopover()}
          onChange={(event) => queueSearch(event.currentTarget.value)}
          onBlur={(event) => {
            const nextTarget = event.relatedTarget instanceof Node ? event.relatedTarget : null;
            if (!nextTarget || !event.currentTarget.parentElement?.contains(nextTarget)) {
              flushSearch(event.currentTarget.value);
              visualMapControls.closeSearchPopover();
            }
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              if (event.currentTarget.value !== visualMapControls.searchQuery) {
                const value = event.currentTarget.value;
                cancelQueuedSearch();
                visualMapControls.runSearch(value);
              } else {
                const firstResult = visualMapControls.searchGroups[0]?.results[0];
                if (firstResult) {
                  visualMapControls.selectSearchResult(firstResult);
                } else {
                  visualMapControls.runSearch();
                }
              }
            } else if (event.key === "ArrowDown" && visualMapControls.searchGroups.length > 0) {
              event.preventDefault();
              focusFirstSearchResult();
            } else if (event.key === "Escape") {
              flushSearch(event.currentTarget.value);
              visualMapControls.closeSearchPopover();
            }
          }}
          placeholder="검색 (파일, 엔드포인트, DB)"
          aria-label="프로젝트 탐색 검색"
        />
        {visualMapControls.searchPopoverOpen ? (
          <SearchResultsPopover visualMapControls={visualMapControls} searchScope={searchScope} />
        ) : null}
      </div>

      <div className="project-explorer-tree">
        {tab === "project" ? (
          <ProjectOverview
            workspaceName={currentWorkspace?.name ?? "분석 대상"}
            catalog={catalog}
            hasDbInventory={Boolean(dbProfileControls.inventory)}
            onOpenTab={setTab}
            onOpenDatabase={onOpenDatabase}
          />
        ) : (
          <TargetTree
            kind={tab === "db" ? "table" : tab}
            items={filteredItems(catalog, tab === "db" ? "table" : tab, normalizedQuery)}
            activeFocusIds={activeFocusIds}
            onSelect={selectTarget}
            emptyAction={tab === "db" && !dbProfileControls.inventory ? onOpenDatabase : undefined}
          />
        )}
      </div>
      <footer className="explorer-footer" aria-label="핵심 보기">
        <div className="explorer-footer-heading">
          <strong>핵심 보기</strong>
          <small>프로젝트 이해 또는 변경 영향</small>
        </div>
        <button
          className={surface === "advanced" ? "active" : ""}
          data-surface-action="understand"
          type="button"
          onClick={() => onShowAdvanced?.("atlas")}
        >
          <Network size={14} />
          이해하기
        </button>
        <button
          className={surface === "answers" ? "active" : ""}
          data-surface-action="impact"
          type="button"
          onClick={onShowAnswers}
        >
          <GitBranch size={14} />
          영향 보기
        </button>
      </footer>
    </aside>
  );

  function selectTarget(item: TargetItem) {
    visualMapControls.showMode(item.mode, item.focusId);
  }

  function moveTab(event: React.KeyboardEvent<HTMLButtonElement>, current: ExplorerTab) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const currentIndex = TABS.findIndex((item) => item.id === current);
    const nextIndex = (currentIndex + (event.key === "ArrowRight" ? 1 : -1) + TABS.length) % TABS.length;
    const next = TABS[nextIndex];
    setTab(next.id);
    window.requestAnimationFrame(() =>
      document.querySelector<HTMLButtonElement>(`[data-explorer-tab="${next.id}"]`)?.focus(),
    );
  }
}

function ProjectOverview({
  workspaceName,
  catalog,
  hasDbInventory,
  onOpenTab,
  onOpenDatabase,
}: {
  workspaceName: string;
  catalog: TargetCatalog;
  hasDbInventory: boolean;
  onOpenTab: (tab: Exclude<ExplorerTab, "project">) => void;
  onOpenDatabase: () => void;
}) {
  const screenRoutes = catalog.code.filter((item) => item.group === "화면 라우트").length;
  const codeItems = catalog.code.length - screenRoutes;
  const stats: Array<{
    tab: Exclude<ExplorerTab, "project">;
    icon: ReactNode;
    label: string;
    count: number;
    question: string;
  }> = [
    {
      tab: "api",
      icon: <Braces size={15} />,
      label: "API",
      count: catalog.api.length,
      question: "요청이 어디를 거치는지",
    },
    {
      tab: "code",
      icon: <Code2 size={15} />,
      label: "코드",
      count: codeItems,
      question: "누가 부르고 무엇을 부르는지",
    },
    {
      tab: "db",
      icon: <Database size={15} />,
      label: "DB",
      count: catalog.table.length,
      question: "무엇이 흔들리는지",
    },
  ];

  return (
    <div className="explorer-overview" aria-label={`${workspaceName} 분석 현황`}>
      <header className="explorer-overview-heading">
        <Folder size={15} />
        <strong>{workspaceName}</strong>
      </header>
      <p className="explorer-overview-guide">
        아래 종류를 고르면 그 질문에 맞는 지도가 열립니다. 목록은 각 탭에, 검색은 위에 있습니다.
      </p>
      <div className="explorer-overview-stats">
        {stats.map(({ tab, icon, label, count, question }) => (
          <button type="button" data-overview-tab={tab} onClick={() => onOpenTab(tab)} key={tab}>
            <span className="explorer-overview-kind">
              {icon}
              {label}
            </span>
            <strong>{count}</strong>
            <small>{question}</small>
          </button>
        ))}
      </div>
      {screenRoutes > 0 ? (
        <button className="explorer-overview-extra" type="button" onClick={() => onOpenTab("code")}>
          <GitBranch size={13} />
          화면 라우트 {screenRoutes}개 · 코드 탭에서 보기
        </button>
      ) : null}
      {!hasDbInventory ? (
        <button className="explorer-empty-action" type="button" onClick={onOpenDatabase}>
          DB를 아직 읽지 않았습니다 · DB 연결
        </button>
      ) : null}
    </div>
  );
}

function TargetTree({
  kind,
  items,
  activeFocusIds,
  onSelect,
  emptyAction,
}: {
  kind: "api" | "code" | "table";
  items: TargetItem[];
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
  emptyAction?: () => void;
}) {
  const label = kind === "api" ? "API 엔드포인트" : kind === "code" ? "코드 항목" : "테이블";
  return (
    <div className="explorer-tree-root">
      <TreeSection
        icon={kind === "api" ? <Braces size={15} /> : kind === "code" ? <Code2 size={15} /> : <Database size={15} />}
        label={label}
        count={items.length}
      >
        {kind === "api" ? (
          <ApiTreeItems items={items} activeFocusIds={activeFocusIds} onSelect={onSelect} />
        ) : kind === "code" ? (
          <CodeTreeItems items={items} activeFocusIds={activeFocusIds} onSelect={onSelect} />
        ) : (
          <TargetTreeItems items={items} activeFocusIds={activeFocusIds} onSelect={onSelect} />
        )}
        {items.length === 0 && emptyAction ? (
          <button className="explorer-empty-action" type="button" onClick={emptyAction}>
            DB 연결
          </button>
        ) : null}
      </TreeSection>
    </div>
  );
}

function ApiTreeItems({
  items,
  activeFocusIds,
  onSelect,
}: {
  items: TargetItem[];
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  if (items.length === 0) return <p className="explorer-empty">표시할 API가 없습니다.</p>;
  const visibleItems = items.slice(0, API_TREE_DISPLAY_LIMIT);
  return (
    <div className="explorer-api-tree">
      <ApiTreeNodes node={buildApiTree(visibleItems)} activeFocusIds={activeFocusIds} onSelect={onSelect} />
      {items.length > API_TREE_DISPLAY_LIMIT ? (
        <p className="explorer-more">
          상위 {API_TREE_DISPLAY_LIMIT}개만 표시합니다. 검색으로 나머지 API를 찾을 수 있습니다.
        </p>
      ) : null}
    </div>
  );
}

function ApiTreeNodes({
  node,
  activeFocusIds,
  onSelect,
}: {
  node: ApiTreeNode;
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  return (
    <div className="explorer-api-tree-branch">
      {node.children.map((child) => (
        <ApiTreeNodeView key={child.key} node={child} activeFocusIds={activeFocusIds} onSelect={onSelect} />
      ))}
    </div>
  );
}

function ApiTreeNodeView({
  node,
  activeFocusIds,
  onSelect,
}: {
  node: ApiTreeNode;
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  const [open, setOpen] = useState(true);
  const itemCount = node.items.length + node.children.reduce((total, child) => total + countApiTreeItems(child), 0);
  return (
    <div className="explorer-api-node">
      <button
        className="explorer-api-folder"
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        title={`${node.label} · ${itemCount}개 API`}
      >
        <ChevronDown size={13} className={open ? "" : "closed"} />
        <Folder size={14} />
        <strong>{node.label}</strong>
        <small>{itemCount}</small>
      </button>
      {open ? (
        <div className="explorer-api-children">
          {node.items.map((item) => (
            <ApiEndpointItem
              key={item.id}
              item={item}
              active={activeFocusIds.includes(item.focusId)}
              onSelect={onSelect}
            />
          ))}
          <ApiTreeNodes node={node} activeFocusIds={activeFocusIds} onSelect={onSelect} />
        </div>
      ) : null}
    </div>
  );
}

function ApiEndpointItem({
  item,
  active,
  onSelect,
}: {
  item: TargetItem;
  active: boolean;
  onSelect: (item: TargetItem) => void;
}) {
  const segments = apiPathSegments(item.title);
  const leaf = segments[segments.length - 1] ?? "/";
  return (
    <button
      className={`explorer-tree-item explorer-api-endpoint ${active ? "active" : ""}`}
      aria-label={item.title}
      aria-current={active ? "true" : undefined}
      type="button"
      onClick={() => onSelect(item)}
      title={`${item.title} · ${item.meta}`}
    >
      <span className="explorer-dot api" aria-hidden="true" />
      <span className="explorer-item-copy">
        <strong>
          {item.badge} /{leaf}
        </strong>
        <small>{item.meta}</small>
      </span>
    </button>
  );
}

function countApiTreeItems(node: ApiTreeNode): number {
  return node.items.length + node.children.reduce((total, child) => total + countApiTreeItems(child), 0);
}

function CodeTreeItems({
  items,
  activeFocusIds,
  onSelect,
}: {
  items: TargetItem[];
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  if (items.length === 0) return <p className="explorer-empty">표시할 코드가 없습니다.</p>;
  const visibleItems = items.slice(0, CODE_TREE_DISPLAY_LIMIT);
  return (
    <div className="explorer-code-tree">
      <CodeTreeNodes node={buildCodeTree(visibleItems)} activeFocusIds={activeFocusIds} onSelect={onSelect} />
      {items.length > CODE_TREE_DISPLAY_LIMIT ? (
        <p className="explorer-more">
          상위 {CODE_TREE_DISPLAY_LIMIT}개만 표시합니다. 검색으로 나머지 코드를 찾을 수 있습니다.
        </p>
      ) : null}
    </div>
  );
}

function CodeTreeNodes({
  node,
  activeFocusIds,
  onSelect,
}: {
  node: CodeTreeNode;
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  return (
    <div className="explorer-code-tree-branch">
      {node.children.map((child) => (
        <CodeTreeNodeView key={child.key} node={child} activeFocusIds={activeFocusIds} onSelect={onSelect} />
      ))}
    </div>
  );
}

function CodeTreeNodeView({
  node,
  activeFocusIds,
  onSelect,
}: {
  node: CodeTreeNode;
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  const [open, setOpen] = useState(true);
  const itemCount = countCodeTreeItems(node);
  const Icon = node.isFile ? FileCode2 : Folder;
  return (
    <div className={`explorer-code-node ${node.isFile ? "file" : "folder"}`}>
      <button
        className="explorer-code-folder"
        type="button"
        aria-label={`${node.label}, ${itemCount}개 코드 항목`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        title={`${node.label} · ${itemCount}개 코드 항목`}
      >
        <ChevronDown size={13} className={open ? "" : "closed"} />
        <Icon size={14} />
        <strong>{node.label}</strong>
        <small>{itemCount}</small>
      </button>
      {open ? (
        <div className="explorer-code-children">
          {node.items.length > 0 ? (
            <TargetTreeItems items={node.items} activeFocusIds={activeFocusIds} onSelect={onSelect} />
          ) : null}
          <CodeTreeNodes node={node} activeFocusIds={activeFocusIds} onSelect={onSelect} />
        </div>
      ) : null}
    </div>
  );
}

function TargetTreeItems({
  items,
  activeFocusIds,
  onSelect,
}: {
  items: TargetItem[];
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  const [rovingFocusId, setRovingFocusId] = useState(items[0]?.id ?? null);

  useEffect(() => {
    if (!items.some((item) => item.id === rovingFocusId)) {
      setRovingFocusId(items[0]?.id ?? null);
    }
  }, [items, rovingFocusId]);

  if (items.length === 0) {
    return <p className="explorer-empty">표시할 항목이 없습니다.</p>;
  }
  return (
    <div className="explorer-tree-items">
      {items.slice(0, 80).map((item, index) => (
        <button
          aria-current={activeFocusIds.includes(item.focusId) ? "true" : undefined}
          className={`explorer-tree-item ${activeFocusIds.includes(item.focusId) ? "active" : ""}`}
          data-target-id={item.focusId}
          key={item.id}
          tabIndex={rovingFocusId === item.id ? 0 : -1}
          type="button"
          onClick={() => onSelect(item)}
          onFocus={() => setRovingFocusId(item.id)}
          onKeyDown={(event) => {
            if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
            event.preventDefault();
            const nextIndex = index + (event.key === "ArrowDown" ? 1 : -1);
            const next = items[nextIndex];
            if (next) {
              setRovingFocusId(next.id);
              Array.from(document.querySelectorAll<HTMLButtonElement>("[data-target-id]"))
                .find((button) => button.dataset.targetId === next.focusId)
                ?.focus();
            }
          }}
          title={`${item.title} · ${item.meta}`}
        >
          <span className={`explorer-dot ${item.kind}`} aria-hidden="true" />
          <span className="explorer-item-copy">
            <strong>
              {item.badge} {item.title}
            </strong>
            <small>{item.meta}</small>
          </span>
        </button>
      ))}
      {items.length > 80 ? <small className="explorer-more">… {items.length - 80}개 더 있음</small> : null}
    </div>
  );
}

function TreeSection({
  icon,
  label,
  count,
  children,
}: {
  icon: ReactNode;
  label: string;
  count: number;
  children: ReactNode;
}) {
  return (
    <section className="explorer-tree-section">
      <header>
        <ChevronDown size={14} />
        {icon}
        <strong>{label}</strong>
        <small>{count}</small>
      </header>
      {children}
    </section>
  );
}

function filteredItems(catalog: TargetCatalog, kind: "api" | "code" | "table", query: string): TargetItem[] {
  const items = catalog[kind];
  if (!query) return items;
  return items.filter((item) =>
    [item.badge, item.title, item.meta, item.group].some((value) => value?.toLocaleLowerCase("ko-KR").includes(query)),
  );
}

function countForTab(tab: Exclude<ExplorerTab, "project">, catalog: TargetCatalog): number {
  return tab === "db" ? catalog.table.length : catalog[tab].length;
}
