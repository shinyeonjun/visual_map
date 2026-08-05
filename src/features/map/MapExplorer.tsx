import { Braces, ChevronDown, Code2, Cloud, Database, GitBranch, Layers, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { useSearchHotkey } from "../../hooks/useSearchHotkey";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { codeInventoryRouteCount, codeInventorySymbolCount } from "../../types/workspace";
import { searchScopeText } from "../../visual/search";
import { SearchResultsPopover, focusFirstSearchResult } from "../../components/common/SearchResultsPopover";
import { buildTargetCatalog, type TargetCatalog, type TargetItem } from "./targetModel";

const LAYER_PREVIEW_LIMIT = 4;

/**
 * The layers panel: one scrollable tree of everything the map knows, stacked as
 * sections instead of tabs. Tabs hid three quarters of the project behind a
 * click; a stack keeps every kind visible at once, which is what a layers
 * panel is for.
 */
export function MapExplorer({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onOpenDatabase,
  onOpenSources,
  onSelectTarget,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onOpenDatabase: () => void;
  onOpenSources?: () => void;
  onSelectTarget?: (item: TargetItem) => void;
}) {
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
  const [revealedLayer, setRevealedLayer] = useState<CanvasRevealLayer | null>(null);
  const activeFocusIds = [visualMapControls.focusId ?? visualMapControls.currentMap?.focus].filter(
    (value): value is string => Boolean(value),
  );
  const apiItems = filteredItems(catalog, "api", normalizedQuery);
  const codeItems = filteredItems(catalog, "code", normalizedQuery);
  const tableItems = filteredItems(catalog, "table", normalizedQuery);
  const apiCount = normalizedQuery ? apiItems.length : codeInventoryRouteCount(workspaceControls.codeInventory);
  const codeCount = normalizedQuery ? codeItems.length : codeInventorySymbolCount(workspaceControls.codeInventory);
  const externalCalls = externalCallPreview(workspaceControls.codeInventory);

  return (
    <aside className="project-explorer" aria-label="레이어">
      <header className="layers-title">
        <Layers size={14} />
        <strong>레이어</strong>
      </header>

      <div className="project-explorer-search">
        <Search size={14} aria-hidden="true" />
        <input
          ref={searchInputRef}
          id="layer-search"
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
        <LayerSection
          icon={<Braces size={14} />}
          kind="api"
          label="API 라우트"
          count={apiCount}
          revealed={revealedLayer === "api"}
          onReveal={() => toggleReveal("api")}
        >
          <ApiTreeItems
            items={apiItems}
            totalCount={apiCount}
            activeFocusIds={activeFocusIds}
            onSelect={selectTarget}
          />
        </LayerSection>
        <LayerSection
          icon={<Code2 size={14} />}
          kind="code"
          label="코드"
          count={codeCount}
          revealed={revealedLayer === "code"}
          onReveal={() => toggleReveal("code")}
        >
          <CodeTreeItems
            items={codeItems}
            totalCount={codeCount}
            activeFocusIds={activeFocusIds}
            onSelect={selectTarget}
          />
        </LayerSection>
        <LayerSection
          icon={<Database size={14} />}
          kind="db"
          label="데이터베이스"
          count={tableItems.length}
          revealed={revealedLayer === "db"}
          onReveal={() => toggleReveal("db")}
        >
          <TargetTreeItems items={tableItems} activeFocusIds={activeFocusIds} onSelect={selectTarget} />
          {tableItems.length === 0 && !dbProfileControls.inventory ? (
            <button className="explorer-empty-action" type="button" onClick={onOpenDatabase}>
              DB 연결
            </button>
          ) : null}
        </LayerSection>
        {externalCalls.length > 0 ? (
          <LayerSection
            icon={<Cloud size={14} />}
            kind="external"
            label="외부 호출"
            count={externalCalls.reduce((total, item) => total + item.count, 0)}
            revealed={revealedLayer === "external"}
            onReveal={() => toggleReveal("external")}
          >
            <div className="explorer-external-items">
              {externalCalls.slice(0, LAYER_PREVIEW_LIMIT).map((item) => (
                <div className="explorer-external-item" key={item.client}>
                  <span className="explorer-external-dot" aria-hidden="true" />
                  <strong title={item.client}>{item.client}</strong>
                  <small>{item.count}</small>
                </div>
              ))}
              {externalCalls.length > LAYER_PREVIEW_LIMIT ? (
                <small className="explorer-more">… {externalCalls.length - LAYER_PREVIEW_LIMIT}개 더보기</small>
              ) : null}
            </div>
          </LayerSection>
        ) : null}
      </div>

      <footer className="source-links" aria-label="소스와 연결">
        <div className="source-links-heading">
          <strong>소스 &amp; 연결</strong>
        </div>
        <button className="source-link-row" type="button" onClick={onOpenSources} disabled={!onOpenSources}>
          <GitBranch size={13} />
          <span>
            <strong>Git 저장소</strong>
            <small>{currentWorkspace?.name ?? "프로젝트 없음"}</small>
          </span>
          <em data-connected={currentWorkspace ? "true" : "false"}>{currentWorkspace ? "연결됨" : "미연결"}</em>
        </button>
        <button className="source-link-row" type="button" onClick={onOpenDatabase}>
          <Database size={13} />
          <span>
            <strong>데이터베이스</strong>
            <small>{dbProfileControls.activeProfile?.name ?? "프로필 없음"}</small>
          </span>
          <em data-connected={dbProfileControls.inventory ? "true" : "false"}>
            {dbProfileControls.inventory ? "연결됨" : "미연결"}
          </em>
        </button>
      </footer>
    </aside>
  );

  function selectTarget(item: TargetItem) {
    setRevealedLayer(targetRevealLayer(item));
    onSelectTarget?.(item);
  }

  function toggleReveal(layer: CanvasRevealLayer) {
    setRevealedLayer((current) => (current === layer ? null : layer));
  }
}

type CanvasRevealLayer = "api" | "code" | "db" | "external";

function LayerSection({
  icon,
  kind,
  label,
  count,
  revealed = false,
  onReveal,
  children,
}: {
  icon: ReactNode;
  kind: string;
  label: string;
  count: number;
  revealed?: boolean;
  onReveal?: () => void;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(true);
  return (
    <section className="layer-section" data-layer={kind}>
      <button
        className="layer-section-header"
        type="button"
        aria-expanded={open}
        data-revealed={revealed ? "true" : undefined}
        onClick={() => {
          setOpen((value) => !value);
          onReveal?.();
        }}
      >
        <ChevronDown size={13} className={open ? "" : "closed"} />
        {icon}
        <strong>{label}</strong>
        <small>{count}</small>
      </button>
      {open ? <div className="layer-section-body">{children}</div> : null}
    </section>
  );
}

function targetRevealLayer(item: TargetItem): CanvasRevealLayer {
  if (item.kind === "api") return "api";
  if (item.kind === "table" || item.kind === "column") return "db";
  return "code";
}

function ApiTreeItems({
  items,
  totalCount,
  activeFocusIds,
  onSelect,
}: {
  items: TargetItem[];
  totalCount: number;
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  if (items.length === 0) return <p className="explorer-empty">표시할 API가 없습니다.</p>;
  return (
    <div className="explorer-tree-items">
      {items.slice(0, LAYER_PREVIEW_LIMIT).map((item) => (
        <ApiEndpointItem key={item.id} item={item} active={activeFocusIds.includes(item.focusId)} onSelect={onSelect} />
      ))}
      {totalCount > LAYER_PREVIEW_LIMIT ? (
        <p className="explorer-more">… {totalCount - LAYER_PREVIEW_LIMIT}개 더보기 · 검색으로 좁히기</p>
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
          {item.badge} {item.title}
        </strong>
        <small>{item.meta}</small>
      </span>
    </button>
  );
}

function CodeTreeItems({
  items,
  totalCount,
  activeFocusIds,
  onSelect,
}: {
  items: TargetItem[];
  totalCount: number;
  activeFocusIds: string[];
  onSelect: (item: TargetItem) => void;
}) {
  if (items.length === 0) return <p className="explorer-empty">표시할 코드가 없습니다.</p>;
  // TargetTreeItems already reports the remainder; repeating it here printed the
  // same "…N개 더보기" line twice under every code section.
  return <TargetTreeItems items={items} totalCount={totalCount} activeFocusIds={activeFocusIds} onSelect={onSelect} />;
}

function TargetTreeItems({
  items,
  totalCount = items.length,
  activeFocusIds,
  onSelect,
}: {
  items: TargetItem[];
  totalCount?: number;
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
      {items.slice(0, LAYER_PREVIEW_LIMIT).map((item, index) => (
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
      {totalCount > LAYER_PREVIEW_LIMIT ? (
        <small className="explorer-more">… {totalCount - LAYER_PREVIEW_LIMIT}개 더보기 · 검색으로 좁히기</small>
      ) : null}
    </div>
  );
}

function filteredItems(catalog: TargetCatalog, kind: "api" | "code" | "table", query: string): TargetItem[] {
  const items = catalog[kind];
  if (!query) return items;
  return items.filter((item) =>
    [item.badge, item.title, item.meta, item.group].some((value) => value?.toLocaleLowerCase("ko-KR").includes(query)),
  );
}

function externalCallPreview(
  codeInventory: WorkspaceControls["codeInventory"],
): Array<{ client: string; count: number }> {
  const counts = new Map<string, number>();
  for (const request of codeInventory?.clientRequests ?? []) {
    if (request.resolution === "excluded") continue;
    const client = request.client.trim() || request.rawUrl.trim() || "외부 서비스";
    counts.set(client, (counts.get(client) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([client, count]) => ({ client, count }))
    .sort((left, right) => right.count - left.count || left.client.localeCompare(right.client, "ko-KR"));
}
