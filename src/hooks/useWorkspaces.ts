import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { toUserError } from "../app/operationStatus";
import { hasTauriRuntime, tauriUnavailableMessage } from "../app/tauriRuntime";
import type { CreateWorkspaceRequest, RepoSourceMode, Workspace, WorkspaceRecoveryWarning } from "../types/workspace";

type WithBusy = (action: string, task: () => Promise<void>) => Promise<void>;

export function useWorkspaces({ withBusy }: { withBusy: WithBusy }) {
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
        items[0] ??
        null;

      selectWorkspace(selected);
      setWorkspaceError(null);
    } catch (error) {
      setWorkspaceError(toUserError(error, "프로젝트 목록을 불러오지 못했습니다").message);
    }
  }

  function selectWorkspace(workspace: Workspace | null) {
    setCurrentWorkspace(workspace);
    if (!workspace) {
      return;
    }

    setWorkspaceName(workspace.name);
    setRepoPath(workspace.repoPath);
    setRepoSourceMode("local");
  }

  async function pickRepoPath() {
    if (!hasTauriRuntime()) {
      setWorkspaceError(tauriUnavailableMessage);
      return;
    }

    const selected = await open({
      directory: true,
      multiple: false,
      title: "저장소 폴더 선택",
    });

    if (!selected) {
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
      if (repoSourceMode === "local" && (/^https?:\/\//i.test(request.repoPath) || /^git@/i.test(request.repoPath))) {
        setWorkspaceStatus(null);
        setWorkspaceError("GitHub URL 모드로 전환하세요.");
        return;
      }
      if (repoSourceMode === "github" && !githubRepoName(request.repoPath)) {
        setWorkspaceStatus(null);
        setWorkspaceError("https://github.com/owner/repo 형식의 GitHub URL을 입력하세요.");
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

  return {
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
    refreshWorkspaces,
    repairWorkspaceFromBackup,
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

function githubRepoName(value: string): string | null {
  const trimmed = value.trim().replace(/\/$/, "");
  const path =
    trimmed.match(/^https:\/\/github\.com\/(.+)$/i)?.[1] ?? trimmed.match(/^git@github\.com:(.+)$/i)?.[1];
  if (!path) {
    return null;
  }

  const parts = path.split("/");
  if (parts.length !== 2) {
    return null;
  }

  const repo = parts[1].replace(/\.git$/i, "");
  return /^[a-z0-9._-]+$/i.test(parts[0]) && /^[a-z0-9._-]+$/i.test(repo) ? repo : null;
}
