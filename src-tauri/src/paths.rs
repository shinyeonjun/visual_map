use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BasePaths {
    pub app_data_dir: PathBuf,
    pub app_state_db: PathBuf,
    pub engines_dir: PathBuf,
    pub workspaces_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppPaths {
    pub app_data_dir: String,
    pub app_state_db: String,
    pub engines_dir: String,
    pub workspaces_dir: String,
}

pub(crate) fn base_paths(app_data_dir: impl AsRef<Path>) -> BasePaths {
    let app_data_dir = app_data_dir.as_ref().to_path_buf();

    BasePaths {
        app_state_db: app_data_dir.join("app-state.sqlite"),
        engines_dir: app_data_dir.join("engines"),
        workspaces_dir: app_data_dir.join("workspaces"),
        app_data_dir,
    }
}

pub(crate) fn ensure_base_dirs(paths: &BasePaths) -> std::io::Result<()> {
    fs::create_dir_all(&paths.app_data_dir)?;
    fs::create_dir_all(&paths.engines_dir)?;
    fs::create_dir_all(&paths.workspaces_dir)?;
    Ok(())
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
        let root = PathBuf::from(r"C:\Users\dev\AppData\Local\CodebaseWorkspace");
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
            "codebase-workspace-paths-test-{}",
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
}
