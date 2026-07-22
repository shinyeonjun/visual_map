import { invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useState } from "react";
import { toUserError } from "../app/operationStatus";
import {
  codeInventoryCodeItems,
  codeInventoryDefaultRoute,
  codeInventoryFileCount,
  codeInventoryItemCount,
  codeInventoryRouteCount,
  codeInventorySymbolCount,
  type CodeInventory,
  type CodeInventoryItem,
  type IndexCodeRequest,
  type Workspace,
} from "../types/workspace";

type WithBusy = (action: string, task: () => Promise<void>) => Promise<void>;

export function useCodeInventory({
  currentWorkspace,
  withBusy,
  setCurrentWorkspace,
  refreshWorkspaces,
  refreshInventorySnapshot,
}: {
  currentWorkspace: Workspace | null;
  withBusy: WithBusy;
  setCurrentWorkspace: (workspace: Workspace | null) => void;
  refreshWorkspaces: (preferredWorkspaceId?: string) => Promise<void>;
  refreshInventorySnapshot: (workspaceId: string) => Promise<void>;
}) {
  const [codeStatus, setCodeStatus] = useState<string | null>(null);
  const [codeError, setCodeError] = useState<string | null>(null);
  const [codeErrorDetail, setCodeErrorDetail] = useState<string | null>(null);
  const [codeInventory, setCodeInventory] = useState<CodeInventory | null>(null);
  const [inventoryWorkspaceId, setInventoryWorkspaceId] = useState<string | null>(null);
  const [selectedCodeItem, setSelectedCodeItem] = useState<CodeInventoryItem | null>(null);

  useLayoutEffect(() => {
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
        const result = await invoke<{
          workspace: Workspace;
          run: { ok: boolean; stderr: string };
          inventory?: CodeInventory | null;
          inventoryError?: string | null;
        }>("index_code_repository", { request });
        setCurrentWorkspace(result.workspace);
        if (result.run.ok) {
          clearCodeInventory();
          if (!result.inventory) {
            const uiError = toUserError(result.inventoryError ?? "코드 inventory가 없습니다", "코드 목록을 불러오지 못했습니다");
            setCodeStatus("코드 구조 읽기 완료");
            setCodeError(uiError.message);
            setCodeErrorDetail(uiError.details);
          } else {
            setCodeStatus("코드 목록 저장 중...");
            try {
              await storeCodeInventory(result.workspace.id, result.inventory);
            } catch (error) {
              const uiError = toUserError(error, "코드 읽기 결과를 저장하지 못했습니다");
              setCodeStatus("코드 구조 읽기 완료");
              setCodeError(uiError.message);
              setCodeErrorDetail(uiError.details);
            }
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

  function clearCodeInventory() {
    setCodeInventory(null);
    setInventoryWorkspaceId(null);
    setSelectedCodeItem(null);
  }

  function restoreCodeInventory(inventory: CodeInventory) {
    setCodeInventory(inventory);
    setInventoryWorkspaceId(currentWorkspace?.id ?? null);
    setSelectedCodeItem(firstCodeInventoryItem(inventory));
    setCodeStatus(codeInventoryStatus(inventory, "불러옴"));
    setCodeError(null);
    setCodeErrorDetail(null);
  }

  async function storeCodeInventory(workspaceId: string, inventory: CodeInventory) {
    setCodeInventory(inventory);
    setInventoryWorkspaceId(workspaceId);
    setSelectedCodeItem(firstCodeInventoryItem(inventory));
    setCodeStatus(codeInventoryStatus(inventory, "읽음"));
    setCodeError(null);
    setCodeErrorDetail(null);
    await refreshInventorySnapshot(workspaceId);
  }

  return {
    codeStatus,
    codeError,
    codeErrorDetail,
    codeInventory: inventoryWorkspaceId === currentWorkspace?.id ? codeInventory : null,
    selectedCodeItem: inventoryWorkspaceId === currentWorkspace?.id ? selectedCodeItem : null,
    setSelectedCodeItem,
    restoreCodeInventory,
    indexCodeRepository,
    clearCodeInventory,
  };
}

function firstCodeInventoryItem(inventory: CodeInventory): CodeInventoryItem | null {
  return codeInventoryDefaultRoute(inventory) ?? codeInventoryCodeItems(inventory)[0] ?? inventory.files[0] ?? null;
}

function codeInventoryStatus(inventory: CodeInventory, action: string): string {
  const count = codeInventoryItemCount(inventory);
  const routeCount = codeInventoryRouteCount(inventory);
  const routeText = routeCount > 0 ? `API ${routeCount}개` : "API 라우트 없음";
  const codeText = codeInventorySymbolCount(inventory) > 0 ? `코드 ${count}개` : `파일 ${codeInventoryFileCount(inventory)}개`;
  return count > 0 ? `${codeText} ${action} · ${routeText}` : `코드 목록이 비어 있음`;
}
