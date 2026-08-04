import { Database, File, Folder, LoaderCircle, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { DbProfileControls } from "../../types/controls";
import { DB_PROFILE_SOURCE_OPTIONS, dbProfileSourceUsesPath } from "../../types/workspace";
import type { Workspace } from "../../types/workspace";

export type AnalysisSetupChoice = "code-only" | "db-only" | "code-and-db";
export type AnalysisProgress = { percent: number; label: string; determinate?: boolean };
type ModalVariant = "analysis" | "connection";

export function AnalysisSetupDialog({
  workspace,
  dbProfileControls,
  busy,
  error,
  onStart,
  onSave,
  onCancel,
  progress,
  variant = "analysis",
}: {
  workspace: Workspace;
  dbProfileControls: DbProfileControls;
  busy: boolean;
  error: string | null;
  onStart?: (choice: AnalysisSetupChoice) => void;
  onSave?: () => void;
  onCancel: () => void;
  progress?: AnalysisProgress;
  variant?: ModalVariant;
}) {
  const [choice, setChoice] = useState<AnalysisSetupChoice>("code-only");
  const dialogRef = useRef<HTMLElement | null>(null);
  const sourceUsesPath = dbProfileSourceUsesPath(dbProfileControls.profileSource);
  const connectionOnly = variant === "connection";
  const analysisDescription =
    choice === "code-only"
      ? "코드 구조만 읽고 바로 지도를 엽니다. DB는 나중에 추가할 수 있습니다."
      : choice === "db-only"
        ? "DB 구조만 읽고 테이블 지도를 엽니다. 코드는 나중에 추가할 수 있습니다."
        : "코드와 DB 구조를 함께 읽고 연결된 지도를 엽니다.";

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    dialog.querySelector<HTMLElement>("input, select, button")?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (!busy) onCancel();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = [
        ...dialog.querySelectorAll<HTMLElement>(
          "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
        ),
      ];
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    dialog.addEventListener("keydown", onKeyDown);
    return () => dialog.removeEventListener("keydown", onKeyDown);
  }, [busy, onCancel]);

  return (
    <div
      className="analysis-setup-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <section
        ref={dialogRef}
        className="analysis-setup-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="analysis-setup-title"
        aria-describedby="analysis-setup-description"
      >
        <header className="analysis-setup-header">
          <div>
            <span className="analysis-setup-kicker">{connectionOnly ? "DB 연결" : "프로젝트 준비"}</span>
            <h2 id="analysis-setup-title">
              {connectionOnly ? "DB 연결을 설정합니다" : `${workspace.name}의 구조를 읽습니다`}
            </h2>
            <p id="analysis-setup-description">
              {connectionOnly ? "연결 정보를 저장하고 필요할 때 DB 구조를 다시 읽을 수 있습니다." : analysisDescription}
            </p>
          </div>
          <button className="tool" type="button" onClick={onCancel} disabled={busy} aria-label="분석 설정 닫기">
            <X size={17} />
          </button>
        </header>

        <div className="analysis-setup-body">
          {!connectionOnly && (
            <fieldset className="analysis-choice-group">
              <legend>이번에 읽을 범위를 선택하세요</legend>
              <label className={`analysis-choice ${choice === "code-and-db" ? "selected" : ""}`}>
                <input
                  type="radio"
                  name="analysis-source"
                  checked={choice === "code-and-db"}
                  onChange={() => setChoice("code-and-db")}
                />
                <Database size={18} />
                <span>
                  <strong>코드 + DB 구조</strong>
                  <small>두 분석을 동시에 실행하고 코드와 테이블 관계까지 준비합니다.</small>
                </span>
              </label>
              <label className={`analysis-choice ${choice === "code-only" ? "selected" : ""}`}>
                <input
                  type="radio"
                  name="analysis-source"
                  checked={choice === "code-only"}
                  onChange={() => setChoice("code-only")}
                />
                <span className="analysis-choice-number">{1}</span>
                <span>
                  <strong>코드만 먼저 읽기</strong>
                  <small>DB는 나중에 소스 관리에서 연결할 수 있습니다.</small>
                </span>
              </label>
              <label className={`analysis-choice ${choice === "db-only" ? "selected" : ""}`}>
                <input
                  type="radio"
                  name="analysis-source"
                  checked={choice === "db-only"}
                  onChange={() => setChoice("db-only")}
                />
                <Database size={18} />
                <span>
                  <strong>DB 구조만 읽기</strong>
                  <small>코드 없이 테이블·컬럼·제약 관계만 먼저 봅니다.</small>
                </span>
              </label>
            </fieldset>
          )}

          {(connectionOnly || choice !== "code-only") && (
            <div className="analysis-db-form">
              <div className="analysis-form-grid">
                <label>
                  <span>연결 이름</span>
                  <input
                    className="workspace-input"
                    value={dbProfileControls.profileName}
                    onChange={(event) => dbProfileControls.setProfileName(event.currentTarget.value)}
                    placeholder="예: 로컬 개발 DB"
                  />
                </label>
                <label>
                  <span>DB 종류</span>
                  <select
                    className="workspace-input"
                    value={dbProfileControls.profileSource}
                    onChange={(event) =>
                      dbProfileControls.setProfileSource(
                        event.currentTarget.value as typeof dbProfileControls.profileSource,
                      )
                    }
                  >
                    {DB_PROFILE_SOURCE_OPTIONS.map((source) => (
                      <option key={source.value} value={source.value}>
                        {source.label}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <label>
                <span>{sourceUsesPath ? "DB 파일 또는 DDL 경로" : "이번 분석에 사용할 연결 문자열"}</span>
                <div className="analysis-path-row">
                  <input
                    className="workspace-input mono"
                    type={sourceUsesPath ? "text" : "password"}
                    value={sourceUsesPath ? dbProfileControls.profilePath : dbProfileControls.connectionString}
                    onChange={(event) =>
                      sourceUsesPath
                        ? dbProfileControls.setProfilePath(event.currentTarget.value)
                        : dbProfileControls.setConnectionString(event.currentTarget.value)
                    }
                    placeholder={sourceUsesPath ? "파일 또는 폴더 경로" : "DB 연결 문자열"}
                  />
                  {sourceUsesPath && (
                    <>
                      <button
                        className="square-button"
                        type="button"
                        onClick={() => dbProfileControls.pickPath(false)}
                        disabled={busy}
                        title="파일 선택"
                        aria-label="파일 선택"
                      >
                        <File size={14} />
                      </button>
                      {dbProfileControls.profileSource === "ddl-sqlite" && (
                        <button
                          className="square-button"
                          type="button"
                          onClick={() => dbProfileControls.pickPath(true)}
                          disabled={busy}
                          title="폴더 선택"
                          aria-label="폴더 선택"
                        >
                          <Folder size={14} />
                        </button>
                      )}
                    </>
                  )}
                </div>
              </label>
              <small className="analysis-privacy-note">
                행 데이터는 읽지 않습니다. 구조, 관계, 제약 정보만 분석합니다.
              </small>
            </div>
          )}

          {busy && !connectionOnly && progress && (
            <div className="analysis-progress" role="status" aria-live="polite">
              <div className="analysis-progress-heading">
                <span>{progress.label}</span>
                <strong>{progress.determinate === false ? "진행 중" : `${progress.percent}%`}</strong>
              </div>
              <div
                className={`analysis-progress-track ${progress.determinate === false ? "indeterminate" : ""}`}
                role="progressbar"
                aria-label="프로젝트 분석 진행률"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={progress.determinate === false ? undefined : progress.percent}
              >
                <span style={progress.determinate === false ? undefined : { width: `${progress.percent}%` }} />
              </div>
              <small>
                {progress.determinate === false
                  ? "분석 엔진이 작업 중입니다. 완료되면 다음 단계로 넘어갑니다."
                  : "완료된 분석 단계 기준 진행률입니다."}
              </small>
            </div>
          )}

          {error && (
            <p className="analysis-setup-error" role="alert">
              {error}
            </p>
          )}
        </div>

        <footer className="analysis-setup-footer">
          <span className="analysis-setup-status">
            {busy ? (
              <>
                <LoaderCircle size={14} className="spin" />{" "}
                {connectionOnly
                  ? "연결 저장 중"
                  : `${progress?.determinate === false ? "진행 중" : `${progress?.percent ?? 0}%`} · ${progress?.label ?? "분석 준비 중"}`}
              </>
            ) : connectionOnly ? (
              "연결 문자열은 저장하지 않습니다."
            ) : (
              "선택한 범위만 분석합니다."
            )}
          </span>
          <div>
            <button className="outline-action" type="button" onClick={onCancel} disabled={busy}>
              {connectionOnly ? "취소" : "나중에"}
            </button>
            <button
              className="primary-action"
              type="button"
              onClick={() => (connectionOnly ? onSave?.() : onStart?.(choice))}
              disabled={busy}
            >
              {busy ? (connectionOnly ? "저장 중..." : "분석 중...") : connectionOnly ? "연결 저장" : "분석 시작"}
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
}
