import { Code2, RefreshCw } from "lucide-react";
import { useEffect, useRef } from "react";
import {
  codeInventoryFileCount,
  codeInventoryItemCount,
  codeInventoryRouteCount,
  codeInventorySymbolCount,
} from "../../types/workspace";
import type { WorkspaceControls } from "../../types/controls";
import { PanelHeader } from "../common/PanelHeader";

export function CodeSourceSection({
  workspaceControls,
}: {
  workspaceControls: WorkspaceControls;
}) {
  const operationMessageRef = useRef<HTMLSpanElement>(null);
  const codeInventory = workspaceControls.codeInventory;
  const codeCounts = {
    routes: codeInventoryRouteCount(codeInventory),
    code: codeInventorySymbolCount(codeInventory),
    files: codeInventoryFileCount(codeInventory),
  };
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasCodeInventory = Boolean(codeInventory);
  const hasCodeItems = codeInventoryItemCount(codeInventory) > 0;
  const showCodeOperationMessage = Boolean(
    workspaceControls.codeError ||
      workspaceControls.codeIndexing ||
      (!hasCodeInventory && workspaceControls.codeStatus),
  );
  const nextAction = codeNextAction(workspaceControls, hasWorkspace, hasCodeInventory);
  const sourceSettings = (
    <>
      <label className="field-label">프로젝트 폴더</label>
      <div className="path-row">
        <div className="path-input">{workspaceControls.currentWorkspace?.repoPath ?? workspaceControls.repoPath}</div>
      </div>
    </>
  );

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
            disabled={workspaceControls.busy || nextAction.disabled || !workspaceControls.canIndexCode}
            title={workspaceControls.codeIndexBlockedReason ?? undefined}
            data-source-action="code-index"
          >
            {nextAction.button === "다시 읽기" && (
              <RefreshCw size={13} className={workspaceControls.codeIndexing ? "spin" : undefined} />
            )}
            <span>{nextAction.button}</span>
          </button>
        )}
      </div>
      {hasCodeInventory && (
        <div className="source-stat-grid" aria-label="코드 목록 요약">
          <span className={codeCounts.routes > 0 ? "ready" : ""}>
            <b>API</b>
            <em>{codeCounts.routes}</em>
          </span>
          <span className={codeCounts.code > 0 ? "ready" : ""}>
            <b>코드</b>
            <em>{codeCounts.code}</em>
          </span>
          <span className={codeCounts.files > 0 ? "ready" : ""}>
            <b>파일</b>
            <em>{codeCounts.files}</em>
          </span>
        </div>
      )}
      {!hasWorkspace ? null : (
        <>
          {hasCodeItems ? (
            <details className="source-advanced">
              <summary>프로젝트 폴더</summary>
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
        </>
      )}
    </section>
  );
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
    if (!workspaceControls.canIndexCode) {
      return {
        label: "읽기 도구 필요",
        text: workspaceControls.codeIndexBlockedReason ?? "코드 읽기 도구 상태를 확인하세요.",
      };
    }
    return {
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
  const hasRoutes = codeInventoryRouteCount(workspaceControls.codeInventory) > 0;
  const codeCount = codeBucketItemCount(workspaceControls);
  const fileCount = codeInventoryFileCount(workspaceControls.codeInventory);
  const summary = [
    hasRoutes ? `API ${codeInventoryRouteCount(workspaceControls.codeInventory)}개` : null,
    codeCount > 0 ? `코드 ${codeCount}개` : null,
    fileCount > 0 ? `파일 ${fileCount}개` : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return {
    label: "근거 준비됨",
    text: `${summary} 읽힘`,
    button: workspaceControls.codeIndexing ? "읽는 중" : "다시 읽기",
    run: workspaceControls.indexCodeRepository,
    tone: "ready",
  };
}

function codeBucketItemCount(workspaceControls: WorkspaceControls): number {
  return codeInventorySymbolCount(workspaceControls.codeInventory);
}
