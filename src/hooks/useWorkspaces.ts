import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { githubRepoName, repoPathErrorFor } from "../app/appState";
import { toUserError } from "../app/operationStatus";
import { hasTauriRuntime, tauriUnavailableMessage } from "../app/tauriRuntime";
import {
  workspaceRepoInputValue,
  type CreateWorkspaceRequest,
  type RepoSourceMode,
  type Workspace,
  type WorkspaceRecoveryWarning,
} from "../types/workspace";

type WithBusy = (action: string, task: () => Promise<void>) => Promise<void>;
const lastWorkspaceKey = "backend-visual-map:last-workspace";
const mapContextKeyPrefix = "backend-visual-map:map-context:v1:";
const investigationKeyPrefix = "backend-visual-map:investigation:v1:";

export function useWorkspaces({ withBusy }: { withBusy: WithBusy }) {
  const [initialized, setInitialized] = useState(!hasTauriRuntime());
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [recoveryWarnings, setRecoveryWarnings] = useState<WorkspaceRecoveryWarning[]>([]);
  const [currentWorkspace, setCurrentWorkspace] = useState<Workspace | null>(null);
  const [repoSourceMode, setRepoSourceMode] = useState<RepoSourceMode>("local");
  const [workspaceName, setWorkspaceName] = useState("");
  const [repoPath, setRepoPath] = useState("");
  const [workspaceStatus, setWorkspaceStatus] = useState<string | null>(null);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);

  useEffect(() => {
    void refreshWorkspaces();
  }, []);

  async function refreshWorkspaces(preferredWorkspaceId?: string) {
    if (!hasTauriRuntime()) {
      setWorkspaces([]);
      setRecoveryWarnings([]);
      selectWorkspace(null);
      setWorkspaceError(null);
      setInitialized(true);
      return;
    }

    try {
      const [items, warnings] = await Promise.all([
        invoke<Workspace[]>("list_workspaces"),
        invoke<WorkspaceRecoveryWarning[]>("get_workspace_recovery_warnings"),
      ]);
      setWorkspaces(items);
      setRecoveryWarnings(warnings);
      const selected =
        (preferredWorkspaceId ? items.find((workspace) => workspace.id === preferredWorkspaceId) : null) ??
        (currentWorkspace && items.find((workspace) => workspace.id === currentWorkspace.id)) ??
        items.find((workspace) => workspace.id === rememberedWorkspaceId()) ??
        items[0] ??
        null;

      selectWorkspace(selected);
      setWorkspaceError(null);
    } catch (error) {
      setWorkspaceError(toUserError(error, "프로젝트 목록을 불러오지 못했습니다").message);
    } finally {
      setInitialized(true);
    }
  }

  function selectWorkspace(workspace: Workspace | null) {
    setCurrentWorkspace(workspace);
    if (!workspace) {
      return;
    }

    rememberWorkspace(workspace.id);
    setWorkspaceName(workspace.name);
    setRepoPath(workspaceRepoInputValue(workspace));
    setRepoSourceMode(workspace.repoSource);
  }

  async function pickRepoPath() {
    if (!hasTauriRuntime()) {
      setWorkspaceError(tauriUnavailableMessage);
      return;
    }

    let selected: string | string[] | null;
    try {
      selected = await open({
        directory: true,
        multiple: false,
        title: "저장소 폴더 선택",
      });
    } catch (error) {
      setWorkspaceError(toUserError(error, "프로젝트 폴더 선택기를 열지 못했습니다").message);
      return;
    }

    if (!selected || Array.isArray(selected)) {
      return;
    }

    updateRepoPath(selected);
    if (!workspaceName.trim()) {
      setWorkspaceName(lastPathPart(selected));
    }
  }

  async function createWorkspace() {
    await withBusy(repoSourceMode === "github" ? "workspace-clone" : "workspace-create", async () => {
      const request: CreateWorkspaceRequest = {
        name: workspaceName.trim(),
        repoPath: repoPath.trim(),
      };

      if (!request.name || !request.repoPath) {
        setWorkspaceStatus(null);
        setWorkspaceError("프로젝트 이름과 저장소 경로를 입력하세요");
        return;
      }
      const repoPathError = repoPathErrorFor(request.repoPath, repoSourceMode);
      if (repoPathError) {
        setWorkspaceStatus(null);
        setWorkspaceError(repoPathError);
        return;
      }
      if (!hasTauriRuntime()) {
        setWorkspaceStatus(null);
        setWorkspaceError(tauriUnavailableMessage);
        return;
      }

      try {
        const created = await invoke<Workspace>("create_workspace", { request });
        selectWorkspace(created);
        setWorkspaceStatus(
          repoSourceMode === "github"
            ? `GitHub 저장소 복제 후 프로젝트 열림: ${created.name}`
            : `프로젝트 열림: ${created.name}`,
        );
        setWorkspaceError(null);
        await refreshWorkspaces(created.id);
      } catch (error) {
        setWorkspaceStatus(null);
        setWorkspaceError(
          toUserError(
            error,
            repoSourceMode === "github" ? "GitHub 저장소를 복제하지 못했습니다" : "프로젝트를 열지 못했습니다",
          ).message,
        );
      }
    });
  }

  function updateRepoPath(value: string) {
    const previousDerivedName = workspaceNameForPath(repoPath, repoSourceMode);
    setRepoPath(value);
    const nextMode = repoModeForValue(value) ?? repoSourceMode;
    if (nextMode !== repoSourceMode) {
      setRepoSourceMode(nextMode);
    }
    if (workspaceName.trim() && workspaceName.trim() !== previousDerivedName) {
      return;
    }

    const derivedName = workspaceNameForPath(value, nextMode);
    if (derivedName) {
      setWorkspaceName(derivedName);
    }
  }

  function chooseRepoSourceMode(value: RepoSourceMode) {
    const previousDerivedName = workspaceNameForPath(repoPath, repoSourceMode);
    setRepoSourceMode(value);
    if (workspaceName.trim() && workspaceName.trim() !== previousDerivedName) {
      return;
    }

    const derivedName = workspaceNameForPath(repoPath, value);
    if (derivedName) {
      setWorkspaceName(derivedName);
    }
  }

  async function openWorkspace(workspaceId: string) {
    if (!workspaceId) {
      return;
    }

    await withBusy("workspace-open", async () => {
      if (!hasTauriRuntime()) {
        setWorkspaceStatus(null);
        setWorkspaceError(tauriUnavailableMessage);
        return;
      }

      try {
        const opened = await invoke<Workspace>("open_workspace", { workspaceId });
        selectWorkspace(opened);
        setWorkspaceStatus(`프로젝트 열림: ${opened.name}`);
        setWorkspaceError(null);
        await refreshWorkspaces(opened.id);
      } catch (error) {
        setWorkspaceStatus(null);
        setWorkspaceError(toUserError(error, "프로젝트를 열지 못했습니다").message);
      }
    });
  }

  async function refreshGithubWorkspace(): Promise<boolean> {
    const workspaceId = currentWorkspace?.id;
    if (!workspaceId || currentWorkspace.repoSource !== "github") {
      return false;
    }
    if (!hasTauriRuntime()) {
      setWorkspaceStatus(null);
      setWorkspaceError(tauriUnavailableMessage);
      return false;
    }

    let refreshed = false;
    await withBusy("workspace-refresh", async () => {
      try {
        const updated = await invoke<Workspace>("refresh_github_workspace", { workspaceId });
        selectWorkspace(updated);
        setWorkspaceStatus(`GitHub 업데이트 완료: ${updated.name}`);
        setWorkspaceError(null);
        await refreshWorkspaces(updated.id);
        refreshed = true;
      } catch (error) {
        setWorkspaceStatus(null);
        setWorkspaceError(toUserError(error, "GitHub 프로젝트를 업데이트하지 못했습니다").message);
      }
    });
    return refreshed;
  }

  async function repairWorkspaceFromBackup(workspaceId: string) {
    if (!workspaceId || !hasTauriRuntime()) {
      return;
    }
    await withBusy("workspace-repair", async () => {
      try {
        const repaired = await invoke<Workspace>("repair_workspace_from_backup", { workspaceId });
        setWorkspaceStatus(`백업에서 프로젝트를 복구했습니다: ${repaired.name}`);
        setWorkspaceError(null);
        await refreshWorkspaces(repaired.id);
      } catch (error) {
        setWorkspaceStatus(null);
        setWorkspaceError(toUserError(error, "프로젝트 백업을 복구하지 못했습니다").message);
      }
    });
  }

  async function deleteWorkspace(workspaceId: string) {
    if (!workspaceId || !hasTauriRuntime()) {
      return;
    }
    const deletedName = workspaces.find((workspace) => workspace.id === workspaceId)?.name ?? "프로젝트";
    await withBusy("workspace-delete", async () => {
      try {
        await invoke("delete_workspace", { workspaceId });
        clearLocalWorkspaceState(workspaceId);
        setWorkspaces((items) => items.filter((workspace) => workspace.id !== workspaceId));
        if (currentWorkspace?.id === workspaceId) {
          setCurrentWorkspace(null);
          setWorkspaceName("");
          setRepoPath("");
        }
        await refreshWorkspaces();
        setWorkspaceStatus(`프로젝트 제거됨: ${deletedName}`);
        setWorkspaceError(null);
      } catch (error) {
        setWorkspaceStatus(null);
        setWorkspaceError(toUserError(error, "프로젝트를 제거하지 못했습니다").message);
      }
    });
  }

  return {
    initialized,
    workspaces,
    recoveryWarnings,
    currentWorkspace,
    repoSourceMode,
    workspaceName,
    repoPath,
    workspaceStatus,
    workspaceError,
    setRepoSourceMode: chooseRepoSourceMode,
    setWorkspaceName,
    setRepoPath: updateRepoPath,
    setCurrentWorkspace: selectWorkspace,
    pickRepoPath,
    createWorkspace,
    openWorkspace,
    refreshGithubWorkspace,
    refreshWorkspaces,
    repairWorkspaceFromBackup,
    deleteWorkspace,
  };
}

function lastPathPart(value: string): string {
  const parts = value.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? value;
}

function repoModeForValue(value: string): RepoSourceMode | null {
  const trimmed = value.trim();
  if (/^https?:\/\//i.test(trimmed) || /^git@/i.test(trimmed)) {
    return "github";
  }
  if (/^[a-z]:[\\/]/i.test(trimmed) || trimmed.startsWith("\\\\") || trimmed.startsWith("/")) {
    return "local";
  }
  return null;
}

function workspaceNameForPath(value: string, mode: RepoSourceMode): string | null {
  return mode === "github" ? githubRepoName(value) : lastPathPart(value);
}

function rememberedWorkspaceId(): string | null {
  try {
    return window.localStorage.getItem(lastWorkspaceKey);
  } catch {
    return null;
  }
}

function rememberWorkspace(workspaceId: string) {
  try {
    window.localStorage.setItem(lastWorkspaceKey, workspaceId);
  } catch {
    // The workspace list still falls back to its most recently updated entry.
  }
}

function clearLocalWorkspaceState(workspaceId: string) {
  try {
    if (window.localStorage.getItem(lastWorkspaceKey) === workspaceId) {
      window.localStorage.removeItem(lastWorkspaceKey);
    }
    window.localStorage.removeItem(`${mapContextKeyPrefix}${workspaceId}`);
    window.localStorage.removeItem(`${investigationKeyPrefix}${workspaceId}`);
  } catch {
    // Filesystem deletion remains authoritative when local storage is unavailable.
  }
}
