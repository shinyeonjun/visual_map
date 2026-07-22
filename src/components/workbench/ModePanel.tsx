import {
  Braces,
  CircleCheck,
  Database,
  FileCode2,
  FolderCog,
  GitCompareArrows,
  LayoutGrid,
  ListFilter,
  Network,
  Search,
  TriangleAlert,
  X,
} from "lucide-react";
import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ComponentType } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import type { VisualMap } from "../../types/visual-map";
import { savedModeMapContext } from "../../visual/mapContext";
import {
  columnRefFromNodeId,
  dbColumnNodeId,
  dbTableIdentityLabel,
  dbTableNodeId,
} from "../../visual/nodeIds";
import {
  codeInventoryCodeItems,
  codeInventoryFileCount,
  codeInventoryItemCount,
  codeInventoryRouteCount,
  codeInventorySymbolCount,
  codeKindChip,
  codeRouteMethod,
  dbInventoryTableCount,
  dbInventoryTableKey,
} from "../../types/workspace";

type ModeIcon = ComponentType<{ size?: number }>;

type ModeContextItem = {
  id: string;
  badge: string;
  title: string;
  meta: string;
  group?: string;
  active: boolean;
  selectable?: boolean;
  open: () => void;
};

type ModeContext = {
  title: string;
  description: string;
  total: number;
  matching: number;
  items: ModeContextItem[];
};

const workbenchModes: [ModeIcon, string, string, string][] = [
  [LayoutGrid, "atlas", "개요", "전체 구조"],
  [Braces, "api-flow", "API", "라우트부터 DB까지"],
  [FileCode2, "search-focus", "코드", "함수·클래스·파일"],
  [Database, "table-usage", "DB", "테이블·컬럼·제약"],
  [GitCompareArrows, "column-impact", "영향", "직접·후보·미확인"],
  [Network, "composition", "관계", "선택한 대상만 연결"],
];

export function ModePanel({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  onNavigate,
  onOpenSources,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  onNavigate?: () => void;
  onOpenSources?: () => void;
}) {
  const [blockedReason, setBlockedReason] = useState<string | null>(null);
  const [contextQueries, setContextQueries] = useState<Record<string, string>>({});
  const [compactContextOpen, setCompactContextOpen] = useState(false);
  const contextListRef = useRef<HTMLDivElement | null>(null);
  const contextFilterRef = useRef<HTMLInputElement | null>(null);
  const contextToggleRef = useRef<HTMLButtonElement | null>(null);
  const restoreContextFocusRef = useRef(false);
  const contextScrollRef = useRef(new Map<string, number>());
  const atlasContextMapRef = useRef<{ workspaceId: string | null; map: VisualMap | null }>({
    workspaceId: null,
    map: null,
  });
  const counts = navigationCounts(workspaceControls, dbProfileControls);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const workspaceId = workspaceControls.currentWorkspace?.id ?? null;
  const cachedAtlasMap = atlasContextMapRef.current.workspaceId === workspaceId
    ? atlasContextMapRef.current.map
    : null;
  const visibleMode = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.mode
    : visualMapControls.mode;
  const pendingMode = visualMapControls.loading && visibleMode !== visualMapControls.mode
    ? visualMapControls.mode
    : null;
  const contextQuery = contextQueries[visibleMode] ?? "";
  const context = modeContext(
    workspaceControls,
    dbProfileControls,
    visualMapControls,
    visibleMode,
    contextQuery,
    cachedAtlasMap,
  );

  useLayoutEffect(() => {
    if (contextListRef.current) {
      contextListRef.current.scrollTop = contextScrollRef.current.get(visibleMode) ?? 0;
    }
  }, [visibleMode]);

  useEffect(() => {
    atlasContextMapRef.current = {
      workspaceId,
      map: visualMapControls.currentMap?.mode === "atlas"
        ? visualMapControls.currentMap
        : atlasContextMapRef.current.workspaceId === workspaceId
          ? atlasContextMapRef.current.map
          : null,
    };
  }, [visualMapControls.currentMap, workspaceId]);

  useEffect(() => {
    if (compactContextOpen) {
      contextFilterRef.current?.focus();
    }
  }, [compactContextOpen]);

  useLayoutEffect(() => {
    if (!compactContextOpen && restoreContextFocusRef.current) {
      restoreContextFocusRef.current = false;
      contextToggleRef.current?.focus();
    }
  }, [compactContextOpen]);

  const closeCompactContext = (restoreFocus = false) => {
    restoreContextFocusRef.current = restoreFocus;
    setCompactContextOpen(false);
  };

  return (
    <section className="product-nav-panel">
      <header className="product-nav-project">
        <span className="project-avatar" aria-hidden="true">
          {(workspaceControls.currentWorkspace?.name ?? "P").slice(0, 1).toUpperCase()}
        </span>
        <span>
          <strong>{workspaceControls.currentWorkspace?.name ?? (workspaceControls.initialized ? "프로젝트 없음" : "프로젝트 확인 중")}</strong>
          <small>{hasWorkspace ? "분석 대상" : workspaceControls.initialized ? "소스를 연결하세요" : "잠시만 기다려 주세요"}</small>
        </span>
      </header>
      <div className="product-nav-body">
        <nav className="product-nav-list" aria-label="프로젝트 보기">
          {workbenchModes.map(([ModeIcon, mode, title, description]) => {
            const blockReason = modeBlockReason(mode, workspaceControls, dbProfileControls);
            const active = visibleMode === mode;
            const pending = pendingMode === mode;
            const count = counts[mode] ?? 0;
            return (
              <button
                className={`${active ? "active" : ""} ${pending ? "pending" : ""} ${blockReason ? "locked" : ""}`}
                type="button"
                key={mode}
                data-mode-id={modeTestId(mode)}
                aria-current={active ? "page" : undefined}
                aria-busy={pending || undefined}
                aria-controls={active ? "product-mode-context" : undefined}
                aria-label={`${title}. ${blockReason ?? description}. ${count.toLocaleString("ko-KR")}개${pending ? ". 준비 중" : ""}`}
                title={pending ? `${title} 준비 중` : blockReason ?? description}
                onClick={() => {
                  if (blockReason) {
                    setBlockedReason(blockReason);
                    setCompactContextOpen(false);
                    return;
                  }
                  setBlockedReason(null);
                  setCompactContextOpen(false);
                  if (visualMapControls.mode !== mode) {
                    showWorkbenchMode(mode, workspaceControls, dbProfileControls, visualMapControls);
                  }
                  onNavigate?.();
                }}
              >
                <span className="product-nav-icon">
                  <ModeIcon size={17} />
                </span>
                <span className="product-nav-copy">
                  <strong>{title}</strong>
                </span>
              </button>
            );
          })}
          <button
            className="product-context-toggle"
            type="button"
            ref={contextToggleRef}
            aria-controls="product-mode-context"
            aria-expanded={compactContextOpen}
            aria-label={`${context.title} 목록 ${compactContextOpen ? "닫기" : "열기"}`}
            title={`${context.title} 목록`}
            disabled={!hasWorkspace}
            onClick={() => setCompactContextOpen((open) => !open)}
          >
            <span className="product-nav-icon">
              <ListFilter size={17} />
            </span>
            <span className="product-nav-copy">
              <strong>항목</strong>
            </span>
          </button>
        </nav>

        <div className="product-context-column">
          <section
            className={`product-context-browser${compactContextOpen ? " compact-open" : ""}`}
            id="product-mode-context"
            aria-label={`${context.title} 탐색`}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                closeCompactContext(true);
              }
            }}
          >
            <header>
              <span>
                <strong>{context.title}</strong>
                <small>{context.description}</small>
              </span>
              <em>{context.total.toLocaleString("ko-KR")}</em>
              <button
                className="product-context-close"
                type="button"
                aria-label={`${context.title} 목록 닫기`}
                title="목록 닫기"
                onClick={() => closeCompactContext(true)}
              >
                <X size={14} />
              </button>
            </header>
            <label className="product-context-filter">
              <Search size={13} aria-hidden="true" />
              <input
                aria-label={`${context.title} 항목 필터`}
                ref={contextFilterRef}
                disabled={!hasWorkspace || context.total === 0}
                placeholder={`${context.title} 필터`}
                value={contextQuery}
                onChange={(event) => {
                  contextListRef.current?.scrollTo({ top: 0 });
                  contextScrollRef.current.set(visibleMode, 0);
                  setContextQueries((current) => ({
                    ...current,
                    [visibleMode]: event.target.value,
                  }));
                }}
              />
            </label>
            <div
              className="product-context-list"
              ref={contextListRef}
              onScroll={(event) => contextScrollRef.current.set(visibleMode, event.currentTarget.scrollTop)}
            >
              {context.items.map((item, index) => (
                <Fragment key={item.id}>
                  {item.group && item.group !== context.items[index - 1]?.group ? (
                    <h3>{item.group}</h3>
                  ) : null}
                  {item.selectable ? (
                    <label className={`product-context-option${item.active ? " active" : ""}`} data-context-id={item.id}>
                      <input
                        type="checkbox"
                        checked={item.active}
                        disabled={!item.active && visualMapControls.compositionFocusIds.length >= 8}
                        onChange={() => {
                          setBlockedReason(null);
                          item.open();
                        }}
                      />
                      <span>{item.badge}</span>
                      <strong title={item.title}>{item.title}</strong>
                      {item.active ? <em>선택</em> : null}
                      <small title={item.meta}>{item.meta}</small>
                    </label>
                  ) : (
                    <button
                      className={item.active ? "active" : ""}
                      type="button"
                      data-context-id={item.id}
                      aria-current={item.active ? "true" : undefined}
                      onClick={() => {
                        setBlockedReason(null);
                        setCompactContextOpen(false);
                        item.open();
                        onNavigate?.();
                      }}
                    >
                      <span>{item.badge}</span>
                      <strong title={item.title}>{item.title}</strong>
                      {item.active ? <em>현재</em> : null}
                      <small title={item.meta}>{item.meta}</small>
                    </button>
                  )}
                </Fragment>
              ))}
              {context.items.length === 0 ? (
                <p>
                  {context.total === 0
                    ? hasWorkspace
                      ? "현재 보기에서 탐색할 항목이 없습니다."
                      : "프로젝트를 연결하면 항목이 표시됩니다."
                    : "필터와 일치하는 항목이 없습니다."}
                </p>
              ) : null}
            </div>
            <footer>
              {context.matching > context.items.length
                ? `${context.items.length.toLocaleString("ko-KR")}개 표시 · 검색으로 좁히세요`
                : `${context.matching.toLocaleString("ko-KR")}개 표시`}
            </footer>
          </section>

          {blockedReason && (
            <div className="product-nav-notice" role="status">
              <TriangleAlert size={15} />
              <span>{blockedReason}</span>
              {hasWorkspace && onOpenSources ? (
                <button type="button" onClick={onOpenSources}>
                  소스 관리
                </button>
              ) : null}
            </div>
          )}

          <footer className="product-nav-footer">
            <span className={hasWorkspace ? "ready" : "pending"}>
              {hasWorkspace ? <CircleCheck size={14} /> : <FolderCog size={14} />}
              {hasWorkspace ? "프로젝트 연결됨" : workspaceControls.initialized ? "프로젝트 연결 필요" : "프로젝트 확인 중"}
            </span>
            {hasWorkspace && (
              <small>
                코드 {counts["search-focus"].toLocaleString("ko-KR")} · DB {counts["table-usage"].toLocaleString("ko-KR")}
              </small>
            )}
          </footer>
        </div>
      </div>
    </section>
  );
}

function routeLocation(path: string | null | undefined, line: number | null | undefined): string {
  if (!path) return line ? `L${line}` : "소스 위치 없음";
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  const compact = parts.slice(-2).join("/");
  return `${compact}${line ? `:${line}` : ""}`;
}

function modeContext(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
  mode: string,
  query: string,
  atlasContextMap: VisualMap | null,
): ModeContext {
  const focus = visualMapControls.loading && visualMapControls.currentMap
    ? visualMapControls.currentMap.focus
    : visualMapControls.focusId;
  const normalizedQuery = query.trim().toLocaleLowerCase("ko-KR");
  const finish = (title: string, description: string, items: ModeContextItem[], total = items.length): ModeContext => {
    const filtered = normalizedQuery
      ? items.filter((item) => [item.badge, item.title, item.meta, item.group]
          .some((value) => value?.toLocaleLowerCase("ko-KR").includes(normalizedQuery)))
      : items;
    return {
      title,
      description,
      total,
      matching: normalizedQuery ? filtered.length : total,
      items: filtered.slice(0, 100),
    };
  };
  const activate = (nodeId: string, open: () => void) => () => {
    if (visualMapControls.loading) {
      if (visualMapControls.focusId !== nodeId) {
        open();
      }
      return;
    }
    const node = visualMapControls.currentMap?.nodes.find((item) => item.id === nodeId) ?? null;
    if (focus === nodeId && node) {
      visualMapControls.selectNode(node);
      return;
    }
    open();
  };

  if (mode === "composition") {
    const selected = new Set(visualMapControls.compositionFocusIds);
    const selectable = (
      nodeId: string,
      badge: string,
      title: string,
      meta: string,
      group: string,
    ): ModeContextItem => ({
      id: nodeId,
      badge,
      title,
      meta,
      group,
      active: selected.has(nodeId),
      selectable: true,
      open: () => visualMapControls.toggleCompositionFocus(nodeId),
    });
    const routeItems = (workspaceControls.codeInventory?.routes ?? []).map((route) =>
        selectable(
          `code:${route.id}`,
          codeRouteMethod(route) ?? "API",
          route.name,
          routeLocation(route.filePath, route.line),
          "API 라우트",
        ),
      );
    const symbolItems = codeInventoryCodeItems(workspaceControls.codeInventory).map((item) =>
        selectable(
          `code:${item.id}`,
          codeKindChip(item.kind),
          item.name,
          routeLocation(item.filePath, item.line),
          "코드",
        ),
      );
    const fileItems = (workspaceControls.codeInventory?.files ?? []).map((item) =>
        selectable(
          `code:${item.id}`,
          "FILE",
          item.name,
          routeLocation(item.filePath, item.line),
          "파일",
        ),
      );
    const tables = dbProfileControls.inventory?.tables ?? [];
    const tableItems = tables.map((table) => {
      const tableKey = dbInventoryTableKey(table);
      return selectable(
        dbTableNodeId(tableKey),
        "TABLE",
        dbTableIdentityLabel(tableKey),
        `컬럼 ${table.columns.length.toLocaleString("ko-KR")}개`,
        "DB 테이블",
      );
    });
    const columnItems = tables.flatMap((table) => {
      const tableKey = dbInventoryTableKey(table);
      return table.columns.map((column) =>
          selectable(
            dbColumnNodeId(tableKey, column.name),
            column.isPrimaryKey ? "PK" : column.isForeignKey ? "FK" : "COL",
            column.name,
            column.dataType ?? "타입 정보 없음",
            `컬럼 · ${dbTableIdentityLabel(tableKey)}`,
          ),
        );
    });
    const groups = [routeItems, symbolItems, fileItems, tableItems, columnItems]
      .map((items) => [...new Map(items.map((item) => [item.id, item])).values()]);
    const allItems = [...new Map(groups.flat().map((item) => [item.id, item])).values()];
    const visibleItems = normalizedQuery ? allItems : balancedCompositionItems(groups, 100);
    return finish(
      "분석 대상",
      `${selected.size}/8 선택 · 2개부터 관계 표시`,
      visibleItems,
      allItems.length,
    );
  }

  if (mode === "api-flow") {
    const routes = workspaceControls.codeInventory?.routes ?? [];
    return finish(
      "API 라우트",
      "요청 경로 선택",
      routes.map((route) => ({
        id: route.id,
        badge: codeRouteMethod(route) ?? "API",
        title: route.name,
        meta: routeLocation(route.filePath, route.line),
        active: focus === `code:${route.id}`,
        open: activate(`code:${route.id}`, () => visualMapControls.showMode("api-flow", `code:${route.id}`)),
      })),
      codeInventoryRouteCount(workspaceControls.codeInventory),
    );
  }

  if (mode === "search-focus") {
    const toItem = (group: string) => (item: ReturnType<typeof codeInventoryCodeItems>[number]): ModeContextItem => ({
      id: item.id,
      badge: codeKindChip(item.kind),
      title: item.name,
      meta: routeLocation(item.filePath, item.line),
      group,
      active: focus === `code:${item.id}`,
      open: activate(`code:${item.id}`, () => visualMapControls.showMode("search-focus", `code:${item.id}`)),
    });
    const codeItems = [
      ...(workspaceControls.codeInventory?.routes ?? []).map(toItem("API 라우트")),
      ...codeInventoryCodeItems(workspaceControls.codeInventory).map(toItem("함수·클래스")),
      ...(workspaceControls.codeInventory?.files ?? []).map(toItem("파일")),
    ];
    return finish(
      "코드 항목",
      "함수·클래스·파일",
      codeItems,
      codeInventoryItemCount(workspaceControls.codeInventory),
    );
  }

  const tables = dbProfileControls.inventory?.tables ?? [];
  if (mode === "table-usage") {
    return finish(
      "DB 테이블",
      "사용처와 제약",
      tables.map((table) => {
        const tableKey = dbInventoryTableKey(table);
        return {
          id: tableKey,
          badge: "TABLE",
          title: dbTableIdentityLabel(tableKey),
          meta: `컬럼 ${table.columns.length.toLocaleString("ko-KR")}개`,
          active: focus === dbTableNodeId(tableKey),
          open: activate(dbTableNodeId(tableKey), () => dbProfileControls.openTable(tableKey)),
        };
      }),
      dbInventoryTableCount(dbProfileControls.inventory),
    );
  }

  if (mode === "column-impact") {
    return finish(
      "DB 컬럼",
      "영향을 확인할 컬럼",
      tables.flatMap((table) => {
        const tableKey = dbInventoryTableKey(table);
        return table.columns.map((column) => ({
            id: dbColumnNodeId(tableKey, column.name),
            badge: column.isPrimaryKey ? "PK" : column.isForeignKey ? "FK" : "COL",
            title: column.name,
            meta: column.dataType ?? "타입 정보 없음",
            group: dbTableIdentityLabel(tableKey),
            active: focus === dbColumnNodeId(tableKey, column.name),
            open: activate(
              dbColumnNodeId(tableKey, column.name),
              () => dbProfileControls.openColumn(tableKey, column.name),
            ),
          }));
      }),
    );
  }

  const atlasMap = visualMapControls.currentMap?.mode === "atlas"
    ? visualMapControls.currentMap
    : atlasContextMap?.mode === "atlas"
      ? atlasContextMap
      : null;
  const groups = atlasMap?.nodes.filter(
    (node) => node.kind === "group-domain" || node.id.startsWith("group:"),
  ) ?? [];
  return finish(
    "구조 영역",
    "먼저 읽을 경계",
    groups.map((node) => ({
      id: node.id,
      badge: "영역",
      title: node.title,
      meta: node.subtitle ?? "구조 상세",
      active: focus === node.id,
      open: activate(node.id, () => visualMapControls.showMode("atlas", node.id)),
    })),
  );
}

function balancedCompositionItems(groups: ModeContextItem[][], limit: number): ModeContextItem[] {
  const selectedCount = groups.reduce(
    (total, items) => total + items.filter((item) => item.active).length,
    0,
  );
  const allocations = groups.map((items) => items.filter((item) => item.active).length);
  let remaining = Math.max(0, limit - selectedCount);
  while (remaining > 0) {
    let added = false;
    for (let index = 0; index < groups.length && remaining > 0; index += 1) {
      if (allocations[index] >= groups[index].length) continue;
      allocations[index] += 1;
      remaining -= 1;
      added = true;
    }
    if (!added) break;
  }
  return groups.flatMap((items, index) => {
    const selected = items.filter((item) => item.active);
    const unselected = items.filter((item) => !item.active);
    return [...selected, ...unselected.slice(0, allocations[index] - selected.length)];
  });
}

function modeTestId(mode: string): string {
  if (mode === "composition") return "composition";
  if (mode === "api-flow") return "api";
  if (mode === "table-usage") return "dependencies";
  if (mode === "column-impact") return "impact";
  if (mode === "search-focus") return "search";
  return "atlas";
}

function navigationCounts(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
): Record<string, number> {
  const codeItems = codeInventorySymbolCount(workspaceControls.codeInventory);
  const routes = codeInventoryRouteCount(workspaceControls.codeInventory);
  const files = codeInventoryFileCount(workspaceControls.codeInventory);
  const tables = dbProfileControls.inventory?.tables ?? [];
  const tableCount = dbInventoryTableCount(dbProfileControls.inventory);
  return {
    atlas: codeInventoryItemCount(workspaceControls.codeInventory) + tableCount,
    "api-flow": routes,
    "search-focus": routes + codeItems + files,
    "table-usage": tableCount,
    "column-impact": tables.reduce((total, table) => total + table.columns.length, 0),
    composition: compositionItemCount(workspaceControls, dbProfileControls),
  };
}

function modeBlockReason(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
): string | null {
  if (!workspaceControls.initialized) {
    return "프로젝트를 확인하고 있습니다.";
  }
  if (!workspaceControls.currentWorkspace) {
    return "프로젝트를 먼저 연결하세요.";
  }
  if (mode === "atlas") {
    return null;
  }
  if (
    mode === "composition"
    && compositionItemCount(workspaceControls, dbProfileControls) < 2
  ) {
    return "코드와 DB에서 관계를 볼 대상을 2개 이상 읽어야 합니다.";
  }
  if (mode === "api-flow") {
    if (!workspaceControls.codeInventory) return "코드를 먼저 읽어야 API 경로를 볼 수 있습니다.";
    if (codeInventoryRouteCount(workspaceControls.codeInventory) === 0) return "읽은 코드에서 API 라우트를 찾지 못했습니다.";
  }
  if (mode === "search-focus" && codeInventoryItemCount(workspaceControls.codeInventory) === 0) {
    return "코드나 파일을 먼저 읽어야 합니다.";
  }
  if (mode === "table-usage" && !dbProfileControls.inventory?.tables.length) {
    return "DB 또는 DDL을 연결해야 테이블을 볼 수 있습니다.";
  }
  if (mode === "column-impact" && !firstColumn(dbProfileControls)) {
    return "컬럼 구조를 읽어야 변경 영향을 볼 수 있습니다.";
  }
  return null;
}

function compositionItemCount(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
): number {
  const tables = dbProfileControls.inventory?.tables ?? [];
  return codeInventoryItemCount(workspaceControls.codeInventory)
    + dbInventoryTableCount(dbProfileControls.inventory)
    + tables.reduce((total, table) => total + table.columns.length, 0);
}

function showWorkbenchMode(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
) {
  if (mode === "composition") {
    visualMapControls.showMode(mode, null);
    return;
  }
  const workspaceId = workspaceControls.currentWorkspace?.id;
  const savedContext = workspaceId ? savedModeMapContext(workspaceId, mode) : null;
  if (
    savedContext &&
    (!savedContext.focusId || modeFocusExists(savedContext.focusId, mode, workspaceControls, dbProfileControls))
  ) {
    visualMapControls.showMode(mode, savedContext.focusId);
    return;
  }
  visualMapControls.showMode(mode, null);
}

function modeFocusExists(
  focusId: string | null,
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
): boolean {
  if (mode === "atlas" || mode === "explore") {
    return true;
  }
  if (!focusId) {
    return false;
  }
  if (focusId.startsWith("code:")) {
    const codeId = focusId.slice("code:".length);
    const inventory = workspaceControls.codeInventory;
    return Boolean(
      inventory?.routes.some((item) => item.id === codeId) ||
        codeInventoryCodeItems(inventory).some((item) => item.id === codeId) ||
        inventory?.files.some((item) => item.id === codeId),
    );
  }
  if (focusId.startsWith("db:table:")) {
    const tableKey = focusId.slice("db:table:".length);
    return Boolean(dbProfileControls.inventory?.tables.some((table) => dbInventoryTableKey(table) === tableKey));
  }
  if (focusId.startsWith("db:column:")) {
    const ref = columnRefFromNodeId(focusId);
    return Boolean(
      ref &&
        dbProfileControls.inventory?.tables.some(
          (table) =>
            dbInventoryTableKey(table) === ref.tableKey &&
            table.columns.some((column) => column.name === ref.columnName),
        ),
    );
  }
  return false;
}

function firstColumn(dbProfileControls: DbProfileControls): { tableKey: string; columnName: string } | null {
  const tables = dbProfileControls.inventory?.tables ?? [];
  const selectedTable =
    (dbProfileControls.selectedTableKey &&
      tables.find((table) => dbInventoryTableKey(table) === dbProfileControls.selectedTableKey)) ||
    null;
  const table = selectedTable?.columns.length
    ? selectedTable
    : tables.find((item) => item.columns.length > 0) ?? null;
  const column = table?.columns[0];
  return table && column ? { tableKey: dbInventoryTableKey(table), columnName: column.name } : null;
}
