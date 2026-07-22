import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useLayoutEffect, useState } from "react";
import { toUserError } from "../app/operationStatus";
import { hasTauriRuntime, tauriUnavailableMessage } from "../app/tauriRuntime";
import {
  dbInventoryTableKey,
  dbInventoryTableCount,
  dbProfileSourceUsesPath,
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
  refreshInventorySnapshot,
}: {
  currentWorkspace: Workspace | null;
  withBusy: WithBusy;
  setCurrentWorkspace: (workspace: Workspace | null) => void;
  refreshWorkspaces: (preferredWorkspaceId?: string) => Promise<void>;
  clearVisualMap: () => void;
  refreshInventorySnapshot: (workspaceId: string) => Promise<void>;
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
  const [inventoryWorkspaceId, setInventoryWorkspaceId] = useState<string | null>(null);
  const [selectedDbTableKey, setSelectedDbTableKey] = useState<string | null>(null);

  useLayoutEffect(() => {
    hydrateDbProfile(activeProfile);
  }, [currentWorkspace?.id, activeProfile?.id]);

  useLayoutEffect(() => {
    setDbStatus(null);
    setDbError(null);
    setDbErrorDetail(null);
  }, [currentWorkspace?.id]);

  async function pickDbPath(directory = false) {
    if (!dbProfileSourceUsesPath(dbProfileSource)) {
      return;
    }
    if (!hasTauriRuntime()) {
      setDbError(tauriUnavailableMessage);
      return;
    }

    let selected: string | string[] | null;
    try {
      selected = await open({
        directory,
        multiple: false,
        title: directory
          ? "DDL 폴더 선택"
          : dbProfileSource === "ddl-sqlite"
            ? "DDL 파일 선택"
            : "SQLite 데이터베이스 선택",
        filters:
          directory
            ? undefined
            : dbProfileSource === "ddl-sqlite"
            ? [{ name: "SQL", extensions: ["sql"] }]
            : [{ name: "SQLite", extensions: ["sqlite", "sqlite3", "db"] }],
      });
    } catch (error) {
      const uiError = toUserError(error, "DB 파일 선택기를 열지 못했습니다");
      setDbError(uiError.message);
      setDbErrorDetail(uiError.details);
      return;
    }

    if (selected && !Array.isArray(selected)) {
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
    const request = dbIndexRequest();
    if (!request) {
      return;
    }

    await withBusy("db-index", async () => {
      try {
        const result = await invoke<{
          workspace: Workspace;
          run: { ok: boolean; stderr: string; stdout?: string };
          indexJson?: unknown | null;
          inventory?: DbInventory | null;
          inventoryError?: string | null;
        }>("index_db_profile", { request });
        const dbMessage = redactUiSecret(result.run.stderr || result.run.stdout || "DB 읽기 실패", dbConnectionString);
        setCurrentWorkspace(result.workspace);
        if (result.run.ok) {
          clearDbInventory();
          const successMessage = ["DB 구조 읽기 완료", dbIndexSummary(result.indexJson)].filter(Boolean).join(": ");
          if (!result.inventory) {
            const uiError = toUserError(result.inventoryError ?? "DB inventory가 없습니다", "테이블 목록을 불러오지 못했습니다");
            setDbStatus(successMessage);
            setDbError(uiError.message);
            setDbErrorDetail(uiError.details);
          } else {
            setDbStatus("DB 읽기 결과 저장 중...");
            try {
              await storeDbInventory(request.workspaceId, result.inventory);
            } catch (error) {
              const uiError = toUserError(error, "DB 읽기 결과를 저장하지 못했습니다");
              setDbStatus(successMessage);
              setDbError(uiError.message);
              setDbErrorDetail(uiError.details);
            }
          }
        } else {
          const uiError = toDbUserError(dbMessage, "DB 읽기 실패");
          setDbStatus(null);
          setDbError(uiError.message);
          setDbErrorDetail(uiError.details);
        }
        await refreshWorkspaces(result.workspace.id);
      } catch (error) {
        const uiError = toDbUserError(redactUiSecret(String(error), dbConnectionString), "DB 읽기 실패");
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

  async function deleteDbProfile() {
    if (!currentWorkspace || !activeProfile || !hasTauriRuntime()) {
      return;
    }
    const deletedName = activeProfile.name;
    await withBusy("db-delete", async () => {
      try {
        const updated = await invoke<Workspace>("delete_db_profile", {
          workspaceId: currentWorkspace.id,
          profileId: activeProfile.id,
        });
        setCurrentWorkspace(updated);
        clearDbInventory();
        clearVisualMap();
        setDbConnectionString("");
        setDbStatus(`DB 연결 삭제됨: ${deletedName}`);
        setDbError(null);
        setDbErrorDetail(null);
        await refreshWorkspaces(updated.id);
      } catch (error) {
        const uiError = toUserError(error, "DB 연결을 삭제하지 못했습니다");
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
    setInventoryWorkspaceId(null);
    setSelectedDbTableKey(null);
  }

  function restoreDbInventory(inventory: DbInventory, selectedTableKey: string | null) {
    setDbInventory(inventory);
    setInventoryWorkspaceId(currentWorkspace?.id ?? null);
    setSelectedDbTableKey(selectedTableKey);
    setDbStatus(dbInventoryStatus(inventory, "불러옴"));
    setDbError(null);
    setDbErrorDetail(null);
  }

  async function storeDbInventory(workspaceId: string, inventory: DbInventory) {
    const presentedInventory = {
      ...inventory,
      partial: Boolean(inventory.partial || dbInventoryTableCount(inventory) > inventory.tables.length),
    };
    setDbInventory(presentedInventory);
    setInventoryWorkspaceId(workspaceId);
    setSelectedDbTableKey(presentedInventory.tables[0] ? dbInventoryTableKey(presentedInventory.tables[0]) : null);
    setDbStatus(dbInventoryStatus(presentedInventory, "읽음"));
    setDbError(null);
    setDbErrorDetail(null);
    await refreshInventorySnapshot(workspaceId);
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
    dbInventory: inventoryWorkspaceId === currentWorkspace?.id ? dbInventory : null,
    selectedDbTableKey: inventoryWorkspaceId === currentWorkspace?.id ? selectedDbTableKey : null,
    setDbProfileName,
    setDbProfileSource,
    setDbProfilePath: updateDbProfilePath,
    setDbConnectionString,
    setSelectedDbTableKey,
    restoreDbInventory,
    pickDbPath,
    saveDbProfile,
    indexDbProfile,
    deleteDbProfile,
    clearDbInventory,
  };
}

function dbInventoryStatus(inventory: DbInventory, action: string): string {
  const tableCount = dbInventoryTableCount(inventory);
  if (tableCount === 0) {
    return "테이블 목록이 비어 있음";
  }
  const columnCount = inventory.tables.reduce((sum, table) => sum + table.columns.length, 0);
  const missingColumnTables = inventory.tables.filter((table) => table.columns.length === 0).length;
  if (missingColumnTables > 0 && columnCount > 0) {
    return `테이블 ${tableCount}개, 컬럼 ${columnCount}개, ${missingColumnTables}개 테이블 컬럼 필요 ${action}`;
  }
  return `테이블 ${tableCount}개, 컬럼 ${columnCount}개 ${action}`;
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
  const tables = countFromJson(indexJson, ["tables_indexed", "tableCount", "tablesCount", "tables"]);
  const columns = countFromJson(indexJson, ["columns_indexed", "columnCount", "columnsCount", "columns"]);
  const facts = [
    tables == null ? null : `테이블 ${tables}개`,
    columns == null ? null : `컬럼 ${columns}개`,
  ].filter((fact): fact is string => fact !== null);
  return facts.length > 0 ? facts.join(", ") : null;
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

export function toDbUserError(value: string, fallback: string): { message: string; details: string } {
  const details = value;
  const lower = value.toLowerCase();

  if (lower.includes("dpi-1047") || lower.includes("oracle client") || lower.includes("oci.dll")) {
    return { message: `${fallback}: Oracle Client를 설치한 뒤 앱을 다시 시작하세요`, details };
  }
  if (
    lower.includes("contract v2") ||
    lower.includes("contract_version") ||
    lower.includes("authority") ||
    lower.includes("inventory가 완전하지") ||
    lower.includes("completeness")
  ) {
    return { message: `${fallback}: 완전한 DB 구조를 확인하지 못했습니다. DB를 다시 읽으세요`, details };
  }
  if (lower.includes("제품 안전 한도") || lower.includes("20,000")) {
    return { message: `${fallback}: 현재 제품의 DB 테이블 처리 한도를 초과했습니다`, details };
  }
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
