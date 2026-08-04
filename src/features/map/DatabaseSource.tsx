import { Database, LockKeyhole, RefreshCw, Trash2 } from "lucide-react";
import { useEffect, useRef } from "react";
import { confirmAction } from "../../app/confirmAction";
import { dbInventoryTableCount, dbProfileSourceLabel } from "../../types/workspace";
import type { DbProfileControls } from "../../types/controls";
import { PanelHeader } from "../../components/common/PanelHeader";

export function DatabaseSource({
  dbProfileControls,
  onEditDbConnection,
}: {
  dbProfileControls: DbProfileControls;
  onEditDbConnection?: () => void;
}) {
  const operationMessageRef = useRef<HTMLSpanElement>(null);
  const hasWorkspace = dbProfileControls.hasWorkspace;
  const allTables = dbProfileControls.inventory?.tables ?? [];
  const hasProfile = Boolean(dbProfileControls.activeProfile);
  const hasInventory = Boolean(dbProfileControls.inventory);
  const hasTables = dbInventoryTableCount(dbProfileControls.inventory) > 0;
  const hasColumns = allTables.some((table) => table.columns.length > 0);
  const missingColumnTables = allTables.filter((table) => table.columns.length === 0).length;
  const hasCompleteColumns = hasTables && missingColumnTables === 0;
  const columnCount = allTables.reduce((sum, table) => sum + table.columns.length, 0);
  const foreignKeyCount = allTables.reduce(
    (sum, table) =>
      sum + Math.max(table.foreignKeys?.length ?? 0, table.columns.filter((column) => column.isForeignKey).length),
    0,
  );
  const nextAction = dbNextAction(
    dbProfileControls,
    hasProfile,
    hasInventory,
    hasTables,
    hasColumns,
    missingColumnTables,
  );
  const showDbOperationMessage = Boolean(
    dbProfileControls.error ||
    dbProfileControls.saving ||
    dbProfileControls.indexing ||
    (!hasInventory && dbProfileControls.status),
  );

  useEffect(() => {
    if (dbProfileControls.error) {
      operationMessageRef.current?.focus();
    }
  }, [dbProfileControls.error]);

  return (
    <section className={`side-card database-source ${hasWorkspace ? "" : "locked"} ${hasInventory ? "ready" : ""}`}>
      <PanelHeader icon={<Database size={16} />} title="데이터베이스" />
      {!hasWorkspace && (
        <div className="source-locked-message">
          <LockKeyhole size={18} aria-hidden="true" />
          <span>
            <b>프로젝트를 먼저 연결하세요</b>
            <small>코드 분석 후 필요할 때 DB 구조를 추가할 수 있습니다.</small>
          </span>
          <em>선택 사항</em>
        </div>
      )}
      {hasWorkspace && hasProfile && (
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
              disabled={dbProfileControls.busy || nextAction.disabled}
              title={nextAction.disabled ? (dbProfileControls.dbIndexBlockedReason ?? undefined) : undefined}
              data-source-action={
                nextAction.button === "다시 읽기" || nextAction.button === "DB 읽기" ? "db-index" : undefined
              }
            >
              {nextAction.button === "다시 읽기" && (
                <RefreshCw size={13} className={dbProfileControls.indexing ? "spin" : undefined} />
              )}
              <span>{nextAction.button}</span>
            </button>
          )}
        </div>
      )}
      {hasWorkspace && (
        <div className={`db-connection-summary ${hasProfile ? "connected" : "disconnected"}`}>
          <span>
            <b>{hasProfile ? "DB 연결됨" : "DB 연결 안 됨"}</b>
            <small>
              {hasProfile
                ? `${dbProfileControls.activeProfile?.name} · ${dbProfileSourceLabel(dbProfileControls.activeProfile?.source ?? dbProfileControls.profileSource)}`
                : "코드만 분석하거나 DB 구조를 추가할 수 있습니다."}
            </small>
          </span>
          {onEditDbConnection && (
            <button
              className="outline-action compact"
              type="button"
              onClick={onEditDbConnection}
              disabled={dbProfileControls.busy}
            >
              {hasProfile ? "편집" : "연결"}
            </button>
          )}
        </div>
      )}
      {hasInventory && (
        <div className="source-stat-grid" aria-label="DB 구조 요약">
          <span className={hasTables ? "ready" : ""}>
            <b>테이블</b>
            <em>{allTables.length}</em>
          </span>
          <span className={hasCompleteColumns ? "ready" : "warn"}>
            <b>컬럼</b>
            <em>{columnCount}</em>
          </span>
          <span className={foreignKeyCount > 0 ? "ready" : ""}>
            <b>FK</b>
            <em>{foreignKeyCount}</em>
          </span>
        </div>
      )}
      {!hasWorkspace ? null : (
        <>
          {hasProfile && (
            <details className="source-advanced">
              <summary>연결 관리</summary>
              <span className="secret-note">연결 정보는 편집 버튼에서 변경합니다.</span>
              <button
                className="outline-action compact danger-action source-delete-action"
                type="button"
                onClick={confirmDeleteProfile}
                disabled={dbProfileControls.busy}
                title="저장된 DB 연결과 로컬 구조 캐시 삭제"
              >
                <Trash2 size={13} />
                {dbProfileControls.deleting ? "삭제 중" : "DB 연결 삭제"}
              </button>
            </details>
          )}
          {showDbOperationMessage && (
            <span
              ref={operationMessageRef}
              className={`workspace-message ${dbProfileControls.error ? "error" : ""}`}
              role={dbProfileControls.error ? "alert" : undefined}
              tabIndex={dbProfileControls.error ? -1 : undefined}
            >
              {dbProfileControls.error ?? dbProfileControls.status}
            </span>
          )}
          {dbProfileControls.error && dbProfileControls.errorDetail && (
            <details className="error-details">
              <summary>상세 오류</summary>
              <pre>{dbProfileControls.errorDetail}</pre>
            </details>
          )}
        </>
      )}
    </section>
  );

  async function confirmDeleteProfile() {
    const profile = dbProfileControls.activeProfile;
    if (!profile) {
      return;
    }
    const confirmed = await confirmAction(
      `"${profile.name}" DB 연결을 삭제할까요?\n\n저장된 연결 정보와 로컬 구조 캐시만 삭제하며 DB 서버나 원본 파일은 변경하지 않습니다.`,
    );
    if (confirmed) {
      dbProfileControls.deleteProfile();
    }
  }
}

function dbNextAction(
  dbProfileControls: DbProfileControls,
  hasProfile: boolean,
  hasInventory: boolean,
  hasTables: boolean,
  hasColumns: boolean,
  missingColumnTables: number,
): {
  label: string;
  text: string;
  button?: string;
  run?: () => void;
  primary?: boolean;
  disabled?: boolean;
  tone?: "ready";
} {
  if (!hasProfile) {
    return {
      label: "DB 연결 대기",
      text: "DB 연결 버튼에서 연결 정보를 입력하세요.",
    };
  }
  if (hasInventory && hasTables) {
    const tableCount = dbInventoryTableCount(dbProfileControls.inventory);
    const columnCount = dbProfileControls.inventory?.tables.reduce((sum, table) => sum + table.columns.length, 0) ?? 0;
    if (!hasColumns) {
      if (dbProfileControls.dbIndexBlockedReason) {
        return {
          label: "컬럼 대기",
          text: dbProfileControls.dbIndexBlockedReason,
        };
      }
      return {
        label: "컬럼 대기",
        text: `테이블 ${tableCount}개만 읽힘 · 컬럼을 읽으면 관계가 열립니다.`,
        button: dbProfileControls.canIndexProfile ? "다시 읽기" : undefined,
        run: dbProfileControls.canIndexProfile ? dbProfileControls.indexProfile : undefined,
        primary: dbProfileControls.canIndexProfile,
        disabled: !dbProfileControls.canIndexProfile,
      };
    }
    if (missingColumnTables > 0) {
      return {
        label: "컬럼 보강",
        text: `테이블 ${tableCount}개 중 ${missingColumnTables}개는 컬럼을 더 읽어야 합니다.`,
        button: dbProfileControls.canIndexProfile ? "다시 읽기" : undefined,
        run: dbProfileControls.canIndexProfile ? dbProfileControls.indexProfile : undefined,
        primary: dbProfileControls.canIndexProfile,
        disabled: !dbProfileControls.canIndexProfile,
      };
    }
    return {
      label: "근거 준비됨",
      text: `테이블 ${tableCount}개 · 컬럼 ${columnCount}개 읽힘`,
      button: dbProfileControls.indexing ? "읽는 중" : "다시 읽기",
      run: dbProfileControls.indexProfile,
      disabled: !dbProfileControls.canIndexProfile,
      tone: "ready",
    };
  }
  if (hasInventory && !hasTables) {
    if (dbProfileControls.dbIndexBlockedReason) {
      return {
        label: "읽기 도구 필요",
        text: dbProfileControls.dbIndexBlockedReason,
      };
    }
    return {
      label: "비어 있음",
      text: "테이블이 없습니다. DB 연결과 권한을 확인하세요.",
      button: "다시 읽기",
      run: dbProfileControls.indexProfile,
      disabled: !dbProfileControls.canIndexProfile,
    };
  }
  if (!hasInventory) {
    if (!dbProfileControls.canIndexProfile) {
      if (dbProfileControls.dbIndexBlockedReason) {
        return {
          label: "읽기 도구 필요",
          text: dbProfileControls.dbIndexBlockedReason,
        };
      }
      return {
        label: "DB 연결 확인",
        text: "저장된 연결로 DB 구조를 읽을 수 없습니다.",
      };
    }
    return {
      label: "DB 읽기",
      text: "테이블, 컬럼, FK를 읽습니다.",
      button: "DB 읽기",
      run: dbProfileControls.indexProfile,
      primary: true,
      disabled: !dbProfileControls.canIndexProfile,
    };
  }
  return {
    label: dbProfileControls.dbIndexBlockedReason ? "읽기 도구 필요" : "비어 있음",
    text: dbProfileControls.dbIndexBlockedReason ?? "테이블이 없습니다. DB 연결과 권한을 확인하세요.",
    button: dbProfileControls.dbIndexBlockedReason ? undefined : "다시 읽기",
    run: dbProfileControls.dbIndexBlockedReason ? undefined : dbProfileControls.indexProfile,
    disabled: !dbProfileControls.canIndexProfile,
  };
}
