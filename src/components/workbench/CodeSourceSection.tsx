import { Code2, Filter, RefreshCw, Search } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { codeInventoryCodeItems, codeInventoryItemCount, codeKindChip, codeRouteMethod } from "../../types/workspace";
import type { WorkspaceControls } from "../../types/controls";
import { PanelHeader } from "../common/PanelHeader";

type CodeTab = "routes" | "services" | "files";

const codeTabs: { id: CodeTab; label: string }[] = [
  { id: "routes", label: "API 라우트" },
  { id: "services", label: "코드" },
  { id: "files", label: "파일" },
];

const CODE_LIST_LIMIT = 80;

export function CodeSourceSection({
  workspaceControls,
}: {
  workspaceControls: WorkspaceControls;
}) {
  const operationMessageRef = useRef<HTMLSpanElement>(null);
  const [codeTab, setCodeTab] = useState<CodeTab>("routes");
  const [codeFilter, setCodeFilter] = useState("");
  const codeInventory = workspaceControls.codeInventory;
  const codeBucketItems = codeInventoryCodeItems(codeInventory);
  const unknownCodeIds = new Set((codeInventory?.unknown ?? []).map((item) => item.id));
  const tabCounts: Record<CodeTab, number> = {
    routes: codeInventory?.routes.length ?? 0,
    services: codeBucketItems.length,
    files: codeInventory?.files.length ?? 0,
  };
  const allCodeItems = codeTab === "services" ? codeBucketItems : (codeInventory?.[codeTab] ?? []);
  const confirmedRouteIds = new Set((codeInventory?.handles ?? []).map((handle) => handle.route));
  const filter = codeFilter.trim().toLowerCase();
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasCodeInventory = Boolean(codeInventory);
  const hasCodeItems = codeInventoryItemCount(codeInventory) > 0;
  const showCodeOperationMessage = Boolean(
    workspaceControls.codeError ||
      workspaceControls.codeIndexing ||
      workspaceControls.codeLoading ||
      (!hasCodeInventory && workspaceControls.codeStatus),
  );
  const matchingCodeItems = filter
    ? allCodeItems.filter(
        (item) =>
          item.name.toLowerCase().includes(filter) ||
          (item.filePath ?? "").toLowerCase().includes(filter),
      )
    : allCodeItems;
  const codeItems = limitedCodeItems(
    matchingCodeItems,
    workspaceControls.selectedCodeItem?.id ?? null,
    CODE_LIST_LIMIT,
  );
  const hiddenCodeItemCount = Math.max(0, matchingCodeItems.length - codeItems.length);
  const nextAction = codeNextAction(workspaceControls, hasWorkspace, hasCodeInventory);
  const sourceSettings = (
    <>
      <label className="field-label">프로젝트 폴더</label>
      <div className="path-row">
        <div className="path-input">{workspaceControls.currentWorkspace?.repoPath ?? workspaceControls.repoPath}</div>
      </div>
      {hasCodeInventory && (
        <div className="source-maintenance" aria-label="코드 위치 관리">
          <button
            className="outline-action compact"
            onClick={workspaceControls.indexCodeRepository}
            disabled={workspaceControls.busy || !workspaceControls.canIndexCode}
            title={workspaceControls.codeIndexBlockedReason ?? undefined}
          >
            <RefreshCw size={13} className={workspaceControls.codeIndexing ? "spin" : undefined} />
            {workspaceControls.codeIndexing ? "읽는 중" : "다시 읽기"}
          </button>
          <button
            className="outline-action compact"
            onClick={workspaceControls.loadCodeInventory}
            disabled={workspaceControls.busy}
          >
            {workspaceControls.codeLoading ? "불러오는 중" : "목록 새로고침"}
            <Code2 size={13} />
          </button>
        </div>
      )}
    </>
  );

  useEffect(() => {
    if (!hasCodeInventory || tabCounts[codeTab] > 0) {
      return;
    }

    const nextTab =
      tabCounts.routes > 0 ? "routes" : tabCounts.services > 0 ? "services" : tabCounts.files > 0 ? "files" : codeTab;
    if (nextTab !== codeTab) {
      setCodeTab(nextTab);
    }
  }, [codeTab, hasCodeInventory, tabCounts.files, tabCounts.routes, tabCounts.services]);

  useEffect(() => {
    if (workspaceControls.codeError) {
      operationMessageRef.current?.focus();
    }
  }, [workspaceControls.codeError]);

  return (
    <section className={`side-card code-source ${hasWorkspace ? "" : "locked"}`}>
      <PanelHeader icon={<Code2 size={16} />} title="코드" />
      <div className={`source-next ${nextAction.tone === "ready" ? "source-ready" : ""}`}>
          <span>
            <b>{nextAction.label}</b>
            <small>{nextAction.text}</small>
          </span>
          {nextAction.run && (
            <button
              className={nextAction.primary ? "primary-action compact" : "outline-action compact"}
              type="button"
              onClick={nextAction.run}
              disabled={workspaceControls.busy || nextAction.disabled}
            >
              <span>{nextAction.button}</span>
            </button>
          )}
        </div>
      {hasCodeInventory && (
        <div className="source-stat-grid" aria-label="코드 목록 요약">
          <span className={tabCounts.routes > 0 ? "ready" : ""}>
            <b>API</b>
            <em>{tabCounts.routes}</em>
          </span>
          <span className={tabCounts.services > 0 ? "ready" : ""}>
            <b>코드</b>
            <em>{tabCounts.services}</em>
          </span>
          <span className={tabCounts.files > 0 ? "ready" : ""}>
            <b>파일</b>
            <em>{tabCounts.files}</em>
          </span>
        </div>
      )}
      {!hasWorkspace ? null : (
        <>
          {hasCodeItems ? (
            <details className="source-advanced">
              <summary>프로젝트 폴더 / 다시 읽기</summary>
              {sourceSettings}
            </details>
          ) : (
            sourceSettings
          )}
      {showCodeOperationMessage && (
        <span
          ref={operationMessageRef}
          className={`workspace-message ${workspaceControls.codeError ? "error" : ""}`}
          role={workspaceControls.codeError ? "alert" : undefined}
          tabIndex={workspaceControls.codeError ? -1 : undefined}
        >
          {workspaceControls.codeError ?? workspaceControls.codeStatus}
        </span>
      )}
      {workspaceControls.codeError && workspaceControls.codeErrorDetail && (
        <details className="error-details">
          <summary>상세 오류</summary>
          <pre>{workspaceControls.codeErrorDetail}</pre>
        </details>
      )}
      {hasCodeInventory && (
        <>
          <div className="tabs" role="tablist" aria-label="코드 목록 유형">
            {codeTabs.map((tab) => (
              <button
                className={codeTab === tab.id ? "active" : ""}
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={codeTab === tab.id}
                onClick={() => setCodeTab(tab.id)}
              >
                {tab.label} <span>{tabCounts[tab.id]}</span>
              </button>
            ))}
          </div>
          <label className="field-label">코드 목록</label>
          <div className="filter-input">
            <Search size={13} />
            <input
              aria-label="코드 목록 필터"
              value={codeFilter}
              onChange={(event) => setCodeFilter(event.currentTarget.value)}
              placeholder="이름 또는 경로 필터..."
            />
            <Filter size={13} />
          </div>
          <div className="list">
            {codeItems.map((item) => {
              const method = codeTab === "routes" ? codeRouteMethod(item) : null;
              const confirmed = confirmedRouteIds.has(item.id);
              const unverified = unknownCodeIds.has(item.id);
              const meta = codeTab === "routes"
                ? routeEvidenceMeta(item, method, confirmed)
                : unverified
                  ? "근거 미확인"
                  : codeItemLocation(item);
              return (
                <button
                  className={`route-row route-row-button ${
                    item.id === workspaceControls.selectedCodeItem?.id ? "active" : ""
                  }`}
                  key={item.id}
                  type="button"
                  onClick={() => workspaceControls.openCodeItem(item)}
                  title={`${method === "ANY" ? "ANY?" : method ?? codeKindChip(item.kind)} ${item.name}${meta ? `\n${meta}` : ""}`}
                >
                  <span className={`method ${(method ?? "code").toLowerCase()}`}>
                    {method === "ANY" ? "ANY?" : method ?? codeKindChip(item.kind)}
                  </span>
                  <span className="route-copy">
                    <span className="route-path">{item.name}</span>
                    {meta && <small className={confirmed ? "confirmed" : "unverified"}>{meta}</small>}
                  </span>
                </button>
              );
            })}
            {hiddenCodeItemCount > 0 && (
              <span className="workspace-empty">
                {codeItems.length}개 표시 · +{hiddenCodeItemCount}개 · 필터로 좁히세요
              </span>
            )}
            {codeItems.length === 0 && (
              <span className="workspace-empty">{filter ? "필터와 일치하는 항목이 없습니다" : codeTabEmptyText(codeTab)}</span>
            )}
          </div>
        </>
      )}
        </>
      )}
    </section>
  );
}

function limitedCodeItems<T extends { id: string }>(items: T[], selectedId: string | null, limit: number): T[] {
  const visible = items.slice(0, limit);
  if (!selectedId || visible.some((item) => item.id === selectedId)) {
    return visible;
  }
  const selected = items.find((item) => item.id === selectedId);
  return selected ? [selected, ...visible.slice(0, Math.max(0, limit - 1))] : visible;
}

function codeItemLocation(item: { filePath?: string | null; line?: number | null }): string | null {
  const path = item.filePath ? item.filePath.replace(/\\/g, "/").split("/").slice(-3).join("/") : null;
  if (!path) {
    return item.line ? `L${item.line}` : null;
  }
  return item.line ? `${path}:L${item.line}` : path;
}

function routeEvidenceMeta(
  item: { filePath?: string | null; line?: number | null },
  method: string | null,
  confirmed: boolean,
): string {
  const location = codeItemLocation(item);
  if (confirmed) {
    return `확정 HANDLES${location ? ` · ${location}` : ""}`;
  }
  if (method === "ANY" && !location) {
    return "메서드 미확인 참조 · 소스 위치 없음";
  }
  return `HANDLES 미확인${location ? ` · ${location}` : " · 소스 위치 없음"}`;
}

function codeTabEmptyText(tab: CodeTab): string {
  if (tab === "routes") {
    return "API 라우트가 없습니다";
  }
  if (tab === "files") {
    return "파일 항목이 없습니다";
  }
  return "코드 항목이 없습니다";
}

function codeNextAction(
  workspaceControls: WorkspaceControls,
  hasWorkspace: boolean,
  hasCodeInventory: boolean,
): {
  label: string;
  text: string;
  button?: string;
  run?: () => void;
  primary?: boolean;
  disabled?: boolean;
  tone?: "ready";
} {
  if (!hasWorkspace) {
    return {
      label: "프로젝트 열기",
      text: workspaceControls.canCreateWorkspace
        ? "프로젝트를 연 뒤 API와 코드 항목을 읽습니다."
        : workspaceControls.repoSourceMode === "github"
          ? "URL 입력 후 코드 목록을 만듭니다."
          : "폴더 선택 후 코드 목록을 만듭니다.",
    };
  }
  if (!hasCodeInventory) {
    const indexed = workspaceControls.codeStatus?.includes("완료") ?? false;
    if (!indexed && !workspaceControls.canIndexCode) {
      return {
        label: "읽기 도구 필요",
        text: workspaceControls.codeIndexBlockedReason ?? "코드 읽기 도구 상태를 확인하세요.",
      };
    }
    return indexed
      ? {
          label: "코드 목록 표시",
          text: "읽은 API, 함수, 파일을 목록에 표시합니다.",
          button: "코드 목록 열기",
          run: workspaceControls.loadCodeInventory,
          primary: true,
        }
      : {
          label: "코드 읽기",
          text: "API, 함수, 파일을 읽습니다.",
          button: "코드 읽기",
          run: workspaceControls.indexCodeRepository,
          primary: true,
        };
  }
  const itemCount = codeInventoryItemCount(workspaceControls.codeInventory);
  if (itemCount === 0) {
    if (!workspaceControls.canIndexCode) {
      return {
        label: "읽기 도구 필요",
        text: workspaceControls.codeIndexBlockedReason ?? "코드 읽기 도구 상태를 확인하세요.",
      };
    }
    return {
      label: "비어 있음",
      text: "코드 항목이 없습니다. 프로젝트 폴더를 확인하세요.",
      button: "다시 읽기",
      run: workspaceControls.indexCodeRepository,
    };
  }
  const hasRoutes = (workspaceControls.codeInventory?.routes.length ?? 0) > 0;
  const codeCount = codeBucketItemCount(workspaceControls);
  const fileCount = workspaceControls.codeInventory?.files.length ?? 0;
  const summary = [
    hasRoutes ? `API ${workspaceControls.codeInventory?.routes.length ?? 0}개` : null,
    codeCount > 0 ? `코드 ${codeCount}개` : null,
    fileCount > 0 ? `파일 ${fileCount}개` : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return {
    label: "근거 준비됨",
    text: `${summary} 읽힘`,
    tone: "ready",
  };
}

function codeBucketItemCount(workspaceControls: WorkspaceControls): number {
  return codeInventoryCodeItems(workspaceControls.codeInventory).length;
}
