import { Folder, GitBranch, Plus, RefreshCw } from "lucide-react";
import { useEffect, useRef, type KeyboardEvent } from "react";
import { tauriUnavailableMessage } from "../../app/tauriRuntime";
import type { WorkspaceControls } from "../../types/controls";
import { PanelHeader } from "../common/PanelHeader";

type WorkspaceAction = {
  label: string;
  text: string;
  button?: string;
  run?: () => void;
  primary?: boolean;
  disabled?: boolean;
};

export function WorkspaceCard({ workspaceControls }: { workspaceControls: WorkspaceControls }) {
  const operationMessageRef = useRef<HTMLSpanElement>(null);
  const hasSavedWorkspaces = workspaceControls.workspaces.length > 0;
  const hasRepoTarget = Boolean(workspaceControls.repoPath.trim()) || Boolean(workspaceControls.currentWorkspace);
  const currentWorkspaceMatchesForm = Boolean(
    workspaceControls.currentWorkspace &&
      workspaceControls.currentWorkspace.name === workspaceControls.workspaceName.trim() &&
      workspaceControls.currentWorkspace.repoPath === workspaceControls.repoPath.trim(),
  );
  const repoMessage =
    workspaceControls.repoPathError ??
    (currentWorkspaceMatchesForm
      ? "현재 프로젝트가 열려 있습니다."
      : workspaceControls.canCreateWorkspace
      ? workspaceControls.repoSourceMode === "github"
        ? "GitHub URL 확인됨. 복제해서 프로젝트를 여세요."
        : "폴더 확인됨. 프로젝트를 여세요."
      : workspaceControls.repoSourceMode === "github"
        ? "URL을 입력하면 복제 후 분석합니다."
        : "폴더를 고르거나 경로를 붙여넣으세요.");
  const createLabel = workspaceControls.repoSourceMode === "github" ? "복제" : "열기";
  const nextAction = workspaceNextAction(workspaceControls, createLabel);
  const NextActionIcon = nextAction.primary ? Plus : workspaceControls.repoSourceMode === "github" ? GitBranch : Folder;
  const runtimeNotice = workspaceControls.error === tauriUnavailableMessage;
  const showNameInput =
    Boolean(workspaceControls.repoPath.trim()) ||
    Boolean(workspaceControls.workspaceName.trim()) ||
    workspaceControls.canCreateWorkspace;
  const showPendingNextAction = hasRepoTarget || Boolean(nextAction.button);
  const setupFields = (
    <>
      <div className="source-toggle" role="group" aria-label="프로젝트 가져오기 방식 선택">
        <button
          className={workspaceControls.repoSourceMode === "local" ? "active" : ""}
          type="button"
          onClick={() => workspaceControls.setRepoSourceMode("local")}
          disabled={workspaceControls.busy}
          aria-pressed={workspaceControls.repoSourceMode === "local"}
          aria-label="로컬 폴더"
          title="로컬 폴더"
        >
          <Folder size={12} />
          폴더
        </button>
        <button
          className={workspaceControls.repoSourceMode === "github" ? "active" : ""}
          type="button"
          onClick={() => workspaceControls.setRepoSourceMode("github")}
          disabled={workspaceControls.busy}
          aria-pressed={workspaceControls.repoSourceMode === "github"}
          aria-label="GitHub URL"
          title="GitHub URL"
        >
          <GitBranch size={12} />
          GitHub
        </button>
      </div>
      <label className="field-label" htmlFor="workspace-repo-input">
        {workspaceControls.repoSourceMode === "github" ? "GitHub URL" : "프로젝트 폴더"}
      </label>
      <div className="path-row">
        <input
          id="workspace-repo-input"
          className="workspace-input mono"
          value={workspaceControls.repoPath}
          onChange={(event) => workspaceControls.setRepoPath(event.currentTarget.value)}
          onKeyDown={submitWorkspaceOnEnter}
          placeholder={
            workspaceControls.repoSourceMode === "github"
              ? "GitHub URL 입력"
              : "폴더 경로 입력"
          }
        />
      </div>
      {workspaceControls.repoSourceMode === "local" && (
        <button
          className="outline-action"
          onClick={workspaceControls.pickRepoPath}
          disabled={workspaceControls.busy}
          type="button"
          aria-label="로컬 프로젝트 폴더 선택"
        >
          <Folder size={13} />
          폴더 선택
        </button>
      )}
      <span className={`workspace-message ${workspaceControls.repoPathError ? "error" : ""}`}>
        {repoMessage}
      </span>
      {showNameInput && (
        <details className="source-advanced">
          <summary>프로젝트 이름: {workspaceControls.workspaceName.trim() || "자동"}</summary>
          <label className="field-label" htmlFor="workspace-name-input">프로젝트 이름</label>
          <input
            id="workspace-name-input"
            className="workspace-input"
            value={workspaceControls.workspaceName}
            onChange={(event) => workspaceControls.setWorkspaceName(event.currentTarget.value)}
            onKeyDown={submitWorkspaceOnEnter}
            placeholder="프로젝트명으로 자동 입력"
          />
        </details>
      )}
    </>
  );
  const showWorkspaceList = hasSavedWorkspaces && (!currentWorkspaceMatchesForm || workspaceControls.workspaces.length > 1);

  useEffect(() => {
    if (workspaceControls.error) {
      operationMessageRef.current?.focus();
    }
  }, [workspaceControls.error]);

  return (
    <section className="side-card workspace-source">
      <PanelHeader icon={<Folder size={16} />} title="프로젝트" />
      {currentWorkspaceMatchesForm ? (
        <>
          <div className="source-next workspace-ready">
            <span>
              <b>{workspaceControls.currentWorkspace?.name}</b>
              <small>{workspaceControls.currentWorkspace?.repoPath}</small>
            </span>
            {hasSavedWorkspaces && (
              <button
                className="square-button"
                type="button"
                onClick={workspaceControls.refreshWorkspaces}
                disabled={workspaceControls.busy}
                aria-label="프로젝트 목록 새로고침"
              >
                <RefreshCw size={13} className={workspaceControls.opening ? "spin" : undefined} />
              </button>
            )}
          </div>
          <details className="source-advanced">
            <summary>다른 프로젝트 열기</summary>
            {setupFields}
          </details>
        </>
      ) : (
        <>
          {showPendingNextAction && (
            <div className="source-next">
              <span>
                <b>{nextAction.label}</b>
                <small>{nextAction.text}</small>
              </span>
              {nextAction.button && nextAction.run && (
                <button
                  className={nextAction.primary ? "primary-action compact" : "outline-action compact"}
                  type="button"
                  onClick={nextAction.run}
                  disabled={workspaceControls.busy || nextAction.disabled}
                >
                  {workspaceControls.creating ? <RefreshCw size={13} className="spin" /> : <NextActionIcon size={13} />}
                  <span>
                    {workspaceControls.creating ? (workspaceControls.repoSourceMode === "github" ? "복제 중..." : "여는 중...") : nextAction.button}
                  </span>
                </button>
              )}
            </div>
          )}
          {setupFields}
          {hasSavedWorkspaces && (
            <div className="workspace-actions">
              <button
                className="square-button"
                type="button"
                onClick={workspaceControls.refreshWorkspaces}
                disabled={workspaceControls.busy}
                aria-label="프로젝트 목록 새로고침"
              >
                <RefreshCw size={13} className={workspaceControls.opening ? "spin" : undefined} />
              </button>
            </div>
          )}
        </>
      )}
      {showWorkspaceList && (
        <div className="workspace-list">
          {workspaceControls.workspaces.slice(0, 4).map((workspace) => (
            <button
              className={`workspace-row ${workspace.id === workspaceControls.currentWorkspace?.id ? "active" : ""}`}
              key={workspace.id}
              type="button"
              aria-current={workspace.id === workspaceControls.currentWorkspace?.id ? "true" : undefined}
              disabled={workspaceControls.busy}
              onClick={() => workspaceControls.openWorkspace(workspace.id)}
            >
              <span>{workspace.name}</span>
              <small>{workspace.repoPath}</small>
            </button>
          ))}
        </div>
      )}
      {workspaceControls.recoveryWarnings.length > 0 && (
        <div className="workspace-message error" role="alert">
          {workspaceControls.recoveryWarnings.map((warning) => (
            <div key={warning.workspaceId}>
              <span>{warning.message}</span>
              {warning.action === "repair-from-backup" ? (
                <button
                  className="outline-action compact"
                  type="button"
                  onClick={() => workspaceControls.repairWorkspaceFromBackup(warning.workspaceId)}
                  disabled={workspaceControls.busy}
                >
                  백업 복구
                </button>
              ) : null}
            </div>
          ))}
        </div>
      )}
      {(workspaceControls.status || workspaceControls.error) && (
        <span
          ref={operationMessageRef}
          className={`workspace-message ${workspaceControls.error ? (runtimeNotice ? "notice" : "error") : ""}`}
          role={workspaceControls.error && !runtimeNotice ? "alert" : undefined}
          tabIndex={workspaceControls.error && !runtimeNotice ? -1 : undefined}
        >
          {workspaceControls.error ?? workspaceControls.status}
        </span>
      )}
    </section>
  );

  function submitWorkspaceOnEnter(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Enter" || !workspaceControls.canCreateWorkspace || workspaceControls.busy) {
      return;
    }
    event.preventDefault();
    workspaceControls.createWorkspace();
  }
}

function workspaceNextAction(workspaceControls: WorkspaceControls, createLabel: string): WorkspaceAction {
  if (workspaceControls.canCreateWorkspace) {
    return {
      label: workspaceControls.repoSourceMode === "github" ? "저장소 복제" : "프로젝트 열기",
      text:
        workspaceControls.repoSourceMode === "github"
          ? "복제 후 코드와 DB 연결을 이어갑니다."
          : "선택한 폴더로 코드와 DB 연결을 이어갑니다.",
      button: createLabel,
      run: workspaceControls.createWorkspace,
      primary: true,
      disabled: workspaceControls.busy,
    };
  }
  if (workspaceControls.repoSourceMode === "github") {
    return {
      label: "GitHub URL 입력",
      text: "GitHub URL을 붙여넣으면 복제해서 프로젝트를 열 수 있습니다.",
    };
  }
  return {
      label: "로컬 폴더 선택",
      text: "폴더를 지정하면 바로 열 수 있습니다.",
  };
}
