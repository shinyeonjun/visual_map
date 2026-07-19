import { CheckCircle2, ChevronRight, Database, Filter, Folder, Plus, RefreshCw, Search, Table2, Trash2, Type } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  DB_PROFILE_SOURCE_OPTIONS,
  dbInventoryTableKey,
  dbProfileSourceLabel,
  dbProfileSourceUsesPath,
} from "../../types/workspace";
import type { DbInventoryTable, DbProfileSource } from "../../types/workspace";
import type { DbProfileControls, VisualMapControls } from "../../types/controls";
import { focusDbProfileSetup as focusDbProfileInput } from "../common/focusSourceSetup";
import { PanelHeader } from "../common/PanelHeader";

const DB_TABLE_LIST_LIMIT = 80;

export function DatabaseSourceSection({
  dbProfileControls,
  visualMapControls,
}: {
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const operationMessageRef = useRef<HTMLSpanElement>(null);
  const sourceUsesPath = dbProfileSourceUsesPath(dbProfileControls.profileSource);
  const sourceCopy = dbSourceCopy[dbProfileControls.profileSource];
  const [tableFilter, setTableFilter] = useState("");
  const filter = tableFilter.trim().toLowerCase();
  const hasWorkspace = dbProfileControls.hasWorkspace;
  const allTables = dbProfileControls.inventory?.tables ?? [];
  const hasProfile = Boolean(dbProfileControls.activeProfile);
  const hasInventory = Boolean(dbProfileControls.inventory);
  const hasTables = allTables.length > 0;
  const hasColumns = allTables.some((table) => table.columns.length > 0);
  const missingColumnTables = allTables.filter((table) => table.columns.length === 0).length;
  const hasCompleteColumns = hasTables && missingColumnTables === 0;
  const columnCount = allTables.reduce((sum, table) => sum + table.columns.length, 0);
  const foreignKeyCount = allTables.reduce(
    (sum, table) => sum + Math.max(table.foreignKeys?.length ?? 0, table.columns.filter((column) => column.isForeignKey).length),
    0,
  );
  const isSnapshotInventory = hasInventory && !hasProfile;
  const profileMatchesForm = dbProfileMatchesForm(dbProfileControls, sourceUsesPath);
  const saveProfileLabel = dbProfileControls.canSaveProfile
    ? "DB 연결 저장"
    : profileMatchesForm
      ? "현재 DB 연결 저장됨"
      : "연결 정보 대기";
  const snapshotProfileHint = !isSnapshotInventory
    ? null
    : dbProfileControls.canSaveProfile
      ? "DB 연결을 저장하면 새로고침과 다시 읽기를 사용할 수 있습니다."
      : "저장하려면 연결 이름과 DB 정보를 입력하세요.";
  const requirementCopy = isSnapshotInventory
    ? "저장된 DB 구조를 복구했습니다. 다시 읽으려면 DB 연결을 저장하세요."
    : dbRequirementCopy(sourceCopy.required, hasInventory, hasTables, hasColumns, sourceUsesPath);
  const matchingTables = filter
    ? allTables.filter((table) => dbInventoryTableKey(table).toLowerCase().includes(filter))
    : allTables;
  const tables = limitedTables(matchingTables, dbProfileControls.selectedTableKey, DB_TABLE_LIST_LIMIT);
  const hiddenTableCount = Math.max(0, matchingTables.length - tables.length);
  const focusedMapId = visualMapControls.selectedNode?.id ?? visualMapControls.currentMap?.focus ?? "";
  const nextAction = dbNextAction(dbProfileControls, hasProfile, hasInventory, hasTables, hasColumns, missingColumnTables);
  const compactReady = hasInventory && (profileMatchesForm || isSnapshotInventory);
  const showDbOperationMessage = Boolean(
    dbProfileControls.error ||
      dbProfileControls.saving ||
      dbProfileControls.testing ||
      dbProfileControls.indexing ||
      dbProfileControls.loading ||
      (!hasInventory && dbProfileControls.status),
  );
  const sourceSettings = (
    <>
      <div className="meta-row">
        <label htmlFor="db-profile-source-select">연결 유형</label>
        <select
          id="db-profile-source-select"
          className="inline-select"
          value={dbProfileControls.profileSource}
          onChange={(event) => dbProfileControls.setProfileSource(event.currentTarget.value as DbProfileSource)}
        >
          {DB_PROFILE_SOURCE_OPTIONS.map((source) => (
            <option key={source.value} value={source.value}>
              {source.label}
            </option>
          ))}
        </select>
      </div>
      <span className="secret-note">{sourceCopy.help}</span>
      <span className="secret-note">행 데이터는 조회하지 않고 구조 정보만 읽습니다.</span>
      <label className="field-label" htmlFor="db-profile-target-input">
        {sourceCopy.label}
      </label>
      {sourceUsesPath ? (
        <div className="path-row">
          <input
            id="db-profile-target-input"
            className="workspace-input mono"
            value={dbProfileControls.profilePath}
            onChange={(event) => dbProfileControls.setProfilePath(event.currentTarget.value)}
            placeholder={sourceCopy.placeholder}
          />
          <button
            className="square-button"
            type="button"
            onClick={dbProfileControls.pickPath}
            disabled={dbProfileControls.busy}
            aria-label={`${sourceCopy.label} 선택`}
          >
            <Folder size={14} />
          </button>
        </div>
      ) : (
        <>
          <input
            id="db-profile-target-input"
            className="workspace-input mono"
            type="password"
            value={dbProfileControls.connectionString}
            onChange={(event) => dbProfileControls.setConnectionString(event.currentTarget.value)}
            placeholder={sourceCopy.placeholder}
          />
          <span className="secret-note">세션에서만 사용하며 프로젝트 파일에 저장하지 않습니다.</span>
        </>
      )}
    </>
  );

  useEffect(() => {
    if (dbProfileControls.error) {
      operationMessageRef.current?.focus();
    }
  }, [dbProfileControls.error]);

  return (
    <section className={`side-card database-source ${hasWorkspace ? "" : "locked"} ${compactReady ? "ready" : ""}`}>
      <PanelHeader icon={<Database size={16} />} title="데이터베이스" />
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
            >
              <span>{nextAction.button}</span>
            </button>
          )}
        </div>
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
          {isSnapshotInventory ? (
            <details className="source-advanced">
              <summary>저장된 구조 연결 저장 / 설정</summary>
              <label className="field-label" htmlFor="db-profile-name-input">
                저장된 DB 구조
              </label>
              <div className="profile-line">
                <div className="profile-row db-profile-name">
                  <span className="ok-dot pending" />
                  <input
                    id="db-profile-name-input"
                    className="inline-input"
                    value={dbProfileControls.profileName}
                    onChange={(event) => dbProfileControls.setProfileName(event.currentTarget.value)}
                    placeholder="연결 이름"
                  />
                </div>
                <button
                  className="primary-action compact"
                  type="button"
                  onClick={dbProfileControls.saveProfile}
                  disabled={!dbProfileControls.canSaveProfile || dbProfileControls.busy}
                  aria-label={saveProfileLabel}
                  title={saveProfileLabel}
                >
                  {dbProfileControls.saving ? "저장 중" : "연결 저장"}
                </button>
              </div>
              {snapshotProfileHint && (
                <span className={`secret-note ${dbProfileControls.canSaveProfile ? "ready-note" : ""}`}>
                  {snapshotProfileHint}
                </span>
              )}
              <span className="secret-note ready-note">현재 화면은 저장된 DB 구조를 읽고 있습니다.</span>
              {sourceSettings}
            </details>
          ) : (
            <>
              {compactReady ? (
                <details className="source-advanced">
                  <summary>DB 연결 / 다시 읽기</summary>
                  <label className="field-label" htmlFor="db-profile-name-input">
                    활성 DB 연결
                  </label>
                  <div className="profile-line">
                    <div className="profile-row db-profile-name">
                      <span className="ok-dot" />
                      <input
                        id="db-profile-name-input"
                        className="inline-input"
                        value={dbProfileControls.profileName}
                        onChange={(event) => dbProfileControls.setProfileName(event.currentTarget.value)}
                        placeholder="연결 이름"
                      />
                    </div>
                    <button
                      className="outline-action compact profile-save-action"
                      type="button"
                      onClick={dbProfileControls.saveProfile}
                      disabled={!dbProfileControls.canSaveProfile || dbProfileControls.busy}
                      aria-label={saveProfileLabel}
                      title={saveProfileLabel}
                    >
                      {dbProfileControls.saving ? <RefreshCw size={14} className="spin" /> : <CheckCircle2 size={14} />}
                      {dbProfileControls.saving ? "저장 중" : "저장됨"}
                    </button>
                  </div>
                  {dbProfileControls.activeProfile && (
                    <span className="secret-note">
                      연결 방식: {dbProfileSourceLabel(dbProfileControls.activeProfile.source)}
                    </span>
                  )}
                  <span className="secret-note ready-note">{requirementCopy}</span>
                  {sourceSettings}
                  <div className="source-maintenance three" aria-label="데이터베이스 다시 읽기">
                    <button
                      className="outline-action compact"
                      type="button"
                      onClick={dbProfileControls.loadInventory}
                      disabled={!dbProfileControls.canLoadInventory || dbProfileControls.busy}
                    >
                      {dbProfileControls.loading ? "불러오는 중" : "테이블 새로고침"}
                      <Database size={13} />
                    </button>
                    <button
                      className="outline-action compact"
                      type="button"
                      onClick={dbProfileControls.testConnection}
                      disabled={!dbProfileControls.canTestConnection || dbProfileControls.busy}
                      title={dbProfileControls.dbIndexBlockedReason ?? undefined}
                    >
                      {dbProfileControls.testing ? "테스트 중" : "구조 테스트"}
                      <Database size={13} />
                    </button>
                    <button
                      className="outline-action compact"
                      type="button"
                      onClick={dbProfileControls.indexProfile}
                      disabled={!dbProfileControls.canIndexProfile || dbProfileControls.busy}
                      title={dbProfileControls.dbIndexBlockedReason ?? undefined}
                    >
                      <RefreshCw size={13} className={dbProfileControls.indexing ? "spin" : undefined} />
                      {dbProfileControls.indexing ? "읽는 중" : "다시 읽기"}
                    </button>
                  </div>
                  {dbProfileControls.activeProfile && (
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
                  )}
                </details>
              ) : (
                <>
                  <label className="field-label" htmlFor="db-profile-name-input">
                    {hasProfile ? "활성 DB 연결" : "새 DB 연결"}
                  </label>
                  <div className="profile-line">
                    <div className="profile-row db-profile-name">
                      <span className={`ok-dot ${hasProfile ? "" : "pending"}`} />
                      <input
                        id="db-profile-name-input"
                        className="inline-input"
                        value={dbProfileControls.profileName}
                        onChange={(event) => dbProfileControls.setProfileName(event.currentTarget.value)}
                        placeholder="연결 이름"
                      />
                    </div>
                    <button
                      className="outline-action compact profile-save-action"
                      type="button"
                      onClick={dbProfileControls.saveProfile}
                      disabled={!dbProfileControls.canSaveProfile || dbProfileControls.busy}
                      aria-label={saveProfileLabel}
                      title={saveProfileLabel}
                    >
                      {dbProfileControls.saving ? (
                        <RefreshCw size={14} className="spin" />
                      ) : profileMatchesForm ? (
                        <CheckCircle2 size={14} />
                      ) : (
                        <Plus size={14} />
                      )}
                      {dbProfileControls.saving ? "저장 중" : profileMatchesForm ? "저장됨" : "저장"}
                    </button>
                  </div>
                  {dbProfileControls.activeProfile && (
                    <span className="secret-note">
                      연결 방식: {dbProfileSourceLabel(dbProfileControls.activeProfile.source)}
                    </span>
                  )}
                  <span className={`secret-note ${hasInventory ? "ready-note" : ""}`}>{requirementCopy}</span>
                  {sourceSettings}
                  {dbProfileControls.activeProfile && (
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
                  )}
                </>
              )}
            </>
          )}
          {hasInventory && !compactReady && !isSnapshotInventory && (
            <div className="source-maintenance three" aria-label="데이터베이스 연결 관리">
              <button
                className="outline-action compact"
                type="button"
                onClick={dbProfileControls.testConnection}
                disabled={!dbProfileControls.canTestConnection || dbProfileControls.busy}
                title={dbProfileControls.dbIndexBlockedReason ?? undefined}
              >
                {dbProfileControls.testing ? "테스트 중" : "구조 테스트"}
                <Database size={13} />
              </button>
              <button
                className="outline-action compact"
                type="button"
                onClick={dbProfileControls.indexProfile}
                disabled={!dbProfileControls.canIndexProfile || dbProfileControls.busy}
                title={dbProfileControls.dbIndexBlockedReason ?? undefined}
              >
                <RefreshCw size={13} className={dbProfileControls.indexing ? "spin" : undefined} />
                {dbProfileControls.indexing ? "읽는 중" : "다시 읽기"}
              </button>
              <button
                className="outline-action compact"
                type="button"
                onClick={dbProfileControls.loadInventory}
                disabled={!dbProfileControls.canLoadInventory || dbProfileControls.busy}
              >
                {dbProfileControls.loading ? "불러오는 중" : "테이블 새로고침"}
                <Database size={13} />
              </button>
            </div>
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
          {hasInventory && (
            <>
              <div className="tabs" role="tablist" aria-label="DB 구조 유형">
                <span className="active" role="tab" aria-selected="true">
                  테이블 <span>{dbProfileControls.inventory?.tables.length ?? 0}</span>
                </span>
              </div>
              <div className="filter-input">
                <Search size={13} />
                <input
                  id="db-table-filter-input"
                  aria-label="테이블 필터"
                  value={tableFilter}
                  onChange={(event) => setTableFilter(event.currentTarget.value)}
                  placeholder="테이블 필터..."
                />
                <Filter size={13} />
              </div>
              <div className="list table-list">
                {tables.map((table) => {
                  const tableKey = dbInventoryTableKey(table);
                  const active = tableKey === dbProfileControls.selectedTableKey;
                  const needsColumns = table.columns.length === 0;
                  return (
                    <div className={`table-block ${active ? "open active" : ""}`} key={tableKey}>
                      <button
                        className={`table-row table-button ${active ? "active" : ""} ${needsColumns ? "needs-columns" : ""}`}
                        type="button"
                        aria-expanded={active}
                        aria-label={needsColumns ? `${table.name} 컬럼 대기` : undefined}
                        title={`${tableKey} · ${needsColumns ? "컬럼 대기" : `${table.columns.length}개 컬럼`}`}
                        onClick={() => dbProfileControls.openTable(tableKey)}
                      >
                        <ChevronRight size={13} />
                        <Table2 size={14} />
                        <span className="table-copy">
                          <span>{table.name}</span>
                          {table.schema && <small>{table.schema}</small>}
                        </span>
                        <em className={needsColumns ? "warn" : ""}>
                          {needsColumns ? "컬럼 대기" : `${table.columns.length}개 컬럼`}
                        </em>
                      </button>
                      {active && needsColumns && (
                        <div className="column-row column-empty">
                          <span>컬럼을 읽으면 관계와 변경 범위가 열립니다.</span>
                        </div>
                      )}
                      {active &&
                        table.columns.slice(0, 6).map((column) => (
                          <button
                            className={`column-row column-button ${
                              focusedMapId === `db:column:${tableKey}:${column.name}` ? "active" : ""
                            }`}
                            type="button"
                            key={`${tableKey}:${column.name}`}
                            aria-label={`${table.name}.${column.name} 컬럼 선택`}
                            onClick={() => dbProfileControls.openColumn(tableKey, column.name)}
                          >
                            <Type size={12} />
                            <span>{column.name}</span>
                            {column.isPrimaryKey && <small>PK</small>}
                            {column.isForeignKey && <small>FK</small>}
                            <code>{column.dataType ?? "타입 ?"}</code>
                          </button>
                        ))}
                      {active && table.columns.length > 6 && (
                        <div className="column-row">
                          <span className="row-more">+{table.columns.length - 6}개 더</span>
                        </div>
                      )}
                    </div>
                  );
                })}
                {hiddenTableCount > 0 && (
                  <span className="workspace-empty">
                    {tables.length}개 표시 · +{hiddenTableCount}개 · 필터로 좁히세요
                  </span>
                )}
                {tables.length === 0 && (
                  <span className="workspace-empty">
                    {allTables.length > 0 ? "필터와 일치하는 테이블이 없습니다" : "테이블 목록이 비어 있습니다"}
                  </span>
                )}
              </div>
            </>
          )}
        </>
      )}
    </section>
  );

  function confirmDeleteProfile() {
    const profile = dbProfileControls.activeProfile;
    if (!profile) {
      return;
    }
    const confirmed = window.confirm(
      `\"${profile.name}\" DB 연결을 삭제할까요?\n\n저장된 연결 정보와 로컬 구조 캐시만 삭제하며 DB 서버나 원본 파일은 변경하지 않습니다.`,
    );
    if (confirmed) {
      dbProfileControls.deleteProfile();
    }
  }
}

function limitedTables(items: DbInventoryTable[], selectedKey: string | null, limit: number): DbInventoryTable[] {
  const visible = items.slice(0, limit);
  if (!selectedKey) {
    return visible;
  }
  if (visible.some((item) => dbInventoryTableKey(item) === selectedKey)) {
    return visible;
  }
  const selected = items.find((item) => dbInventoryTableKey(item) === selectedKey);
  return selected ? [selected, ...visible.slice(0, Math.max(0, limit - 1))] : visible;
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
  const canSaveMissingProfile = !hasProfile && dbProfileControls.canSaveProfile;
  if (hasInventory && hasTables) {
    const tableCount = dbProfileControls.inventory?.tables.length ?? 0;
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
        button: canSaveMissingProfile ? "연결 저장" : dbProfileControls.canIndexProfile ? "다시 읽기" : "DB 정보 입력",
        run: canSaveMissingProfile
          ? dbProfileControls.saveProfile
          : dbProfileControls.canIndexProfile
            ? dbProfileControls.indexProfile
            : () => focusDbProfileInput(dbProfileControls),
        primary: canSaveMissingProfile || dbProfileControls.canIndexProfile,
        disabled: dbProfileControls.canIndexProfile ? false : undefined,
      };
    }
    if (missingColumnTables > 0) {
      return {
        label: "컬럼 보강",
        text: `테이블 ${tableCount}개 중 ${missingColumnTables}개는 컬럼을 더 읽어야 합니다.`,
        button: canSaveMissingProfile ? "연결 저장" : dbProfileControls.canIndexProfile ? "다시 읽기" : "DB 정보 입력",
        run: canSaveMissingProfile
          ? dbProfileControls.saveProfile
          : dbProfileControls.canIndexProfile
            ? dbProfileControls.indexProfile
            : () => focusDbProfileInput(dbProfileControls),
        primary: canSaveMissingProfile || dbProfileControls.canIndexProfile,
      };
    }
    return {
      label: "근거 준비됨",
      text: `테이블 ${tableCount}개 · 컬럼 ${columnCount}개 읽힘`,
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
  if (!hasProfile) {
    if (!dbProfileControls.hasWorkspace) {
      return {
        label: "프로젝트 열기",
        text: "프로젝트를 열면 DB를 등록합니다.",
      };
    }
    return dbProfileControls.canSaveProfile
      ? {
          label: "연결 저장",
          text: "저장 후 DB 구조를 읽습니다.",
          button: "연결 저장",
          run: dbProfileControls.saveProfile,
          primary: true,
        }
      : {
          label: "DB 정보 입력",
          text: "연결 정보를 입력합니다.",
          button: "DB 정보 입력",
          run: () => focusDbProfileInput(dbProfileControls),
        };
  }
  if (!hasInventory) {
    const indexed = dbProfileControls.status?.includes("완료") ?? false;
    if (indexed) {
      return {
        label: "DB 목록 표시",
        text: "읽은 테이블과 컬럼을 목록에 표시합니다.",
        button: "DB 목록 열기",
        run: dbProfileControls.loadInventory,
        primary: true,
      };
    }
    if (!dbProfileControls.canIndexProfile) {
      if (dbProfileControls.dbIndexBlockedReason) {
        return {
          label: "읽기 도구 필요",
          text: dbProfileControls.dbIndexBlockedReason,
        };
      }
      return {
        label: "DB 정보 입력",
        text: "연결 정보를 입력합니다.",
        button: "DB 정보 입력",
        run: () => focusDbProfileInput(dbProfileControls),
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

function dbProfileMatchesForm(dbProfileControls: DbProfileControls, sourceUsesPath: boolean): boolean {
  const activeProfile = dbProfileControls.activeProfile;
  if (!activeProfile) {
    return false;
  }
  return (
    activeProfile.name === dbProfileControls.profileName.trim() &&
    activeProfile.source === dbProfileControls.profileSource &&
    (!sourceUsesPath || (activeProfile.path ?? "") === dbProfileControls.profilePath.trim())
  );
}

function dbRequirementCopy(
  required: string,
  hasInventory: boolean,
  hasTables: boolean,
  hasColumns: boolean,
  sourceUsesPath: boolean,
): string {
  if (!hasInventory) {
    return required;
  }
  if (!hasTables) {
    return sourceUsesPath
      ? "DB 구조를 읽었지만 테이블이 없습니다. 경로와 스키마 내용을 확인하세요."
      : "DB 구조를 읽었지만 테이블이 없습니다. 연결 정보와 DB 권한을 확인하세요.";
  }
  if (!hasColumns) {
    return sourceUsesPath
      ? "테이블 목록만 읽혔습니다. 컬럼 정의를 포함한 스키마로 다시 읽으세요."
      : "테이블 목록만 읽혔습니다. 연결 정보와 컬럼 조회 권한을 확인한 뒤 다시 읽으세요.";
  }
  return sourceUsesPath
    ? "DB 구조 읽힘. 경로를 바꾸면 다시 읽기로 갱신합니다."
    : "DB 구조 읽힘. 다시 읽을 때만 연결 문자열을 사용합니다.";
}

const dbSourceCopy: Record<
  DbProfileSource,
  { label: string; placeholder: string; help: string; required: string }
> = {
  "ddl-sqlite": {
    label: "DDL 파일/폴더",
    placeholder: "D:\\path\\to\\schema.sql",
    help: "SQL DDL 파일 또는 디렉터리의 스키마 정의를 읽습니다.",
    required: "필수: 연결 이름, DDL 파일/디렉터리 위치",
  },
  sqlite: {
    label: "SQLite DB 파일",
    placeholder: "D:\\path\\to\\database.sqlite",
    help: "SQLite 파일의 PRAGMA/catalog 구조를 읽습니다.",
    required: "필수: 연결 이름, SQLite 파일 위치",
  },
  postgres: {
    label: "PostgreSQL 연결 문자열",
    placeholder: "postgres://user:password@localhost:5432/db",
    help: "PostgreSQL catalog 구조를 읽습니다.",
    required: "필수: 연결 저장 후 이번 세션의 연결 문자열",
  },
  mysql: {
    label: "MySQL/MariaDB 연결 문자열",
    placeholder: "mysql://user:password@localhost:3306/db",
    help: "MySQL/MariaDB information_schema 구조를 읽습니다.",
    required: "필수: 연결 저장 후 이번 세션의 연결 문자열",
  },
  sqlserver: {
    label: "SQL Server 연결 문자열",
    placeholder: "Server=localhost;Database=db;User Id=user;Password=password;",
    help: "SQL Server catalog 구조를 읽습니다.",
    required: "필수: 연결 저장 후 이번 세션의 연결 문자열",
  },
  oracle: {
    label: "Oracle 연결 문자열",
    placeholder: "user/password@localhost/XEPDB1",
    help: "Oracle catalog 구조를 읽습니다.",
    required: "필수: 연결 저장 후 이번 세션의 연결 문자열",
  },
};
