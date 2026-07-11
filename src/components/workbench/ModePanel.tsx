import { useEffect, useRef, useState } from "react";
import {
  codeInventoryCodeItems,
  codeInventoryDefaultRoute,
  codeInventoryItemCount,
  dbInventoryTableKey,
} from "../../types/workspace";
import type { DbProfileControls, WorkspaceControls } from "../../types/controls";
import type { VisualMapControls } from "../../types/controls";
import { focusDbProfileSetup } from "../common/focusSourceSetup";
import { focusGlobalSearch } from "../common/focusGlobalSearch";
import { Star, workbenchModes } from "./workbenchModes";

export function ModePanel({
  workspaceControls,
  dbProfileControls,
  visualMapControls,
}: {
  workspaceControls: WorkspaceControls;
  dbProfileControls: DbProfileControls;
  visualMapControls: VisualMapControls;
}) {
  const noteRef = useRef<HTMLDivElement>(null);
  const [note, setNote] = useState<ModeUnlockHint | null>(null);
  const readiness = workbenchReadiness(workspaceControls, dbProfileControls, visualMapControls);
  const displayedNote = note;
  const mapHasNoRelations = Boolean(visualMapControls.currentMap && visualMapControls.currentMap.edges.length === 0);
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const showCardLockReasons = hasWorkspace;

  useEffect(() => {
    setNote(null);
  }, [
    workspaceControls.repoSourceMode,
    workspaceControls.currentWorkspace?.id,
    workspaceControls.canCreateWorkspace,
    workspaceControls.codeStatus,
    workspaceControls.canIndexCode,
    workspaceControls.codeIndexBlockedReason,
    workspaceControls.codeInventory,
    dbProfileControls.activeProfile,
    dbProfileControls.status,
    dbProfileControls.dbIndexBlockedReason,
    dbProfileControls.inventory,
  ]);

  useEffect(() => {
    if (note) {
      noteRef.current?.focus();
    }
  }, [note]);

  return (
    <section className="side-card" aria-label="찾을 답">
      <div className="panel-header">
        <Star size={16} />
        <h2>찾을 답</h2>
      </div>
      <div className="mode-list">
        <div className={`mode-readiness ${readiness.tone}`}>
          <span>{readiness.badge}</span>
          <strong>{readiness.title}</strong>
          <small>{readiness.text}</small>
        </div>
        <div className={`mode-note-slot ${displayedNote ? "visible" : ""}`} aria-live="polite">
          {displayedNote && (
            <div
              ref={noteRef}
              className={`mode-note ${displayedNote.run ? "" : "passive"}`}
              tabIndex={note ? -1 : undefined}
            >
              <span>{displayedNote.reason}</span>
              {displayedNote.run && (
                <button
                  className={displayedNote.primary ? "primary-action compact" : "outline-action compact"}
                  type="button"
                  onClick={() => {
                    displayedNote.run?.();
                    setNote(null);
                  }}
                  disabled={displayedNote.disabled || workspaceControls.busy || dbProfileControls.busy}
                >
                  {displayedNote.button}
                </button>
              )}
            </div>
          )}
        </div>
        {!hasWorkspace && (
          <div className="mode-empty-hint">
            프로젝트를 열면 이곳에서 전체 구조, API 흐름, 테이블 연결, 컬럼 변경 범위를 바로 전환합니다.
          </div>
        )}
        {hasWorkspace && workbenchModes.map(([ModeIcon, mode, title, text], index) => {
          const blockReason = workbenchModeBlockReason(mode, workspaceControls, dbProfileControls);
          const active = !blockReason && visualMapControls.mode === mode;
          const displayTitle = modeTitle(mode, title, workspaceControls, dbProfileControls, mapHasNoRelations);
          const displayText = modeText(mode, text, workspaceControls, dbProfileControls, mapHasNoRelations);
          const displayAnswer = modeAnswerLabel(mode, workspaceControls, dbProfileControls, mapHasNoRelations);
          const unlockHint = blockReason ? workbenchModeUnlockHint(mode, blockReason, workspaceControls, dbProfileControls, visualMapControls) : null;
          const disabledUntilProject = Boolean(blockReason && !workspaceControls.currentWorkspace);
          const cardBody = (
            <>
              <span className="mode-step" aria-hidden="true">{index + 1}</span>
              <span className="mode-icon">
                <ModeIcon size={16} />
              </span>
              <span className="mode-text">
                <strong>{displayTitle}</strong>
                <small>{displayText}</small>
                <span className="mode-answer">{displayAnswer}</span>
                <em className="mode-source">{modeDataLabel(mode, workspaceControls, dbProfileControls, mapHasNoRelations)}</em>
                {blockReason && showCardLockReasons && <small className="mode-lock-reason">{blockReason}</small>}
              </span>
            </>
          );
          if (disabledUntilProject) {
            return (
              <div
                className="mode-card locked passive"
                key={title}
                aria-label={`찾을 답: ${displayAnswer}: ${blockReason}`}
                title={blockReason ?? undefined}
              >
                {cardBody}
              </div>
            );
          }
          return (
            <button
              className={`mode-card ${active ? "active" : ""} ${blockReason ? "locked" : ""}`}
              key={title}
              type="button"
              onClick={() => {
                if (blockReason) {
                  setNote(unlockHint);
                  return;
                }
                setNote(null);
                showWorkbenchMode(mode, workspaceControls, dbProfileControls, visualMapControls);
                if (mode === "search-focus") {
                  focusGlobalSearch(visualMapControls);
                }
              }}
              aria-pressed={active}
              aria-label={`찾을 답: ${displayAnswer}: ${blockReason ?? displayText}`}
              title={blockReason ?? undefined}
              disabled={workspaceControls.busy || dbProfileControls.busy}
            >
              {cardBody}
            </button>
          );
        })}
      </div>
    </section>
  );
}

function workbenchReadiness(
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): {
  badge: string;
  title: string;
  text: string;
  tone: "pending" | "partial" | "ready";
} {
  const hasWorkspace = Boolean(workspaceControls.currentWorkspace);
  const hasRoutes = Boolean(workspaceControls.codeInventory?.routes.length);
  const hasCodeItems = codeInventoryCodeItems(workspaceControls.codeInventory).length > 0;
  const codeSymbolCount = codeInventoryCodeItems(workspaceControls.codeInventory).length;
  const fileCount = workspaceControls.codeInventory?.files.length ?? 0;
  const hasCodeContext = hasRoutes || hasCodeItems;
  const hasCodeInventory = Boolean(workspaceControls.codeInventory);
  const tables = dbProfileControls.inventory?.tables ?? [];
  const hasTables = Boolean(tables.length);
  const hasColumns = Boolean(firstColumn(dbProfileControls));
  const missingColumnTables = tables.filter((table) => table.columns.length === 0).length;
  const hasAnyInventory = Boolean(workspaceControls.codeInventory || dbProfileControls.inventory);
  const hasSearchable = hasSearchableItems(workspaceControls, dbProfileControls);
  const openModes = 1 + Number(hasRoutes) + Number(hasTables) + Number(hasColumns) + Number(hasSearchable);
  const needsGithub = workspaceControls.repoSourceMode === "github";
  const mapHasNoRelations = Boolean(
    visualMapControls.currentMap &&
      visualMapControls.currentMap.nodes.length === 0 &&
      visualMapControls.currentMap.edges.length === 0,
  );

  if (!hasWorkspace) {
    if (workspaceControls.canCreateWorkspace) {
      return {
        badge: "0/5",
        title: needsGithub ? "저장소 복제 준비" : "프로젝트 열기 준비",
        text: needsGithub ? "복제하면 답을 고를 수 있습니다." : "열면 답을 고를 수 있습니다.",
        tone: "pending",
      };
    }
    return {
      badge: "0/5",
      title: needsGithub ? "GitHub URL 필요" : "로컬 폴더 필요",
      text: needsGithub ? "URL을 붙여넣으면 복제해서 열 수 있습니다." : "폴더를 지정하면 열기 단계가 보입니다.",
      tone: "pending",
    };
  }
  if (mapHasNoRelations && (hasSearchable || hasTables) && !(hasTables && !hasColumns) && missingColumnTables === 0) {
    return {
      badge: `${openModes}/5`,
      title: "코드/DB 목록만 있음",
      text: "관계는 아직 없고 실제 카드부터 확인합니다.",
      tone: "partial",
    };
  }
  if (hasColumns && missingColumnTables > 0) {
    return {
      badge: `${openModes}/5`,
      title: "컬럼 보강 필요",
      text: `테이블 ${missingColumnTables}개는 컬럼 보강이 필요합니다.`,
      tone: "partial",
    };
  }
  if (openModes === workbenchModes.length) {
    return {
      badge: "5/5",
      title: "답 찾기 가능",
      text: "대상을 좁히고 API·테이블·컬럼 근거 확인",
      tone: "ready",
    };
  }
  if (hasRoutes && !hasTables) {
    return {
      badge: `${openModes}/5`,
      title: "API 라우트 확인",
      text: "DB 구조를 불러오면 변경 범위까지 볼 수 있습니다.",
      tone: "partial",
    };
  }
  if (hasTables) {
    return {
      badge: `${openModes}/5`,
      title: hasColumns ? (hasCodeContext ? "키/후보 확인" : "컬럼 제약 가능") : "컬럼 구조 필요",
      text: hasColumns
        ? hasRoutes
          ? "API 경로와 DB 키/후보 확인"
          : hasCodeItems
            ? "API 라우트가 없어 DB 제약과 코드 후보를 봅니다."
            : hasCodeInventory
              ? "코드 항목이 없어 DB 구조와 컬럼 제약을 봅니다."
              : "코드 없이도 DB 구조와 컬럼 제약을 볼 수 있습니다."
        : "컬럼 구조를 불러오면 변경 범위가 열립니다.",
      tone: "partial",
    };
  }
  if (hasSearchable) {
    if (hasCodeItems && !hasRoutes && !hasTables) {
      return {
        badge: `${openModes}/5`,
        title: codeSymbolCount > 0 ? "코드 주변 근거" : fileCount > 0 ? "파일 주변 근거" : "대상 좁히기 가능",
        text: codeSymbolCount > 0 ? "코드나 파일을 고르면 해당 항목으로 좁혀 볼 수 있습니다." : "파일을 고르면 해당 항목으로 좁혀 볼 수 있습니다.",
        tone: "partial",
      };
    }
    return {
      badge: `${openModes}/5`,
      title: "대상 좁히기 가능",
      text: "라우트나 테이블을 고르면 더 좁혀 볼 수 있습니다.",
      tone: "partial",
    };
  }
  if (hasAnyInventory) {
    return {
      badge: `${openModes}/5`,
      title: "읽은 결과가 비어 있음",
      text: "코드, 파일, 테이블 중 하나가 잡혀야 대상을 좁힐 수 있습니다.",
      tone: "pending",
    };
  }
  return {
    badge: "0/5",
    title: "코드/DB 필요",
    text: "코드 또는 DB 목록을 불러오면 답을 고를 수 있습니다.",
    tone: "pending",
  };
}

type ModeUnlockHint = {
  reason: string;
  button?: string;
  run?: () => void;
  primary?: boolean;
  disabled?: boolean;
};

function modeTitle(
  mode: string,
  fallback: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  if (!workspaceControls.currentWorkspace) {
    return fallback;
  }
  if (mode === "api-flow" && !workspaceControls.codeInventory) {
    return "API 진입점 보기";
  }
  if (mode === "api-flow" && workspaceControls.codeInventory && workspaceControls.codeInventory.routes.length === 0) {
    return "API 라우트 없음";
  }
  if (mode === "api-flow" && mapHasNoRelations) {
    return "API 진입점 보기";
  }
  const hasCodeContext = workbenchHasCodeContext(workspaceControls);
  const hasTables = Boolean(dbProfileControls.inventory?.tables.length);
  const hasColumns = Boolean(firstColumn(dbProfileControls));
  if (mode === "table-usage" && hasTables && !hasColumns) {
    return "테이블 목록 확인";
  }
  if (mode === "table-usage" && hasTables && mapHasNoRelations) {
    return "테이블 구조 보기";
  }
  if (mode === "table-usage" && dbProfileControls.inventory?.tables.length && !hasCodeContext) {
    return "테이블 구조 보기";
  }
  if (mode === "table-usage" && !dbProfileControls.inventory?.tables.length) {
    return "테이블 사용처 보기";
  }
  if (mode === "column-impact" && firstColumn(dbProfileControls) && !hasCodeContext) {
    return "컬럼 제약 보기";
  }
  if (mode === "column-impact" && firstColumn(dbProfileControls) && mapHasNoRelations) {
    return "컬럼 속성 보기";
  }
  if (mode === "column-impact" && !firstColumn(dbProfileControls)) {
    return dbProfileControls.inventory?.tables.length ? "컬럼 영향 보기" : "컬럼 변경 범위";
  }
  return fallback;
}

function modeText(
  mode: string,
  fallback: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  if (!workspaceControls.currentWorkspace) {
    return fallback;
  }
  const hasRoutes = Boolean(workspaceControls.codeInventory?.routes.length);
  const hasCodeContext = workbenchHasCodeContext(workspaceControls);
  if (mode === "api-flow") {
    if (!workspaceControls.codeInventory) {
      return "코드 읽은 뒤 확인";
    }
    if (!hasRoutes) {
      return "라우트가 잡히면 선택";
    }
    if (mapHasNoRelations) {
      return "라우트 구조 확인";
    }
    return fallback;
  }
  if (mode === "table-usage" && !dbProfileControls.inventory?.tables.length) {
    return "DB 연결 후 사용처 확인";
  }
  if (mode === "table-usage" && dbProfileControls.inventory?.tables.length && !firstColumn(dbProfileControls)) {
    return "컬럼 불러오면 관계 확인";
  }
  if (mode === "table-usage" && dbProfileControls.inventory?.tables.length && !hasCodeContext) {
    return "PK/FK 컬럼 확인";
  }
  if (mode === "table-usage" && dbProfileControls.inventory?.tables.length && mapHasNoRelations) {
    return "테이블/컬럼 구조 확인";
  }
  if (mode === "column-impact" && !firstColumn(dbProfileControls)) {
    return dbProfileControls.inventory?.tables.length ? "컬럼 읽으면 선택" : "DB 연결 후 범위 확인";
  }
  if (mode === "column-impact" && firstColumn(dbProfileControls) && !hasCodeContext) {
    return "컬럼 제약 확인";
  }
  if (mode === "column-impact" && firstColumn(dbProfileControls) && mapHasNoRelations) {
    return "컬럼 타입과 키 확인";
  }
  if (mode === "search-focus" && workspaceControls.codeInventory && !hasRoutes) {
    return "코드/파일 선택";
  }
  if (mode === "search-focus" && dbProfileControls.inventory?.tables.length && !hasCodeContext) {
    return "테이블 선택";
  }
  return fallback;
}

function modeDataLabel(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  const hasCodeContext = workbenchHasCodeContext(workspaceControls);
  if (!workspaceControls.currentWorkspace) {
    return "열기 후";
  }
  if (mode === "api-flow") {
    if (!workspaceControls.codeInventory) return "코드 필요";
    if (workspaceControls.codeInventory.routes.length === 0) return "라우트 필요";
    if (mapHasNoRelations) return "API 목록";
    return "코드 근거";
  }
  if (mode === "table-usage") {
    if (!dbProfileControls.inventory?.tables.length) return "DB 필요";
    if (!firstColumn(dbProfileControls)) return "테이블 목록";
    if (mapHasNoRelations) return "DB 구조";
    return !hasCodeContext ? "DB 구조" : "코드+DB";
  }
  if (mode === "column-impact") {
    if (!firstColumn(dbProfileControls)) return dbProfileControls.inventory?.tables.length ? "컬럼 대기" : "DB 필요";
    if (mapHasNoRelations) return "컬럼 목록";
    return !hasCodeContext ? "컬럼 제약" : "변경 근거";
  }
  if (mode === "search-focus") return dbProfileControls.inventory?.tables.length && !hasCodeContext ? "DB 대상" : "대상";
  return "전체 구조";
}

function modeAnswerLabel(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  const hasCodeContext = workbenchHasCodeContext(workspaceControls);
  if (mode === "api-flow") {
    const hasRoutes = Boolean(workspaceControls.codeInventory?.routes.length);
    return hasRoutes && !mapHasNoRelations ? "API가 어디까지 닿나?" : "API 진입점은?";
  }
  if (mode === "table-usage") {
    if (!dbProfileControls.inventory?.tables.length) return "이 테이블은 어디에 쓰이나?";
    return !hasCodeContext || mapHasNoRelations ? "테이블 구조는?" : "이 테이블은 어디에 쓰이나?";
  }
  if (mode === "column-impact") {
    if (!firstColumn(dbProfileControls)) return "이 컬럼을 바꾸면?";
    return !hasCodeContext || mapHasNoRelations ? "컬럼 제약은?" : "이 컬럼을 바꾸면?";
  }
  if (mode === "search-focus") {
    return "선택 항목 주변은?";
  }
  return "전체 구조는?";
}

function workbenchHasCodeContext(workspaceControls: WorkspaceControls): boolean {
  return Boolean(workspaceControls.codeInventory?.routes.length) || codeInventoryCodeItems(workspaceControls.codeInventory).length > 0;
}

function workbenchModeBlockReason(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
): string | null {
  if (!workspaceControls.currentWorkspace) {
    return workspaceControls.canCreateWorkspace
      ? workspaceControls.repoSourceMode === "github"
        ? "저장소 복제 준비"
        : "프로젝트 열기 준비"
      : workspaceControls.repoSourceMode === "github"
        ? "GitHub URL 필요"
        : "로컬 폴더 필요";
  }
  if (mode === "atlas") {
    return null;
  }
  if (mode === "api-flow") {
    const codeInventory = workspaceControls.codeInventory;
    if (!codeInventory) {
      return "코드 목록을 불러오면 열립니다.";
    }
    if (codeInventory.routes.length === 0) {
      return "API 라우트가 없습니다.";
    }
  }
  if (mode === "table-usage" && !dbProfileControls.inventory?.tables.length) {
    return "DB를 연결하면 열립니다.";
  }
  if (mode === "column-impact" && !firstColumn(dbProfileControls)) {
    return "컬럼을 읽으면 열립니다.";
  }
  if (mode === "search-focus" && !hasSearchableItems(workspaceControls, dbProfileControls)) {
    return workspaceControls.codeInventory || dbProfileControls.inventory
      ? "고를 코드, 파일, 테이블이 없습니다."
      : "코드 또는 DB 목록을 불러오면 고를 수 있습니다.";
  }
  return null;
}

function hasSearchableItems(workspaceControls: WorkspaceControls, dbProfileControls: DbProfileControls): boolean {
  return codeInventoryItemCount(workspaceControls.codeInventory) > 0 || Boolean(dbProfileControls.inventory?.tables.length);
}

function workbenchModeUnlockHint(
  mode: string,
  reason: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): ModeUnlockHint {
  if (mode === "api-flow" && workspaceControls.codeInventory?.routes.length === 0) {
    const item = firstCodeSearchItem(workspaceControls);
    if (item) return { reason, button: "코드 주변 보기", run: () => visualMapControls.showMode("search-focus", `code:${item.id}`), primary: true };
    if (workspaceControls.canIndexCode) return { reason, button: "다시 읽기", run: workspaceControls.indexCodeRepository };
    return { reason: workspaceControls.codeIndexBlockedReason ?? reason };
  }
  if (mode === "api-flow" && !workspaceControls.codeInventory) {
    const indexed = workspaceControls.codeStatus?.includes("완료") ?? false;
    if (indexed) return { reason, button: "코드 목록 열기", run: workspaceControls.loadCodeInventory, primary: true };
    if (workspaceControls.canIndexCode) return { reason, button: "코드 읽기", run: workspaceControls.indexCodeRepository, primary: true };
    return { reason: workspaceControls.codeIndexBlockedReason ?? reason };
  }
  if (mode === "table-usage" || mode === "column-impact") {
    return workbenchDbUnlockHint(reason, dbProfileControls);
  }
  return { reason };
}

function firstCodeSearchItem(workspaceControls: WorkspaceControls) {
  return codeInventoryCodeItems(workspaceControls.codeInventory)[0] ?? workspaceControls.codeInventory?.files[0] ?? null;
}

function workbenchDbUnlockHint(reason: string, dbProfileControls: DbProfileControls): ModeUnlockHint {
  if (!dbProfileControls.activeProfile) {
    return dbProfileControls.canSaveProfile
      ? { reason, button: "DB 연결 저장", run: dbProfileControls.saveProfile, primary: true }
      : { reason, button: "DB 정보 입력", run: () => focusDbProfileSetup(dbProfileControls) };
  }
  const indexed = dbProfileControls.status?.includes("완료") ?? false;
  if (indexed && !dbProfileControls.inventory) {
    return { reason, button: "DB 목록 열기", run: dbProfileControls.loadInventory, primary: true };
  }
  if (dbProfileControls.canIndexProfile) {
    return { reason, button: "DB 읽기", run: dbProfileControls.indexProfile, primary: true };
  }
  if (dbProfileControls.dbIndexBlockedReason) {
    return { reason: dbProfileControls.dbIndexBlockedReason };
  }
  return { reason, button: "DB 정보 입력", run: () => focusDbProfileSetup(dbProfileControls) };
}

function showWorkbenchMode(
  mode: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
) {
  if (mode === "api-flow") {
    const item = selectedRouteOrFirst(workspaceControls);
    if (item) {
      visualMapControls.showMode(mode, `code:${item.id}`);
      return;
    }
  }

  if (mode === "table-usage") {
    const tableKey = dbProfileControls.selectedTableKey ?? firstTableKey(dbProfileControls);
    if (tableKey) {
      visualMapControls.showMode(mode, `db:table:${tableKey}`);
      return;
    }
  }

  if (mode === "column-impact") {
    const focusId = firstColumnFocusId(dbProfileControls, visualMapControls);
    if (focusId) {
      visualMapControls.showMode(mode, focusId);
      return;
    }
  }

  if (mode === "search-focus") {
    if (visualMapControls.selectedNode) {
      visualMapControls.showMode(mode, visualMapControls.selectedNode.id);
      return;
    }
    if (workspaceControls.selectedCodeItem) {
      visualMapControls.showMode(mode, `code:${workspaceControls.selectedCodeItem.id}`);
      return;
    }
    if (dbProfileControls.selectedTableKey) {
      visualMapControls.showMode(mode, `db:table:${dbProfileControls.selectedTableKey}`);
      return;
    }
    const item =
      workspaceControls.codeInventory?.routes[0] ??
      codeInventoryCodeItems(workspaceControls.codeInventory)[0] ??
      workspaceControls.codeInventory?.files[0] ??
      null;
    if (item) {
      visualMapControls.showMode(mode, `code:${item.id}`);
      return;
    }
    const tableKey = dbProfileControls.selectedTableKey ?? firstTableKey(dbProfileControls);
    if (tableKey) {
      visualMapControls.showMode(mode, `db:table:${tableKey}`);
      return;
    }
  }

  visualMapControls.showMode(mode);
}

function selectedRouteOrFirst(workspaceControls: WorkspaceControls) {
  return codeInventoryDefaultRoute(workspaceControls.codeInventory, workspaceControls.selectedCodeItem?.id);
}

function firstTableKey(dbProfileControls: DbProfileControls): string | null {
  const table = dbProfileControls.inventory?.tables[0];
  return table ? dbInventoryTableKey(table) : null;
}

function firstColumn(dbProfileControls: DbProfileControls): { tableKey: string; columnName: string } | null {
  const selectedKey = dbProfileControls.selectedTableKey;
  const tables = dbProfileControls.inventory?.tables ?? [];
  const selectedTable =
    (selectedKey && tables.find((item) => dbInventoryTableKey(item) === selectedKey)) || null;
  const table = selectedTable?.columns.length
    ? selectedTable
    : tables.find((item) => item.columns.length > 0) ?? null;
  const column = table?.columns[0];
  return table && column ? { tableKey: dbInventoryTableKey(table), columnName: column.name } : null;
}

function firstColumnFocusId(dbProfileControls: DbProfileControls, visualMapControls: VisualMapControls): string | null {
  if (visualMapControls.selectedNode?.kind === "column") {
    return visualMapControls.selectedNode.id;
  }
  const tables = dbProfileControls.inventory?.tables ?? [];
  const selectedTable =
    (dbProfileControls.selectedTableKey &&
      tables.find((table) => dbInventoryTableKey(table) === dbProfileControls.selectedTableKey)) ||
    null;
  return (
    columnFocusId(selectedTable, (column) => column.isForeignKey) ??
    columnFocusId(selectedTable) ??
    columnFocusId(tables.find((table) => table.columns.some((column) => column.isForeignKey)) ?? null, (column) => column.isForeignKey) ??
    columnFocusId(tables.find((table) => table.columns.length > 0) ?? null)
  );
}

function columnFocusId(
  table: NonNullable<DbProfileControls["inventory"]>["tables"][number] | null,
  pick: (column: NonNullable<DbProfileControls["inventory"]>["tables"][number]["columns"][number]) => boolean = () => true,
): string | null {
  const column = table?.columns.find(pick) ?? null;
  return table && column ? `db:column:${dbInventoryTableKey(table)}:${column.name}` : null;
}
