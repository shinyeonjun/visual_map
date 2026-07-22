import { invoke } from "@tauri-apps/api/core";
import { CheckSquare, Copy, ExternalLink, FolderOpen, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import type { CodeInventoryItem } from "../../types/workspace";
import { copyValue } from "../common/copyValue";

type InvestigationItem = {
  path: string;
  line: number | null;
  column: number | null;
  evidenceId: string;
  checked: boolean;
};

const INVESTIGATION_STORAGE_PREFIX = "backend-visual-map:investigation:v1:";
const INVESTIGATION_LIMIT = 50;

export function SourceJumpRow({
  workspaceId,
  code,
  showInvestigationTray = true,
}: {
  workspaceId: string | null;
  code: CodeInventoryItem;
  showInvestigationTray?: boolean;
}) {
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [statusTone, setStatusTone] = useState<"success" | "error" | null>(null);
  const [investigationItems, setInvestigationItems] = useState<InvestigationItem[]>(() =>
    workspaceId ? loadInvestigation(workspaceId) : [],
  );
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  if (!workspaceId || !code.filePath) {
    return null;
  }
  const sourceWorkspaceId = workspaceId;

  const requestBase = {
    workspaceId,
    path: code.filePath,
  };
  const currentItem: InvestigationItem = {
    path: code.filePath,
    line: code.line ?? null,
    column: code.column ?? null,
    evidenceId: code.id,
    checked: false,
  };
  const currentKey = investigationKey(currentItem);
  const alreadyAdded = investigationItems.some((item) => investigationKey(item) === currentKey);

  function updateInvestigation(next: InvestigationItem[]) {
    const bounded = next.slice(-INVESTIGATION_LIMIT);
    setInvestigationItems(bounded);
    saveInvestigation(sourceWorkspaceId, bounded);
  }

  function addInvestigationItem() {
    if (!alreadyAdded) {
      updateInvestigation([...investigationItems, currentItem]);
    }
  }

  function toggleInvestigationItem(key: string) {
    updateInvestigation(
      investigationItems.map((item) =>
        investigationKey(item) === key ? { ...item, checked: !item.checked } : item,
      ),
    );
  }

  function removeInvestigationItem(key: string) {
    updateInvestigation(investigationItems.filter((item) => investigationKey(item) !== key));
  }

  async function copyInvestigation() {
    const copied = await copyValue(investigationMarkdown(investigationItems));
    setCopyState(copied ? "copied" : "failed");
    window.setTimeout(() => setCopyState("idle"), 1200);
  }

  async function openEditor(editor: "vscode" | "cursor") {
    try {
      setBusyAction(editor);
      setStatus(null);
      setStatusTone(null);
      await invoke("open_source_location", {
        request: {
          ...requestBase,
          line: code.line ?? null,
          column: code.column ?? null,
          editor,
        },
      });
      setStatus(editor === "vscode" ? "VS Code에서 열었습니다" : "Cursor에서 열었습니다");
      setStatusTone("success");
    } catch (error) {
      setStatus(String(error));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  async function revealFile() {
    try {
      setBusyAction("reveal");
      setStatus(null);
      setStatusTone(null);
      await invoke("reveal_source_location", { request: requestBase });
      setStatus("파일 탐색기에서 표시했습니다");
      setStatusTone("success");
    } catch (error) {
      setStatus(String(error));
      setStatusTone("error");
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <>
      <div className="copy-row" aria-label="소스 위치 열기">
        <span className="copy-row-title">열기</span>
        <button type="button" data-source-action="vscode" disabled={busyAction !== null} onClick={() => void openEditor("vscode")}>
          <ExternalLink size={12} />
          <span>{busyAction === "vscode" ? "여는 중" : "VS Code"}</span>
        </button>
        <button type="button" data-source-action="cursor" disabled={busyAction !== null} onClick={() => void openEditor("cursor")}>
          <ExternalLink size={12} />
          <span>{busyAction === "cursor" ? "여는 중" : "Cursor"}</span>
        </button>
        <button type="button" data-source-action="reveal" disabled={busyAction !== null} onClick={() => void revealFile()}>
          <FolderOpen size={12} />
          <span>{busyAction === "reveal" ? "여는 중" : "탐색기"}</span>
        </button>
        <button
          type="button"
          data-investigation-action="add"
          disabled={alreadyAdded}
          onClick={addInvestigationItem}
        >
          <Plus size={12} />
          <span>{alreadyAdded ? "조사함에 있음" : "조사함 추가"}</span>
        </button>
        {status && <small role="status" data-source-status={statusTone}>{status}</small>}
      </div>
      {showInvestigationTray && investigationItems.length > 0 && (
        <details className="investigation-tray-shell">
          <summary><span><CheckSquare size={13} />조사함 <b>{investigationItems.length}</b></span></summary>
          <section className="investigation-tray" aria-label="로컬 조사함">
            <div className="investigation-tray-head">
              <span>확인 목록</span>
              <button
                type="button"
                data-investigation-action="copy"
                data-copy-state={copyState}
                onClick={() => void copyInvestigation()}
              >
                <Copy size={12} />
                {copyState === "copied" ? "복사됨" : copyState === "failed" ? "복사 실패" : "Markdown"}
              </button>
            </div>
            <div className="investigation-list">
              {investigationItems.map((item) => {
                const key = investigationKey(item);
                const location = investigationLocation(item);
                return (
                  <div className={item.checked ? "investigation-item checked" : "investigation-item"} key={key}>
                    <button
                      type="button"
                      className="investigation-check"
                      data-investigation-action="toggle"
                      aria-label={`${sourceFileLabel(item.path)} 확인 ${item.checked ? "해제" : "완료"}`}
                      aria-pressed={item.checked}
                      onClick={() => toggleInvestigationItem(key)}
                    >
                      <span aria-hidden="true">{item.checked ? "✓" : ""}</span>
                    </button>
                    <span className="investigation-location" title={location}>
                      <b>{sourceFileLabel(item.path)}</b>
                      <small>{location}</small>
                      <code>{item.evidenceId}</code>
                    </span>
                    <button
                      type="button"
                      className="investigation-remove"
                      data-investigation-action="remove"
                      aria-label={`${sourceFileLabel(item.path)} 조사함에서 삭제`}
                      onClick={() => removeInvestigationItem(key)}
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                );
              })}
            </div>
            <small className="investigation-privacy" data-investigation-storage="source-location-evidence-check-state-only">
              소스 위치, 근거 ID와 확인 상태만 이 PC에 저장됩니다.
            </small>
          </section>
        </details>
      )}
    </>
  );
}

function investigationKey(item: InvestigationItem): string {
  return `${item.path}\u0000${item.line ?? ""}\u0000${item.column ?? ""}\u0000${item.evidenceId}`;
}

function investigationLocation(item: InvestigationItem): string {
  const line = item.line ? `:${item.line}` : "";
  const column = item.line && item.column ? `:${item.column}` : "";
  return `${item.path}${line}${column}`;
}

function sourceFileLabel(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? path;
}

function loadInvestigation(workspaceId: string): InvestigationItem[] {
  try {
    const value: unknown = JSON.parse(
      window.localStorage.getItem(`${INVESTIGATION_STORAGE_PREFIX}${workspaceId}`) ?? "[]",
    );
    return Array.isArray(value) ? value.filter(isInvestigationItem).slice(-INVESTIGATION_LIMIT) : [];
  } catch {
    return [];
  }
}

function saveInvestigation(workspaceId: string, items: InvestigationItem[]) {
  try {
    window.localStorage.setItem(`${INVESTIGATION_STORAGE_PREFIX}${workspaceId}`, JSON.stringify(items));
  } catch {
    // The tray remains usable for the current selection when local storage is unavailable.
  }
}

function isInvestigationItem(value: unknown): value is InvestigationItem {
  if (!value || typeof value !== "object") {
    return false;
  }
  const item = value as Partial<InvestigationItem>;
  return Boolean(
    typeof item.path === "string" && item.path.length > 0 && item.path.length <= 4096 &&
      isOptionalPosition(item.line) &&
      isOptionalPosition(item.column) &&
      typeof item.evidenceId === "string" && item.evidenceId.length > 0 && item.evidenceId.length <= 1024 &&
      typeof item.checked === "boolean",
  );
}

function isOptionalPosition(value: unknown): value is number | null {
  return value === null || (Number.isInteger(value) && Number(value) > 0 && Number(value) <= 0xffff_ffff);
}

function investigationMarkdown(items: InvestigationItem[]): string {
  const lines = ["# Backend Visual Map 조사", ""];
  for (const item of items) {
    const location = investigationLocation(item).replace(/`/g, "'");
    const evidenceId = item.evidenceId.replace(/`/g, "'");
    lines.push(`- [${item.checked ? "x" : " "}] \`${location}\` (근거: \`${evidenceId}\`)`);
  }
  return `${lines.join("\n")}\n`;
}
