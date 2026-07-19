import {
  Braces,
  CircleCheck,
  Database,
  FileCode2,
  FolderCog,
  GitCompareArrows,
  LayoutGrid,
  TriangleAlert,
} from "lucide-react";
import { useState } from "react";
import type { ComponentType } from "react";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import {
  codeInventoryCodeItems,
  codeInventoryDefaultRoute,
  codeInventoryItemCount,
  dbInventoryTableKey,
} from "../../types/workspace";

type ModeIcon = ComponentType<{ size?: number }>;

const workbenchModes: [ModeIcon, string, string, string][] = [
  [LayoutGrid, "atlas", "개요", "전체 구조"],
  [Braces, "api-flow", "API", "라우트부터 DB까지"],
  [FileCode2, "search-focus", "코드", "함수·클래스·파일"],
  [Database, "table-usage", "데이터베이스", "테이블·컬럼·제약"],
  [GitCompareArrows, "column-impact", "변경 영향", "직접·후보·미확인"],
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
  const counts = navigationCounts(workspaceControls, dbProfileControls);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);

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

      <nav className="product-nav-list" aria-label="프로젝트 보기">
        {workbenchModes.map(([ModeIcon, mode, title, description]) => {
          const blockReason = modeBlockReason(mode, workspaceControls, dbProfileControls);
          const active = visualMapControls.mode === mode;
          const count = counts[mode] ?? null;
          return (
            <button
              className={`${active ? "active" : ""} ${blockReason ? "locked" : ""}`}
              type="button"
              key={mode}
              data-mode-id={modeTestId(mode)}
              aria-current={active ? "page" : undefined}
              aria-label={`${title}. ${blockReason ?? description}`}
              title={blockReason ?? description}
              onClick={() => {
                if (blockReason) {
                  setBlockedReason(blockReason);
                  return;
                }
                if (active) {
                  setBlockedReason(null);
                  onNavigate?.();
                  return;
                }
                setBlockedReason(null);
                showWorkbenchMode(mode, workspaceControls, dbProfileControls, visualMapControls);
                onNavigate?.();
              }}
            >
              <span className="product-nav-icon">
                <ModeIcon size={17} />
              </span>
              <span className="product-nav-copy">
                <strong>{title}</strong>
                <small>{description}</small>
              </span>
              {count !== null && <em>{count.toLocaleString("ko-KR")}</em>}
            </button>
          );
        })}
      </nav>

      {blockedReason && (
        <div className="product-nav-notice" role="status">
          <TriangleAlert size={15} />
          <span>{blockedReason}</span>
          <button type="button" onClick={onOpenSources}>
            소스 관리
          </button>
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
    </section>
  );
}

function modeTestId(mode: string): string {
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
  const codeItems = codeInventoryCodeItems(workspaceControls.codeInventory).length;
  const files = workspaceControls.codeInventory?.files.length ?? 0;
  const tables = dbProfileControls.inventory?.tables ?? [];
  return {
    atlas: codeInventoryItemCount(workspaceControls.codeInventory) + tables.length,
    "api-flow": workspaceControls.codeInventory?.routes.length ?? 0,
    "search-focus": codeItems + files,
    "table-usage": tables.length,
    "column-impact": tables.reduce((total, table) => total + table.columns.length, 0),
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
  if (mode === "api-flow") {
    if (!workspaceControls.codeInventory) return "코드를 먼저 읽어야 API 경로를 볼 수 있습니다.";
    if (workspaceControls.codeInventory.routes.length === 0) return "읽은 코드에서 API 라우트를 찾지 못했습니다.";
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

function showWorkbenchMode(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
) {
  if (mode === "api-flow") {
    const route = codeInventoryDefaultRoute(
      workspaceControls.codeInventory,
      workspaceControls.selectedCodeItem?.id,
    );
    visualMapControls.showMode(mode, route ? `code:${route.id}` : null);
    return;
  }

  if (mode === "table-usage") {
    const tableKey = dbProfileControls.selectedTableKey ?? firstTableKey(dbProfileControls);
    visualMapControls.showMode(mode, tableKey ? `db:table:${tableKey}` : null);
    return;
  }

  if (mode === "column-impact") {
    visualMapControls.showMode(mode, firstColumnFocusId(dbProfileControls, visualMapControls));
    return;
  }

  if (mode === "search-focus") {
    const focusId =
      (visualMapControls.selectedNode?.source === "code" ? visualMapControls.selectedNode.id : null) ??
      (workspaceControls.selectedCodeItem ? `code:${workspaceControls.selectedCodeItem.id}` : null) ??
      firstCodeFocusId(workspaceControls) ??
      (dbProfileControls.selectedTableKey ? `db:table:${dbProfileControls.selectedTableKey}` : null) ??
      firstTableFocusId(dbProfileControls);
    visualMapControls.showMode(mode, focusId);
    return;
  }

  visualMapControls.showMode(mode);
}

function firstCodeFocusId(workspaceControls: WorkspaceControls): string | null {
  const item =
    codeInventoryDefaultRoute(workspaceControls.codeInventory) ??
    codeInventoryCodeItems(workspaceControls.codeInventory)[0] ??
    workspaceControls.codeInventory?.files[0] ??
    null;
  return item ? `code:${item.id}` : null;
}

function firstTableKey(dbProfileControls: DbProfileControls): string | null {
  const table = dbProfileControls.inventory?.tables[0];
  return table ? dbInventoryTableKey(table) : null;
}

function firstTableFocusId(dbProfileControls: DbProfileControls): string | null {
  const tableKey = firstTableKey(dbProfileControls);
  return tableKey ? `db:table:${tableKey}` : null;
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

function firstColumnFocusId(
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): string | null {
  if (visualMapControls.selectedNode?.kind === "column") {
    return visualMapControls.selectedNode.id;
  }
  const column = firstColumn(dbProfileControls);
  return column ? `db:column:${column.tableKey}:${column.columnName}` : null;
}
