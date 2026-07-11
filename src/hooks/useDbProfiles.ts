import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { toUserError } from "../app/operationStatus";
import { hasTauriRuntime, tauriUnavailableMessage } from "../app/tauriRuntime";
import {
  dbInventoryTableKey,
  dbProfileSourceUsesPath,
  type CodeInventory,
  type DbInventory,
  type DbProfile,
  type DbProfileSource,
  type IndexDbProfileRequest,
  type SaveDbProfileRequest,
  type Workspace,
} from "../types/workspace";

type WithBusy = (action: string, task: () => Promise<void>) => Promise<void>;

export function useDbProfiles({
  currentWorkspace,
  withBusy,
  setCurrentWorkspace,
  refreshWorkspaces,
  clearVisualMap,
  saveInventorySnapshot,
  getCodeInventory,
}: {
  currentWorkspace: Workspace | null;
  withBusy: WithBusy;
  setCurrentWorkspace: (workspace: Workspace | null) => void;
  refreshWorkspaces: (preferredWorkspaceId?: string) => Promise<void>;
  clearVisualMap: () => void;
  saveInventorySnapshot: (workspaceId: string, code: CodeInventory | null, db: DbInventory | null) => Promise<void>;
  getCodeInventory: () => CodeInventory | null;
}) {
  const activeProfile = getActiveDbProfile(currentWorkspace);
  const [dbProfileName, setDbProfileName] = useState("");
  const [dbProfileSource, setDbProfileSource] = useState<DbProfileSource>("ddl-sqlite");
  const [dbProfilePath, setDbProfilePath] = useState("");
  const [dbConnectionString, setDbConnectionString] = useState("");
  const [dbStatus, setDbStatus] = useState<string | null>(null);
  const [dbError, setDbError] = useState<string | null>(null);
  const [dbErrorDetail, setDbErrorDetail] = useState<string | null>(null);
  const [dbInventory, setDbInventory] = useState<DbInventory | null>(null);
  const [selectedDbTableKey, setSelectedDbTableKey] = useState<string | null>(null);

  useEffect(() => {
    hydrateDbProfile(activeProfile);
  }, [currentWorkspace?.id, activeProfile?.id]);

  useEffect(() => {
    setDbStatus(null);
    setDbError(null);
    setDbErrorDetail(null);
  }, [currentWorkspace?.id]);

  async function pickDbPath() {
    if (!dbProfileSourceUsesPath(dbProfileSource)) {
      return;
    }
    if (!hasTauriRuntime()) {
      setDbError(tauriUnavailableMessage);
      return;
    }

    const selected = await open({
      multiple: false,
      title: dbProfileSource === "ddl-sqlite" ? "DDL 파일 선택" : "SQLite 데이터베이스 선택",
      filters:
        dbProfileSource === "ddl-sqlite"
          ? [{ name: "SQL", extensions: ["sql"] }]
          : [{ name: "SQLite", extensions: ["sqlite", "sqlite3", "db"] }],
    });

    if (selected) {
      updateDbProfilePath(selected);
    }
  }

  async function saveDbProfile() {
    if (!currentWorkspace) {
      setDbError("프로젝트를 연 뒤 DB 연결을 저장하세요");
      return;
    }
    if (!hasTauriRuntime()) {
      setDbError(tauriUnavailableMessage);
      return;
    }

    const request: SaveDbProfileRequest = {
      workspaceId: currentWorkspace.id,
      name: dbProfileName.trim(),
      source: dbProfileSource,
      path: dbProfileSourceUsesPath(dbProfileSource) ? dbProfilePath.trim() : null,
    };

    if (!request.name) {
      setDbStatus(null);
      setDbError("DB 연결 이름을 입력하세요");
      return;
    }
    if (dbProfileSourceUsesPath(request.source) && !request.path) {
      setDbStatus(null);
      setDbError("DB 경로를 입력하세요");
      return;
    }

    await withBusy("db-save", async () => {
      try {
        const updated = await invoke<Workspace>("save_db_profile", { request });
        setCurrentWorkspace(updated);
        clearDbInventory();
        clearVisualMap();
        setDbStatus(`DB 연결 저장됨: ${request.name}`);
        setDbError(null);
        setDbErrorDetail(null);
        await refreshWorkspaces(updated.id);
      } catch (error) {
        const uiError = toUserError(error, "DB 연결을 저장하지 못했습니다");
        setDbStatus(null);
        setDbError(uiError.message);
        setDbErrorDetail(uiError.details);
      }
    });
  }

  async function indexDbProfile() {
    await runDbProfileIndex("db-index", "DB 구조 읽기 완료", "DB 읽기 실패");
  }

  async function testDbConnection() {
    await runDbProfileIndex("db-test", "DB 구조 테스트 완료", "DB 구조 테스트 실패");
  }

  async function runDbProfileIndex(action: "db-index" | "db-test", success: string, failure: string) {
    const request = dbIndexRequest();
    if (!request) {
      return;
    }

    await withBusy(action, async () => {
      try {
        const result = await invoke<{
          workspace: Workspace;
          run: { ok: boolean; stderr: string; stdout?: string };
          indexJson?: unknown | null;
        }>("index_db_profile", { request });
        const dbMessage = redactUiSecret(result.run.stderr || result.run.stdout || failure, dbConnectionString);
        setCurrentWorkspace(result.workspace);
        if (result.run.ok) {
          clearDbInventory();
          const successMessage = [success, dbIndexSummary(result.indexJson)].filter(Boolean).join(": ");
          if (action === "db-index") {
            setDbStatus("테이블 목록 불러오는 중...");
            try {
              await loadDbInventoryForProfile(request.workspaceId, request.profileId, "읽음");
            } catch (error) {
              const uiError = toUserError(error, "테이블 목록을 불러오지 못했습니다");
              setDbStatus(successMessage);
              setDbError(uiError.message);
              setDbErrorDetail(uiError.details);
            }
          } else {
            setDbStatus(successMessage);
            setDbError(null);
            setDbErrorDetail(null);
          }
        } else {
          const uiError = toDbUserError(dbMessage, failure);
          setDbStatus(null);
          setDbError(uiError.message);
          setDbErrorDetail(uiError.details);
        }
        await refreshWorkspaces(result.workspace.id);
      } catch (error) {
        const uiError = toDbUserError(redactUiSecret(String(error), dbConnectionString), failure);
        setDbStatus(null);
        setDbError(uiError.message);
        setDbErrorDetail(uiError.details);
      }
    });
  }

  function dbIndexRequest(): IndexDbProfileRequest | null {
    if (!currentWorkspace || !activeProfile) {
      setDbError("DB 연결을 저장한 뒤 구조를 읽으세요");
      return null;
    }
    if (!activeProfileMatchesForm(activeProfile, dbProfileName, dbProfileSource, dbProfilePath)) {
      setDbStatus(null);
      setDbError("변경한 DB 연결을 저장한 뒤 구조를 읽으세요");
      return null;
    }

    const request: IndexDbProfileRequest = {
      workspaceId: currentWorkspace.id,
      profileId: activeProfile.id,
      connectionString: dbProfileSourceUsesPath(activeProfile.source) ? null : dbConnectionString.trim(),
    };

    if (!dbProfileSourceUsesPath(activeProfile.source) && !request.connectionString) {
      setDbStatus(null);
      setDbError("이번 읽기에 사용할 연결 문자열을 입력하세요");
      return null;
    }

    return request;
  }

  async function loadDbInventory() {
    if (!currentWorkspace || !activeProfile) {
      setDbError("DB 연결을 저장한 뒤 테이블 목록을 불러오세요");
      return;
    }

    const workspaceId = currentWorkspace.id;
    const profileId = activeProfile.id;

    await withBusy("db-load", async () => {
      try {
        await loadDbInventoryForProfile(workspaceId, profileId, "불러옴");
      } catch (error) {
        const uiError = toUserError(error, "테이블 목록을 불러오지 못했습니다");
        clearDbInventory();
        setDbStatus(null);
        setDbError(uiError.message);
        setDbErrorDetail(uiError.details);
      }
    });
  }

  function hydrateDbProfile(profile: DbProfile | null) {
    if (!profile) {
      setDbProfileName("");
      setDbProfileSource("ddl-sqlite");
      setDbProfilePath("");
      setDbConnectionString("");
      clearDbInventory();
      return;
    }

    setDbProfileName(profile.name);
    setDbProfileSource(profile.source);
    setDbProfilePath(profile.path ?? "");
    setDbConnectionString("");
    clearDbInventory();
  }

  function updateDbProfilePath(value: string) {
    const previousName = dbProfileNameForPath(dbProfilePath);
    setDbProfilePath(value);

    const source = dbProfileSourceForPath(value);
    if (source) {
      setDbProfileSource(source);
    }

    if (dbProfileName.trim() && dbProfileName.trim() !== previousName) {
      return;
    }

    const nextName = dbProfileNameForPath(value);
    if (nextName) {
      setDbProfileName(nextName);
    }
  }

  function clearDbInventory() {
    setDbInventory(null);
    setSelectedDbTableKey(null);
  }

  function restoreDbInventory(inventory: DbInventory, selectedTableKey: string | null) {
    setDbInventory(inventory);
    setSelectedDbTableKey(selectedTableKey);
    setDbStatus(dbInventoryStatus(inventory, "불러옴"));
    setDbError(null);
    setDbErrorDetail(null);
  }

  async function loadDbInventoryForProfile(workspaceId: string, profileId: string, action: string) {
    const inventory = await invoke<DbInventory>("get_db_inventory", {
      workspaceId,
      profileId,
    });
    setDbInventory(inventory);
    setSelectedDbTableKey(inventory.tables[0] ? dbInventoryTableKey(inventory.tables[0]) : null);
    setDbStatus(dbInventoryStatus(inventory, action));
    setDbError(null);
    setDbErrorDetail(null);
    void saveInventorySnapshot(workspaceId, getCodeInventory(), inventory);
  }

  return {
    activeProfile,
    dbProfileName,
    dbProfileSource,
    dbProfilePath,
    dbConnectionString,
    dbStatus,
    dbError,
    dbErrorDetail,
    dbInventory,
    selectedDbTableKey,
    setDbProfileName,
    setDbProfileSource,
    setDbProfilePath: updateDbProfilePath,
    setDbConnectionString,
    setSelectedDbTableKey,
    restoreDbInventory,
    pickDbPath,
    saveDbProfile,
    testDbConnection,
    indexDbProfile,
    loadDbInventory,
    clearDbInventory,
  };
}

function dbInventoryStatus(inventory: DbInventory, action: string): string {
  if (inventory.tables.length === 0) {
    return "테이블 목록이 비어 있음";
  }
  const columnCount = inventory.tables.reduce((sum, table) => sum + table.columns.length, 0);
  const missingColumnTables = inventory.tables.filter((table) => table.columns.length === 0).length;
  if (missingColumnTables > 0 && columnCount > 0) {
    return `테이블 ${inventory.tables.length}개, 컬럼 ${columnCount}개, ${missingColumnTables}개 테이블 컬럼 필요 ${action}`;
  }
  return `테이블 ${inventory.tables.length}개, 컬럼 ${columnCount}개 ${action}`;
}

function getActiveDbProfile(workspace: Workspace | null): DbProfile | null {
  return workspace?.dbProfiles.find((profile) => profile.id === workspace.activeDbProfileId) ?? workspace?.dbProfiles[0] ?? null;
}

function dbProfileSourceForPath(value: string): DbProfileSource | null {
  const lower = value.trim().toLowerCase();
  if (lower.endsWith(".sql")) {
    return "ddl-sqlite";
  }
  if (lower.endsWith(".sqlite") || lower.endsWith(".sqlite3") || lower.endsWith(".db")) {
    return "sqlite";
  }
  return null;
}

function dbProfileNameForPath(value: string): string | null {
  const fileName = value.split(/[\\/]+/).filter(Boolean).pop()?.trim();
  if (!fileName) {
    return null;
  }
  return fileName.replace(/\.(sql|sqlite3?|db)$/i, "") || fileName;
}

function activeProfileMatchesForm(
  profile: DbProfile,
  name: string,
  source: DbProfileSource,
  path: string,
): boolean {
  return (
    profile.name === name.trim() &&
    profile.source === source &&
    (!dbProfileSourceUsesPath(source) || (profile.path ?? "") === path.trim())
  );
}

function dbIndexSummary(indexJson: unknown): string | null {
  const tables = countFromJson(indexJson, ["tableCount", "tablesCount", "tables"]);
  const columns = countFromJson(indexJson, ["columnCount", "columnsCount", "columns"]);

  if (tables == null && columns == null) {
    return null;
  }

  return `테이블 ${tables ?? 0}개, 컬럼 ${columns ?? 0}개`;
}

function countFromJson(value: unknown, keys: string[]): number | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  for (const key of keys) {
    const item = (value as Record<string, unknown>)[key];
    if (typeof item === "number") {
      return item;
    }
    if (Array.isArray(item)) {
      return item.length;
    }
  }

  return null;
}

function toDbUserError(value: string, fallback: string): { message: string; details: string } {
  const details = value;
  const lower = value.toLowerCase();

  if (lower.includes("password") || lower.includes("access denied") || lower.includes("login failed") || lower.includes("ora-01017") || lower.includes("auth")) {
    return { message: `${fallback}: 인증 정보를 확인하세요`, details };
  }
  if (lower.includes("driver") || lower.includes("odbc") || lower.includes("provider") || lower.includes("dll") || lower.includes("module not found")) {
    return { message: `${fallback}: DB 드라이버를 확인하세요`, details };
  }
  if (lower.includes("parse") || lower.includes("invalid json") || lower.includes("metadata")) {
    return { message: `${fallback}: DB 구조 해석에 실패했습니다`, details };
  }
  if (lower.includes("connection refused") || lower.includes("could not connect") || lower.includes("timed out") || lower.includes("timeout") || lower.includes("network")) {
    return { message: `${fallback}: 연결 정보를 확인하세요`, details };
  }

  return toUserError(details, fallback);
}

function redactUiSecret(value: string, secret: string): string {
  const trimmedSecret = secret.trim();
  return trimmedSecret ? value.split(trimmedSecret).join("[REDACTED]") : value;
}
