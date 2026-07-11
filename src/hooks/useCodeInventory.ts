import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { toUserError } from "../app/operationStatus";
import {
  codeInventoryCodeItems,
  codeInventoryItemCount,
  type CodeInventory,
  type CodeInventoryItem,
  type DbInventory,
  type IndexCodeRequest,
  type Workspace,
} from "../types/workspace";

type WithBusy = (action: string, task: () => Promise<void>) => Promise<void>;

export function useCodeInventory({
  currentWorkspace,
  withBusy,
  setCurrentWorkspace,
  refreshWorkspaces,
  saveInventorySnapshot,
  getDbInventory,
}: {
  currentWorkspace: Workspace | null;
  withBusy: WithBusy;
  setCurrentWorkspace: (workspace: Workspace | null) => void;
  refreshWorkspaces: (preferredWorkspaceId?: string) => Promise<void>;
  saveInventorySnapshot: (workspaceId: string, code: CodeInventory | null, db: DbInventory | null) => Promise<void>;
  getDbInventory: () => DbInventory | null;
}) {
  const [codeStatus, setCodeStatus] = useState<string | null>(null);
  const [codeError, setCodeError] = useState<string | null>(null);
  const [codeErrorDetail, setCodeErrorDetail] = useState<string | null>(null);
  const [codeInventory, setCodeInventory] = useState<CodeInventory | null>(null);
  const [selectedCodeItem, setSelectedCodeItem] = useState<CodeInventoryItem | null>(null);

  useEffect(() => {
    clearCodeInventory();
    setCodeStatus(null);
    setCodeError(null);
    setCodeErrorDetail(null);
  }, [currentWorkspace?.id]);

  async function indexCodeRepository() {
    if (!currentWorkspace) {
      setCodeError("프로젝트를 연 뒤 코드 읽기를 실행하세요");
      return;
    }

    const request: IndexCodeRequest = { workspaceId: currentWorkspace.id };

    await withBusy("code-index", async () => {
      try {
        setCodeStatus("저장소 구조 읽는 중...");
        setCodeError(null);
        setCodeErrorDetail(null);
        const result = await invoke<{ workspace: Workspace; run: { ok: boolean; stderr: string } }>(
          "index_code_repository",
          { request },
        );
        setCurrentWorkspace(result.workspace);
        if (result.run.ok) {
          clearCodeInventory();
          setCodeStatus("코드 목록 불러오는 중...");
          try {
            await loadCodeInventoryForWorkspace(result.workspace.id, "읽음");
          } catch (error) {
            const uiError = toUserError(error, "코드 목록을 불러오지 못했습니다");
            setCodeStatus("코드 구조 읽기 완료");
            setCodeError(uiError.message);
            setCodeErrorDetail(uiError.details);
          }
        } else {
          const uiError = toUserError(result.run.stderr || "코드 읽기 실패", "코드 읽기 실패");
          setCodeStatus(null);
          setCodeError(uiError.message);
          setCodeErrorDetail(uiError.details);
        }
        await refreshWorkspaces(result.workspace.id);
      } catch (error) {
        const uiError = toUserError(error, "코드 읽기 실패");
        setCodeStatus(null);
        setCodeError(uiError.message);
        setCodeErrorDetail(uiError.details);
      }
    });
  }

  async function loadCodeInventory() {
    if (!currentWorkspace) {
      setCodeError("프로젝트를 연 뒤 코드 목록을 불러오세요");
      return;
    }

    const workspaceId = currentWorkspace.id;

    await withBusy("code-load", async () => {
      try {
        await loadCodeInventoryForWorkspace(workspaceId, "불러옴");
      } catch (error) {
        const uiError = toUserError(error, "코드 목록을 불러오지 못했습니다");
        clearCodeInventory();
        setCodeStatus(null);
        setCodeError(uiError.message);
        setCodeErrorDetail(uiError.details);
      }
    });
  }

  function clearCodeInventory() {
    setCodeInventory(null);
    setSelectedCodeItem(null);
  }

  function restoreCodeInventory(inventory: CodeInventory) {
    setCodeInventory(inventory);
    setSelectedCodeItem(null);
    setCodeStatus(codeInventoryStatus(inventory, "불러옴"));
    setCodeError(null);
    setCodeErrorDetail(null);
  }

  async function loadCodeInventoryForWorkspace(workspaceId: string, action: string) {
    const inventory = await invoke<CodeInventory>("get_code_inventory", { workspaceId });
    setCodeInventory(inventory);
    setSelectedCodeItem(firstCodeInventoryItem(inventory));
    setCodeStatus(codeInventoryStatus(inventory, action));
    setCodeError(null);
    setCodeErrorDetail(null);
    void saveInventorySnapshot(workspaceId, inventory, getDbInventory());
  }

  return {
    codeStatus,
    codeError,
    codeErrorDetail,
    codeInventory,
    selectedCodeItem,
    setSelectedCodeItem,
    restoreCodeInventory,
    indexCodeRepository,
    loadCodeInventory,
    clearCodeInventory,
  };
}

function firstCodeInventoryItem(inventory: CodeInventory): CodeInventoryItem | null {
  return inventory.routes[0] ?? codeInventoryCodeItems(inventory)[0] ?? inventory.files[0] ?? null;
}

function codeInventoryStatus(inventory: CodeInventory, action: string): string {
  const count = codeInventoryItemCount(inventory);
  const routeText = inventory.routes.length > 0 ? `API ${inventory.routes.length}개` : "API 라우트 없음";
  const codeText = codeInventoryCodeItems(inventory).length > 0 ? `코드 ${count}개` : `파일 ${inventory.files.length}개`;
  return count > 0 ? `${codeText} ${action} · ${routeText}` : `코드 목록이 비어 있음`;
}
