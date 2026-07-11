use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePaths {
    pub app_data_dir: PathBuf,
    pub app_state_db: PathBuf,
    pub engines_dir: PathBuf,
    pub workspaces_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub app_data_dir: String,
    pub app_state_db: String,
    pub engines_dir: String,
    pub workspaces_dir: String,
}

pub fn base_paths(app_data_dir: impl AsRef<Path>) -> BasePaths {
    let app_data_dir = app_data_dir.as_ref().to_path_buf();

    BasePaths {
        app_state_db: app_data_dir.join("app-state.sqlite"),
        engines_dir: app_data_dir.join("engines"),
        workspaces_dir: app_data_dir.join("workspaces"),
        app_data_dir,
    }
}

pub fn ensure_base_dirs(paths: &BasePaths) -> std::io::Result<()> {
    fs::create_dir_all(&paths.app_data_dir)?;
    fs::create_dir_all(&paths.engines_dir)?;
    fs::create_dir_all(&paths.workspaces_dir)?;
    Ok(())
}

/// Uses LocalAppData for machine-local caches while preserving an existing RoamingAppData
/// installation. A Local profile with workspace/state data is authoritative. A directory created
/// only by WebView or engine setup is not treated as a user profile, so an old workspace can be
/// moved into it without overwriting any Local user data.
pub fn migrate_roaming_data_to_local(local: PathBuf, roaming: PathBuf) -> std::io::Result<PathBuf> {
    if !roaming.exists() {
        return Ok(local);
    }
    if local.exists() {
        if has_user_state(&local) || !has_user_state(&roaming) {
            return Ok(local);
        }
        return move_roaming_user_state_into_empty_local(local, roaming);
    }
    if let Some(parent) = local.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(&roaming, &local) {
        Ok(()) => Ok(local),
        // A locked cache or a cross-volume policy must not make an existing workspace disappear.
        // Continue from RoamingAppData and retry migration on a later launch.
        Err(_) => Ok(roaming),
    }
}

fn has_user_state(root: &Path) -> bool {
    root.join("app-state.sqlite").is_file() || directory_has_entries(&root.join("workspaces"))
}

fn directory_has_entries(path: &Path) -> bool {
    fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn move_roaming_user_state_into_empty_local(
    local: PathBuf,
    roaming: PathBuf,
) -> std::io::Result<PathBuf> {
    let roaming_workspaces = roaming.join("workspaces");
    let local_workspaces = local.join("workspaces");
    if directory_has_entries(&roaming_workspaces) {
        if directory_has_entries(&local_workspaces) {
            return Ok(local);
        }
        if local_workspaces.exists() {
            fs::remove_dir(&local_workspaces)?;
        }
        if fs::rename(&roaming_workspaces, &local_workspaces).is_err() {
            return Ok(roaming);
        }
    }

    let roaming_state = roaming.join("app-state.sqlite");
    let local_state = local.join("app-state.sqlite");
    if roaming_state.is_file() && !local_state.exists() {
        // Workspace records are already moved above. A state-file failure only leaves optional
        // UI preferences behind; it never causes the recovered workspace to be hidden again.
        let _ = fs::rename(roaming_state, local_state);
    }
    Ok(local)
}

impl From<BasePaths> for AppPaths {
    fn from(paths: BasePaths) -> Self {
        Self {
            app_data_dir: paths.app_data_dir.display().to_string(),
            app_state_db: paths.app_state_db.display().to_string(),
            engines_dir: paths.engines_dir.display().to_string(),
            workspaces_dir: paths.workspaces_dir.display().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_paths_are_derived_from_app_data_dir() {
        let root = PathBuf::from(r"C:\Users\dev\AppData\Local\BackendVisualMap");
        let paths = base_paths(&root);

        assert_eq!(paths.app_data_dir, root);
        assert_eq!(
            paths.app_state_db,
            paths.app_data_dir.join("app-state.sqlite")
        );
        assert_eq!(paths.engines_dir, paths.app_data_dir.join("engines"));
        assert_eq!(paths.workspaces_dir, paths.app_data_dir.join("workspaces"));
    }

    #[test]
    fn ensure_base_dirs_creates_required_directories() {
        let root = std::env::temp_dir().join(format!(
            "backend-visual-map-paths-test-{}",
            std::process::id()
        ));
        let paths = base_paths(&root);

        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }

        ensure_base_dirs(&paths).unwrap();

        assert!(paths.app_data_dir.is_dir());
        assert!(paths.engines_dir.is_dir());
        assert!(paths.workspaces_dir.is_dir());
        assert!(!paths.app_state_db.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrates_existing_roaming_data_without_overwriting_local_data() {
        let root = std::env::temp_dir().join(format!(
            "backend-visual-map-path-migration-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let roaming = root.join("Roaming").join("BackendVisualMap");
        let local = root.join("Local").join("BackendVisualMap");
        fs::create_dir_all(&roaming).unwrap();
        fs::write(roaming.join("workspace.json"), "preserve-me").unwrap();

        assert_eq!(
            migrate_roaming_data_to_local(local.clone(), roaming.clone()).unwrap(),
            local
        );
        assert!(!roaming.exists());
        assert_eq!(
            fs::read_to_string(local.join("workspace.json")).unwrap(),
            "preserve-me"
        );

        fs::create_dir_all(&roaming).unwrap();
        fs::write(roaming.join("older.json"), "do-not-merge").unwrap();
        assert_eq!(
            migrate_roaming_data_to_local(local.clone(), roaming.clone()).unwrap(),
            local
        );
        assert!(roaming.exists());
        assert!(!local.join("older.json").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moves_roaming_workspace_when_local_only_contains_runtime_files() {
        let root = std::env::temp_dir().join(format!(
            "backend-visual-map-empty-local-migration-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let roaming = root.join("Roaming").join("BackendVisualMap");
        let local = root.join("Local").join("BackendVisualMap");
        fs::create_dir_all(local.join("workspaces")).unwrap();
        fs::create_dir_all(local.join("EBWebView")).unwrap();
        fs::create_dir_all(roaming.join("workspaces").join("workspace-1")).unwrap();
        fs::write(
            roaming
                .join("workspaces")
                .join("workspace-1")
                .join("workspace.json"),
            "preserve-me",
        )
        .unwrap();

        assert_eq!(
            migrate_roaming_data_to_local(local.clone(), roaming.clone()).unwrap(),
            local
        );
        assert_eq!(
            fs::read_to_string(
                local
                    .join("workspaces")
                    .join("workspace-1")
                    .join("workspace.json")
            )
            .unwrap(),
            "preserve-me"
        );
        assert!(local.join("EBWebView").is_dir());
        assert!(!roaming.join("workspaces").exists());

        fs::remove_dir_all(root).unwrap();
    }
}
