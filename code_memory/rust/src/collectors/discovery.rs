use std::fs;
use std::path::{Path, PathBuf};

use crate::source::is_managed_provider_root;

pub(crate) const MAX_DESCRIPTOR_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) fn find_files(root: &Path, predicate: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    fn visit(dir: &Path, predicate: &impl Fn(&Path) -> bool, files: &mut Vec<PathBuf>) {
        if is_managed_provider_root(dir) {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !is_excluded_collection_dir(&entry.file_name().to_string_lossy()) {
                    visit(&path, predicate, files);
                }
            } else if file_type.is_file() && predicate(&path) {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    visit(root, &predicate, &mut files);
    files.sort();
    files
}

fn is_excluded_collection_dir(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        ".git"
            | ".dart_tool"
            | ".gradle"
            | ".idea"
            | ".pytest_cache"
            | ".ruby-lsp"
            | ".venv"
            | ".vscode"
            | ".cache"
            | ".code_memory"
            | "__pycache__"
            | "build"
            | "dist"
            | "node_modules"
            | "obj"
            | "out"
            | "target"
            | "tmp"
            | "vendor"
            | "venv"
    )
}

pub(crate) fn read_descriptor(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if metadata.len() > MAX_DESCRIPTOR_BYTES {
        return Err(format!(
            "descriptor exceeds {} bytes: {}",
            MAX_DESCRIPTOR_BYTES,
            path.display()
        ));
    }
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

pub(crate) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn stable_segment(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .replace([' ', ':', '#'], "-")
}

#[cfg(test)]
mod tests {
    use super::find_files;

    #[test]
    fn managed_provider_runtime_is_not_part_of_the_project() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-collector-discovery-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("providers/runtime")).unwrap();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(
            root.join("providers/manifest.json"),
            r#"{"schema":"code-memory.provider-manifest.v1"}"#,
        )
        .unwrap();
        std::fs::write(root.join("providers/runtime/package.json"), "{}").unwrap();
        std::fs::write(root.join("app/package.json"), "{}").unwrap();

        let files = find_files(&root, |path| path.ends_with("package.json"));
        assert_eq!(files, vec![root.join("app/package.json")]);
        let _ = std::fs::remove_dir_all(root);
    }
}
