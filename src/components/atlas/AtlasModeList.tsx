import { useEffect, useRef, useState } from "react";
import { codeInventoryCodeItems, codeInventoryDefaultRoute, codeInventoryItemCount, dbInventoryTableKey } from "../../types/workspace";
import { dbProfileWorkStarted } from "../../types/controls";
import type { DbProfileControls, VisualMapControls, WorkspaceControls } from "../../types/controls";
import { focusDbProfileSetup } from "../common/focusSourceSetup";
import { focusGlobalSearch } from "../common/focusGlobalSearch";
import type { View } from "../common/ViewSwitch";
import { atlasModes } from "./atlasModes";

type AtlasModeId = (typeof atlasModes)[number]["id"];
type AtlasSelectableMode = AtlasModeId | "search";

export function AtlasModeList({
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
  const noteRef = useRef<HTMLDivElement>(null);
  const [note, setNote] = useState<AtlasUnlockHint | null>(null);
  const readiness = atlasReadiness(workspaceControls, dbProfileControls, visualMapControls);
  const readinessAction = atlasReadinessAction(setView, workspaceControls, dbProfileControls, visualMapControls);
  const displayedNote = note;
  const mapHasNoRelations = Boolean(visualMapControls.currentMap && visualMapControls.currentMap.edges.length === 0);
  const showCardLockReasons = Boolean(workspaceControls.currentWorkspace);
  const showModeCards = Boolean(workspaceControls.currentWorkspace);

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
    <div className="mode-list">
      <div className={`mode-readiness ${readiness.tone}`}>
        <span>{readiness.badge}</span>
        <strong>{readiness.title}</strong>
        <small>{readiness.text}</small>
        {readinessAction?.run && (
          <button
            className={`mode-readiness-action ${
              readinessAction.primary ? "primary-action compact" : "outline-action compact"
            }`}
            type="button"
            onClick={() => {
              readinessAction.run?.();
              setNote(null);
            }}
            disabled={readinessAction.disabled || workspaceControls.busy || dbProfileControls.busy}
          >
            {readinessAction.button}
          </button>
        )}
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
      {showModeCards && atlasModes.map((mode) => {
        const ModeIcon = mode.icon;
        const blockReason = atlasModeBlockReason(mode.id, workspaceControls, dbProfileControls);
        const active = !blockReason && isActiveMode(mode.id, visualMapControls.mode);
        const modeTitle = atlasModeTitle(mode.id, mode.title, workspaceControls, dbProfileControls, mapHasNoRelations);
        const modeText = atlasModeText(mode.id, workspaceControls, dbProfileControls, mapHasNoRelations);
        const modeAnswer = atlasModeAnswerLabel(mode.id, workspaceControls, dbProfileControls, mapHasNoRelations);
        const unlockHint = blockReason
          ? atlasUnlockHint(mode.id, blockReason, setView, workspaceControls, dbProfileControls, visualMapControls)
          : null;
        const disabledUntilProject = Boolean(blockReason && !workspaceControls.currentWorkspace);
        return (
          <button
            className={`mode-card boxed ${active ? "active" : ""} ${blockReason ? "locked" : ""}`}
            key={mode.id}
            type="button"
            data-mode-id={mode.id}
            onClick={() => {
              if (blockReason) {
                setNote(unlockHint);
                return;
              }
              const nextNote = selectAtlasMode(mode.id, workspaceControls, dbProfileControls, visualMapControls);
              setNote(nextNote ? { reason: nextNote } : null);
            }}
            aria-label={`찾을 답: ${modeAnswer}: ${blockReason ?? modeText}`}
            aria-pressed={active}
            title={blockReason ?? undefined}
            disabled={disabledUntilProject || workspaceControls.busy || dbProfileControls.busy}
          >
            <span className="mode-icon">
              <ModeIcon size={16} />
            </span>
            <span className="mode-text">
              <strong title={modeAnswer}>{modeAnswer}</strong>
              <small title={modeTitle}>{modeTitle}</small>
              <span className="mode-answer">
                {modeText}
              </span>
              <em className="mode-source">
                {atlasModeDataLabel(mode.id, workspaceControls, dbProfileControls, mapHasNoRelations)}
              </em>
              {blockReason && showCardLockReasons && <small className="mode-lock-reason">{blockReason}</small>}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function atlasReadiness(
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
  const hasFileItems = Boolean(workspaceControls.codeInventory?.files.length);
  const hasSearchableCode = hasCodeItems || hasFileItems || hasRoutes;
  const hasCodeContext = hasRoutes || hasCodeItems;
  const hasCodeInventory = Boolean(workspaceControls.codeInventory);
  const tables = dbProfileControls.inventory?.tables ?? [];
  const hasTables = Boolean(tables.length);
  const hasColumns = firstColumn(dbProfileControls);
  const missingColumnTables = tables.filter((table) => table.columns.length === 0).length;
  const openModes = 1 + Number(hasRoutes) + Number(hasTables) + Number(hasColumns);
  const needsGithub = workspaceControls.repoSourceMode === "github";
  const mapHasNoRelations = Boolean(
    visualMapControls.currentMap &&
      visualMapControls.currentMap.nodes.length === 0 &&
      visualMapControls.currentMap.edges.length === 0,
  );

  if (!hasWorkspace) {
    if (workspaceControls.canCreateWorkspace) {
      return {
        badge: "0/4",
        title: needsGithub ? "저장소 복제 준비" : "프로젝트 열기 준비",
        text: needsGithub ? "복제하면 나머지 답을 고를 수 있습니다." : "열면 나머지 답을 고를 수 있습니다.",
        tone: "pending",
      };
    }
    return {
      badge: "0/4",
      title: needsGithub ? "GitHub URL 필요" : "로컬 폴더 필요",
      text: needsGithub ? "URL을 붙여넣으면 복제해서 열 수 있습니다." : "폴더를 지정하면 답을 고를 수 있습니다.",
      tone: "pending",
    };
  }
  if (mapHasNoRelations && (hasSearchableCode || hasTables) && !(hasTables && !hasColumns) && missingColumnTables === 0) {
    return {
      badge: `${openModes}/4`,
      title: "코드/DB 목록만 있음",
      text: "관계는 아직 없고 실제 카드부터 확인합니다.",
      tone: "partial",
    };
  }
  if (hasColumns && missingColumnTables > 0) {
    return {
      badge: `${openModes}/4`,
      title: "컬럼 보강 필요",
      text: `테이블 ${missingColumnTables}개는 컬럼 보강이 필요합니다.`,
      tone: "partial",
    };
  }
  if (openModes === atlasModes.length) {
    return {
      badge: "4/4",
      title: "답 찾기 가능",
      text: "대상을 좁히고 API·테이블·컬럼 근거 확인",
      tone: "ready",
    };
  }
  if (hasRoutes && !hasTables) {
    return {
      badge: `${openModes}/4`,
      title: "API 라우트 확인",
      text: "DB 구조를 불러오면 변경 범위까지 볼 수 있습니다.",
      tone: "partial",
    };
  }
  if (hasTables) {
    return {
      badge: `${openModes}/4`,
      title: hasColumns ? (hasCodeContext ? "키/후보 확인" : "컬럼 제약 가능") : "컬럼 구조 필요",
      text: hasColumns
        ? hasRoutes
          ? "API 경로와 DB 키/후보 확인"
          : hasCodeContext
            ? "API 라우트가 없어 DB 제약과 코드 후보를 봅니다."
            : hasCodeInventory
              ? "코드 항목이 없어 DB 구조와 컬럼 제약을 봅니다."
              : "코드 없이도 DB 구조와 컬럼 제약을 볼 수 있습니다."
        : "컬럼 구조를 불러오면 변경 범위가 열립니다.",
      tone: "partial",
    };
  }
  if (hasSearchableCode) {
    return {
      badge: `${openModes}/4`,
      title: hasCodeItems ? "코드 주변 근거" : "파일 주변 근거",
      text: hasCodeItems ? "코드 구조와 검색을 바로 사용할 수 있습니다." : "파일을 기준으로 바로 좁혀 볼 수 있습니다.",
      tone: "partial",
    };
  }
  return {
    badge: "0/4",
    title: "코드/DB 필요",
    text: "코드 또는 DB 목록을 불러오면 답을 고를 수 있습니다.",
    tone: "pending",
  };
}

type AtlasUnlockHint = {
  reason: string;
  button?: string;
  run?: () => void;
  primary?: boolean;
  disabled?: boolean;
};

function atlasReadinessAction(
  setView: (view: View) => void,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): AtlasUnlockHint | null {
  if (!workspaceControls.currentWorkspace) {
    return null;
  }
  const dbStarted = dbProfileWorkStarted(dbProfileControls);
  if (dbStarted && !dbProfileControls.inventory) {
    return atlasUnlockHint("dependencies", "DB를 연결하면 테이블 답이 열립니다.", setView, workspaceControls, dbProfileControls);
  }
  if (!workspaceControls.codeInventory) {
    return workspaceControls.canIndexCode
      ? atlasUnlockHint("api", "코드 목록을 불러오면 열립니다.", setView, workspaceControls, dbProfileControls)
      : atlasUnlockHint("dependencies", "코드 읽기가 막혀 있으면 DB 구조부터 볼 수 있습니다.", setView, workspaceControls, dbProfileControls);
  }
  if (dbProfileControls.inventory?.tables.length && !firstColumn(dbProfileControls)) {
    return {
      reason: "컬럼을 읽으면 변경 범위가 열립니다.",
      button:
        !dbProfileControls.activeProfile && dbProfileControls.canSaveProfile
          ? "DB 연결 저장"
          : dbProfileControls.canIndexProfile
            ? "다시 읽기"
            : "DB 정보 입력",
      run:
        !dbProfileControls.activeProfile && dbProfileControls.canSaveProfile
          ? dbProfileControls.saveProfile
          : dbProfileControls.canIndexProfile
            ? dbProfileControls.indexProfile
            : () => showWorkbenchDbSetup(setView, dbProfileControls),
      primary: true,
    };
  }
  if (atlasHasCodeContext(workspaceControls) && dbProfileControls.inventory?.tables.length) {
    return {
      reason: "API, 코드, 테이블 중 답을 찾을 대상을 좁혀 보세요.",
      button: "검색으로 찾기",
      run: () => focusGlobalSearch(visualMapControls),
      primary: true,
    };
  }
  if (firstColumn(dbProfileControls)) {
    const hasCodeContext =
      Boolean(workspaceControls.codeInventory?.routes.length) || codeInventoryCodeItems(workspaceControls.codeInventory).length > 0;
    return {
      reason: hasCodeContext ? "직접 근거와 후보 근거를 볼 수 있습니다." : "컬럼 제약을 바로 볼 수 있습니다.",
      button: hasCodeContext ? "변경 범위" : "컬럼 제약",
      run: () => selectAtlasMode("impact", workspaceControls, dbProfileControls, visualMapControls),
      primary: true,
    };
  }
  if (dbProfileControls.inventory?.tables.length) {
    return {
      reason: "테이블 연결을 바로 볼 수 있습니다.",
      button: "테이블 연결",
      run: () => selectAtlasMode("dependencies", workspaceControls, dbProfileControls, visualMapControls),
      primary: true,
    };
  }
  if (workspaceControls.codeInventory?.routes.length) {
    return {
      reason: "API 라우트가 닿는 코드를 볼 수 있습니다.",
      button: "API가 닿는 코드",
      run: () => selectAtlasMode("api", workspaceControls, dbProfileControls, visualMapControls),
      primary: true,
    };
  }
  if (codeInventoryItemCount(workspaceControls.codeInventory) > 0) {
    const hasCodeSymbols = codeInventoryCodeItems(workspaceControls.codeInventory).length > 0;
    return {
      reason: hasCodeSymbols ? "코드나 파일 기준으로 바로 좁혀 볼 수 있습니다." : "파일 기준으로 바로 좁혀 볼 수 있습니다.",
      button: hasCodeSymbols ? "코드 주변 근거" : "파일 주변 근거",
      run: () => {
        selectAtlasMode("search", workspaceControls, dbProfileControls, visualMapControls);
        focusGlobalSearch(visualMapControls);
      },
      primary: true,
    };
  }
  return atlasUnlockHint("dependencies", "DB를 연결하면 테이블 답이 열립니다.", setView, workspaceControls, dbProfileControls);
}

function atlasModeTitle(
  id: AtlasModeId,
  fallback: string,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  const hasCodeContext = atlasHasCodeContext(workspaceControls);
  const codeInventory = workspaceControls.codeInventory;
  const tables = dbProfileControls.inventory?.tables.length ?? 0;
  if (id === "api") {
    if (!codeInventory) {
      return "API 진입점 보기";
    }
    if (codeInventory.routes.length === 0) {
      return "API 라우트 없음";
    }
    if (mapHasNoRelations) {
      return "API 진입점 보기";
    }
  }
  if (id === "dependencies" && tables > 0 && !firstColumn(dbProfileControls)) {
    return "테이블 목록 확인";
  }
  if (id === "dependencies" && tables > 0 && mapHasNoRelations) {
    return "테이블 구조 보기";
  }
  if (id === "dependencies" && tables > 0 && !hasCodeContext) {
    return "테이블 구조 보기";
  }
  if (id === "dependencies" && tables === 0) {
    return "테이블 사용처 보기";
  }
  if (id === "impact" && firstColumn(dbProfileControls) && !hasCodeContext) {
    return "컬럼 제약 보기";
  }
  if (id === "impact" && firstColumn(dbProfileControls) && mapHasNoRelations) {
    return "컬럼 속성 보기";
  }
  if (id === "impact" && !firstColumn(dbProfileControls)) {
    return tables > 0 ? "컬럼 영향 보기" : "컬럼 변경 범위";
  }
  return fallback;
}

function atlasModeText(
  id: AtlasModeId,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  const routes = workspaceControls.codeInventory?.routes.length ?? 0;
  const code = codeInventoryCodeItems(workspaceControls.codeInventory).length;
  const files = workspaceControls.codeInventory?.files.length ?? 0;
  const tables = dbProfileControls.inventory?.tables.length ?? 0;
  const hasCodeContext = routes > 0 || code > 0;

  if (id === "atlas") {
    return atlasInventoryFlow(routes, code, files, tables);
  }
  if (id === "api") {
    if (!workspaceControls.codeInventory) {
      return "코드 읽은 뒤 확인";
    }
    if (routes === 0) {
      return "라우트가 잡히면 선택";
    }
    return mapHasNoRelations ? "라우트 구조 확인" : "API가 닿는 코드";
  }
  if (id === "dependencies") {
    if (tables === 0) {
      return "DB 연결 후 사용처 확인";
    }
    if (!firstColumn(dbProfileControls)) {
      return "컬럼 불러오면 관계 확인";
    }
    if (mapHasNoRelations) {
      return "테이블/컬럼 구조 확인";
    }
    return hasCodeContext ? "코드 후보와 제약" : "DB 제약과 테이블 연결";
  }
  if (firstColumn(dbProfileControls) && mapHasNoRelations) {
    return "컬럼 타입과 키 확인";
  }
  return firstColumn(dbProfileControls)
    ? (hasCodeContext ? "깨질 수 있는 범위" : "DB 컬럼 제약")
    : tables > 0 ? "컬럼 읽으면 선택" : "DB 연결 후 범위 확인";
}

function atlasModeDataLabel(
  id: AtlasModeId,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  const hasCodeContext = atlasHasCodeContext(workspaceControls);
  const tables = dbProfileControls.inventory?.tables.length ?? 0;
  if (id === "api") {
    if (!workspaceControls.codeInventory) return "코드 필요";
    if (workspaceControls.codeInventory.routes.length === 0) return "라우트 필요";
    return mapHasNoRelations ? "API 목록" : "코드 근거";
  }
  if (id === "dependencies") {
    if (tables > 0 && !firstColumn(dbProfileControls)) return "테이블 목록";
    if (tables > 0 && mapHasNoRelations) return "DB 구조";
    if (tables > 0 && !hasCodeContext) return "DB 구조";
    return tables === 0 ? "DB 필요" : "코드+DB";
  }
  if (id === "impact") {
    if (firstColumn(dbProfileControls) && mapHasNoRelations) return "컬럼 목록";
    if (firstColumn(dbProfileControls) && !hasCodeContext) return "컬럼 제약";
    return tables === 0 ? "DB 필요" : firstColumn(dbProfileControls) ? "변경 근거" : "컬럼 대기";
  }
  return "전체 구조";
}

function atlasModeAnswerLabel(
  id: AtlasModeId,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  mapHasNoRelations: boolean,
): string {
  const hasCodeContext = atlasHasCodeContext(workspaceControls);
  if (id === "api") {
    const hasRoutes = Boolean(workspaceControls.codeInventory?.routes.length);
    return hasRoutes && !mapHasNoRelations ? "API가 어디까지 닿나?" : "API 진입점은?";
  }
  if (id === "dependencies") {
    if (!dbProfileControls.inventory?.tables.length) return "이 테이블은 어디에 쓰이나?";
    return !hasCodeContext || mapHasNoRelations ? "테이블 구조는?" : "이 테이블은 어디에 쓰이나?";
  }
  if (id === "impact") {
    if (!firstColumn(dbProfileControls)) return "이 컬럼을 바꾸면?";
    return !hasCodeContext || mapHasNoRelations ? "컬럼 제약은?" : "이 컬럼을 바꾸면?";
  }
  return "전체 구조는?";
}

function atlasHasCodeContext(workspaceControls: WorkspaceControls): boolean {
  return Boolean(workspaceControls.codeInventory?.routes.length) || codeInventoryCodeItems(workspaceControls.codeInventory).length > 0;
}

function atlasInventoryFlow(routes: number, code: number, files: number, tables: number): string {
  const parts = [
    routes > 0 ? "API" : null,
    code > 0 ? "코드" : null,
    files > 0 ? "파일" : null,
    tables > 0 ? "DB" : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" → ") : "코드/DB 필요";
}

function atlasModeBlockReason(
  id: AtlasModeId,
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
  if (id === "atlas") {
    return null;
  }
  if (id === "api") {
    const codeInventory = workspaceControls.codeInventory;
    if (!codeInventory) {
      return "코드 목록을 불러오면 열립니다.";
    }
    if (codeInventory.routes.length === 0) {
      return "API 라우트가 없습니다.";
    }
  }
  if (id === "dependencies" && !dbProfileControls.inventory?.tables.length) {
    return "DB를 연결하면 열립니다.";
  }
  if (id === "impact" && !firstColumn(dbProfileControls)) {
    return "컬럼을 읽으면 열립니다.";
  }
  return null;
}

function atlasUnlockHint(
  id: AtlasModeId,
  reason: string,
  setView: (view: View) => void,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls?: VisualMapControls,
): AtlasUnlockHint {
  if (!workspaceControls.currentWorkspace) {
    if (workspaceControls.canCreateWorkspace) {
      return { reason, button: workspaceControls.repoSourceMode === "github" ? "복제하고 열기" : "프로젝트 열기", run: workspaceControls.createWorkspace, primary: true };
    }
    if (workspaceControls.repoSourceMode === "local") {
      return { reason, button: "로컬 폴더 선택", run: workspaceControls.pickRepoPath };
    }
    return { reason };
  }

  if (id === "api" && workspaceControls.codeInventory?.routes.length === 0) {
    if (visualMapControls && codeInventoryItemCount(workspaceControls.codeInventory) > 0) {
      return {
        reason,
        button: "코드 주변 보기",
        run: () => selectAtlasMode("search", workspaceControls, dbProfileControls, visualMapControls),
        primary: true,
      };
    }
    if (workspaceControls.canIndexCode) {
      return { reason, button: "다시 읽기", run: workspaceControls.indexCodeRepository };
    }
    return { reason };
  }

  if (id === "api") {
    return atlasCodeInventoryAction(reason, workspaceControls);
  }

  if (id === "dependencies" || id === "impact") {
    if (!dbProfileControls.activeProfile) {
      return dbProfileControls.canSaveProfile
        ? { reason, button: "DB 연결 저장", run: dbProfileControls.saveProfile, primary: true }
        : { reason, button: "DB 정보 입력", run: () => showWorkbenchDbSetup(setView, dbProfileControls) };
    }
    const indexed = dbProfileControls.status?.includes("완료") ?? false;
    return indexed
      ? { reason, button: "DB 목록 열기", run: dbProfileControls.loadInventory, primary: true }
      : dbProfileControls.dbIndexBlockedReason
        ? { reason: dbProfileControls.dbIndexBlockedReason }
      : {
          reason,
          button: "DB 읽기",
          run: dbProfileControls.indexProfile,
          primary: true,
          disabled: !dbProfileControls.canIndexProfile,
        };
  }

  return { reason };
}

function atlasCodeInventoryAction(reason: string, workspaceControls: WorkspaceControls): AtlasUnlockHint {
  const indexed = workspaceControls.codeStatus?.includes("완료") ?? false;
  if (indexed) {
    return { reason, button: "코드 목록 열기", run: workspaceControls.loadCodeInventory, primary: true };
  }
  if (!workspaceControls.canIndexCode) {
    return { reason: workspaceControls.codeIndexBlockedReason ?? reason };
  }
  return { reason, button: "코드 읽기", run: workspaceControls.indexCodeRepository, primary: true };
}

function firstColumn(dbProfileControls: DbProfileControls): boolean {
  return Boolean(dbProfileControls.inventory?.tables.some((table) => table.columns.length > 0));
}

function showWorkbenchDbSetup(setView: (view: View) => void, dbProfileControls: DbProfileControls) {
  setView("workbench");
  window.requestAnimationFrame(() => focusDbProfileSetup(dbProfileControls));
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

function isActiveMode(id: AtlasModeId, currentMode: string): boolean {
  return (
    (id === "atlas" && currentMode === "atlas") ||
    (id === "api" && currentMode === "api-flow") ||
    (id === "dependencies" && currentMode === "table-usage") ||
    (id === "impact" && currentMode === "column-impact")
  );
}

function selectAtlasMode(
  id: AtlasSelectableMode,
  workspaceControls: WorkspaceControls,
  dbProfileControls: DbProfileControls,
  visualMapControls: VisualMapControls,
): string | null {
  if (id === "atlas") {
    visualMapControls.showMode("atlas");
    return null;
  }

  const routes = workspaceControls.codeInventory?.routes ?? [];
  const tables = dbProfileControls.inventory?.tables ?? [];
  const selectedTableKey =
    (dbProfileControls.selectedTableKey &&
      tables.some((table) => dbInventoryTableKey(table) === dbProfileControls.selectedTableKey) &&
      dbProfileControls.selectedTableKey) ||
    null;
  const tableKey =
    selectedTableKey || (tables[0] ? dbInventoryTableKey(tables[0]) : null);

  if (id === "search") {
    if (visualMapControls.selectedNode) {
      visualMapControls.showMode("search-focus", visualMapControls.selectedNode.id);
      return null;
    }
    if (workspaceControls.selectedCodeItem) {
      visualMapControls.showMode("search-focus", `code:${workspaceControls.selectedCodeItem.id}`);
      return null;
    }
    if (selectedTableKey) {
      visualMapControls.showMode("search-focus", `db:table:${selectedTableKey}`);
      return null;
    }
    const item =
      routes[0] ??
      codeInventoryCodeItems(workspaceControls.codeInventory)[0] ??
      workspaceControls.codeInventory?.files[0] ??
      null;
    if (item) {
      visualMapControls.showMode("search-focus", `code:${item.id}`);
      return null;
    }
    if (tableKey) {
      visualMapControls.showMode("search-focus", `db:table:${tableKey}`);
      return null;
    }
    visualMapControls.showMode("search-focus");
    return null;
  }

  if (id === "api") {
    const firstRoute = codeInventoryDefaultRoute(
      workspaceControls.codeInventory,
      workspaceControls.selectedCodeItem?.id,
    );
    if (!firstRoute) {
      return "API 목록이 없습니다. 코드/DB 연결에서 코드 목록을 불러오세요.";
    }
    visualMapControls.showMode("api-flow", `code:${firstRoute.id}`);
    return null;
  }

  if (id === "impact") {
    const focusId = firstColumnFocusId(dbProfileControls, visualMapControls);
    if (!focusId) {
      return "컬럼 목록이 없습니다. 코드/DB 연결에서 DB 구조를 불러오세요.";
    }
    visualMapControls.showMode("column-impact", focusId);
    return null;
  }

  if (!tableKey) {
    return "테이블 목록이 없습니다. 코드/DB 연결에서 DB 구조를 불러오세요.";
  }
  visualMapControls.showMode("table-usage", `db:table:${tableKey}`);
  return null;
}
