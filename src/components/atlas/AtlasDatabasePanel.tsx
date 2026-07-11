import {
  Database,
  Search,
  Table2,
} from "lucide-react";
import type { ReactNode } from "react";
import { useState } from "react";
import type { EngineRegistry } from "../../types/engine";
import { dbInventoryTableKey, dbProfileSourceLabel } from "../../types/workspace";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { focusDbProfileSetup, focusSourceSetup } from "../common/focusSourceSetup";
import type { View } from "../common/ViewSwitch";
import { EngineMiniStatus } from "../common/EngineStatus";

export function AtlasDatabasePanel({
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
  engineRegistry,
  engineError,
  devSlot,
}: {
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
  engineRegistry: EngineRegistry | null;
  engineError: string | null;
  devSlot?: ReactNode;
}) {
  const db = dbProfileControls.inventory;
  const dbSource = dbProfileControls.activeProfile?.source;
  const dbSourceLabel = dbSource ? dbProfileSourceLabel(dbSource) : db ? "저장된 구조" : "DB 연결 후";
  const hasWorkspace = dbProfileControls.hasWorkspace;
  const [tableFilter, setTableFilter] = useState("");
  const filter = tableFilter.trim().toLowerCase();
  const allTables = db?.tables ?? [];
  const missingColumnTables = allTables.filter((table) => table.columns.length === 0).length;
  const hasAnyColumns = allTables.some((table) => table.columns.length > 0);
  const needsColumns = allTables.length > 0 && missingColumnTables > 0;
  const tables = allTables.filter(
    (table) => !filter || dbInventoryTableKey(table).toLowerCase().includes(filter),
  );
  const schemaNames = [...new Set(tables.map((table) => table.schema).filter(Boolean))] as string[];
  const schemaLabel = db ? (schemaNames.length > 0 ? schemaNames.join(", ") : "기본 스키마") : "스키마 대기";
  const emptyText = !hasWorkspace
    ? "프로젝트를 열면 DB 연결을 등록할 수 있습니다"
    : dbProfileControls.activeProfile
      ? dbProfileControls.dbIndexBlockedReason ?? "DB를 읽으면 테이블/컬럼이 열립니다"
      : "DB 연결을 등록하면 테이블/컬럼 구조가 열립니다";
  const showEngineFooter = hasWorkspace || Boolean(db);

  return (
    <section className="side-card at-db">
      <div className="at-panel-head">
        <span className="at-panel-title">
          <Database size={14} />
          데이터베이스
        </span>
        <span className={`status-pill ${needsColumns ? "amber" : db ? "green" : "pending"}`}>
          {needsColumns ? (hasAnyColumns ? "컬럼 일부 대기" : "컬럼 대기") : dbSourceLabel}
        </span>
      </div>
      <div className="at-path-row">
        <div className="profile-row">
          <span className={`ok-dot ${db ? "" : "pending"}`} />
          <span>{dbProfileControls.activeProfile?.name ?? (db ? "저장된 구조" : "연결 없음")}</span>
          {dbSource && <small className="profile-source-label">{dbProfileSourceLabel(dbSource)}</small>}
        </div>
      </div>
      {db && (
        <div className="at-db-toolbar">
          <span className="at-db-tab">스키마</span>
          <div className="filter-input slim">
            <input
              aria-label="테이블 검색"
              value={tableFilter}
              onChange={(event) => setTableFilter(event.currentTarget.value)}
              placeholder="테이블 검색..."
            />
            <Search size={13} />
          </div>
        </div>
      )}
      <div className="at-tree">
        {db ? (
          <>
            <div className="at-tree-group">
              <Database size={13} />
              <span>{schemaLabel}</span>
            </div>
            <div className="at-tree-group nested">
              <Table2 size={13} />
              <span>
                테이블 <em>({tables.length})</em>
              </span>
            </div>
            {tables.slice(0, 10).map((table) => {
              const tableKey = dbInventoryTableKey(table);
              const tableNeedsColumns = table.columns.length === 0;
              return (
                <button
                  className={`at-tree-leaf table ${tableNeedsColumns ? "needs-columns" : ""} ${tableKey === dbProfileControls.selectedTableKey ? "active" : ""}`}
                  key={tableKey}
                  type="button"
                  aria-label={tableNeedsColumns ? `${table.name} 컬럼 대기` : undefined}
                  onClick={() => selectTableFocus(tableKey)}
                >
                  <Table2 size={12} />
                  <span className="leaf-name">{table.name}</span>
                  {tableNeedsColumns && <em className="leaf-badge warn">컬럼 대기</em>}
                  {tableKey === dbProfileControls.selectedTableKey && <i className={`dot ${tableNeedsColumns ? "orange" : "green"}`} />}
                </button>
              );
            })}
          </>
        ) : (
          <div className="at-empty-action">
            <span className="workspace-empty">{emptyText}</span>
            <button className="outline-action compact" type="button" onClick={showWorkbenchDbTarget}>
              {hasWorkspace ? "코드/DB 연결에서 DB 등록" : "프로젝트 열기"}
            </button>
          </div>
        )}
        {db && tables.length === 0 && (
          <span className="workspace-empty">
            {allTables.length > 0 ? "필터와 일치하는 테이블이 없습니다" : "테이블 목록이 비어 있습니다"}
          </span>
        )}
        {tables.length > 10 && (
          <span className="workspace-empty">+{tables.length - 10}개 더 · 검색으로 좁히세요</span>
        )}
      </div>
      {showEngineFooter && (
        <div className="at-side-footer">
          <EngineMiniStatus label="코드" role="code" registry={engineRegistry} error={engineError} />
          <EngineMiniStatus label="DB" role="db" registry={engineRegistry} error={engineError} />
          {devSlot}
        </div>
      )}
    </section>
  );

  function showWorkbenchDbTarget() {
    if (!hasWorkspace) {
      focusSourceSetup(setView, workspaceControls, dbProfileControls);
      return;
    }

    setView("workbench");
    window.requestAnimationFrame(() => {
        focusDbProfileSetup(dbProfileControls);
    });
  }

  function selectTableFocus(tableKey: string) {
    dbProfileControls.selectTable(tableKey);
    visualMapControls.showMode("table-usage", `db:table:${tableKey}`);
  }
}
