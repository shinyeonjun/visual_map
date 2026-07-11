import { Boxes, FileText, Globe, ListTree, Search } from "lucide-react";
import { useState } from "react";
import { codeInventoryCodeItems, codeKindChip } from "../../types/workspace";
import type { CodeInventoryItem } from "../../types/workspace";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { focusSourceSetup } from "../common/focusSourceSetup";
import type { View } from "../common/ViewSwitch";

export function AtlasRepositoryPanel({
  setView,
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  setView: (view: View) => void;
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const code = workspaceControls.codeInventory;
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const [symbolFilter, setSymbolFilter] = useState("");
  const filter = symbolFilter.trim().toLowerCase();
  const matches = (item: CodeInventoryItem) =>
    !filter || item.name.toLowerCase().includes(filter) || (item.filePath ?? "").toLowerCase().includes(filter);
  const allRoutes = code?.routes ?? [];
  const allServices = codeInventoryCodeItems(code);
  const allFiles = code?.files ?? [];
  const routes = allRoutes.filter(matches);
  const services = allServices.filter(matches);
  const files = allFiles.filter(matches);
  const hasDbTables = Boolean(dbProfileControls.inventory?.tables.length);
  const hasDbColumns = Boolean(dbProfileControls.inventory?.tables.some((table) => table.columns.length > 0));
  const codeIndexed = workspaceControls.codeStatus?.includes("완료") ?? false;
  const codeActionLabel = codeIndexed
    ? workspaceControls.codeLoading
      ? "불러오는 중"
      : "코드 불러오기"
    : !workspaceControls.canIndexCode
      ? workspaceControls.codeIndexBlockedReason ?? "코드 읽기 도구 필요"
    : workspaceControls.codeIndexing
      ? "읽는 중"
      : "코드 읽기";

  return (
    <section className="side-card at-repo">
      <div className="at-panel-head">
        <span className="at-panel-title">
          <FileText size={14} />
          프로젝트
        </span>
        <span className={`status-pill ${hasWorkspace ? "green" : "pending"}`}>
          {hasWorkspace ? "열림" : "프로젝트 필요"}
        </span>
      </div>
      <div className="at-path-row">
        <span className="at-path">
          {(workspaceControls.currentWorkspace?.repoPath ?? workspaceControls.repoPath) || "프로젝트 폴더 필요"}
        </span>
      </div>
      {code && (
        <div className="filter-input">
          <Search size={13} />
          <input
            aria-label="파일과 심볼 필터"
            value={symbolFilter}
            onChange={(event) => setSymbolFilter(event.currentTarget.value)}
            placeholder="파일과 심볼 필터..."
          />
        </div>
      )}
      <div className="at-tree">
        {code ? (
          <>
            <div className="at-tree-group">
              <Globe size={13} />
              <span>
                API 라우트 <em>({routes.length})</em>
              </span>
            </div>
            {routes.slice(0, 8).map((route) => (
              <button
                className={`at-tree-leaf route ${route.id === workspaceControls.selectedCodeItem?.id ? "active" : ""}`}
                key={route.id}
                type="button"
                onClick={() => selectCodeFocus(route)}
              >
                <span className="method get">{codeKindChip(route.kind)}</span>
                <span className="leaf-name">{route.name}</span>
              </button>
            ))}
            <ListHint
              count={routes.length}
              total={allRoutes.length}
              limit={8}
              empty="API 라우트가 없습니다"
              filteredEmpty="필터와 일치하는 API 라우트가 없습니다"
            />

            <div className="at-tree-group">
              <Boxes size={13} />
              <span>
                코드 <em>({services.length})</em>
              </span>
            </div>
            {services.slice(0, 8).map((item) => (
              <button
                className={`at-tree-leaf ${item.id === workspaceControls.selectedCodeItem?.id ? "active" : ""}`}
                key={item.id}
                type="button"
                onClick={() => selectCodeFocus(item)}
              >
                <span className="leaf-name">{item.name}</span>
              </button>
            ))}
            <ListHint
              count={services.length}
              total={allServices.length}
              limit={8}
              empty="코드 항목이 없습니다"
              filteredEmpty="필터와 일치하는 코드 항목이 없습니다"
            />

            <div className="at-tree-group">
              <ListTree size={13} />
              <span>
                파일 <em>({files.length})</em>
              </span>
            </div>
            {files.slice(0, 6).map((item) => (
              <button
                className={`at-tree-leaf ${item.id === workspaceControls.selectedCodeItem?.id ? "active" : ""}`}
                key={item.id}
                type="button"
                onClick={() => selectCodeFocus(item)}
              >
                <span className="leaf-name">{item.name}</span>
              </button>
            ))}
            <ListHint
              count={files.length}
              total={allFiles.length}
              limit={6}
              empty="파일 항목이 없습니다"
              filteredEmpty="필터와 일치하는 파일이 없습니다"
            />
          </>
        ) : (
          <div className="at-empty-action">
            <span className="workspace-empty">
              {hasWorkspace && hasDbTables
                ? hasDbColumns
                  ? "코드를 연결하면 DB 구조에 API와 후보 근거가 이어집니다."
                  : "DB 테이블은 읽혔고, 코드는 나중에 연결해도 됩니다."
                : hasWorkspace
                ? "API 경로와 코드 검색을 열려면 코드 목록이 필요합니다."
                : "프로젝트를 열면 API 경로와 코드 검색이 열립니다."}
            </span>
            {hasWorkspace ? (
              <button
                className={hasDbTables ? "outline-action compact" : "primary-action compact"}
                type="button"
                onClick={codeIndexed ? workspaceControls.loadCodeInventory : workspaceControls.indexCodeRepository}
                disabled={workspaceControls.busy || (!codeIndexed && !workspaceControls.canIndexCode)}
                title={codeIndexed ? undefined : (workspaceControls.codeIndexBlockedReason ?? undefined)}
              >
                {codeActionLabel}
              </button>
            ) : (
              <button
                className="primary-action compact"
                type="button"
                onClick={() => focusSourceSetup(setView, workspaceControls, dbProfileControls)}
                disabled={workspaceControls.busy}
              >
                코드/DB 연결에서 프로젝트 열기
              </button>
            )}
          </div>
        )}
      </div>
    </section>
  );

  function selectCodeFocus(item: CodeInventoryItem) {
    workspaceControls.selectCodeItem(item);
    visualMapControls.showMode(isApiItem(item) ? "api-flow" : "search-focus", `code:${item.id}`);
  }
}

function isApiItem(item: CodeInventoryItem): boolean {
  const kind = item.kind.trim().toLowerCase();
  return kind === "route" || kind === "api";
}

function ListHint({
  count,
  total,
  limit,
  empty,
  filteredEmpty,
}: {
  count: number;
  total: number;
  limit: number;
  empty: string;
  filteredEmpty: string;
}) {
  if (count === 0) {
    return <span className="workspace-empty">{total > 0 ? filteredEmpty : empty}</span>;
  }
  if (count > limit) {
    return <span className="workspace-empty">+{count - limit}개 더 · 필터로 좁히세요</span>;
  }
  return null;
}
