fn ancestor_workspace_root(language: &str, root: &Path) -> Option<PathBuf> {
    for ancestor in root.ancestors().take(8) {
        let marker = match language {
            "go" if ancestor.join("go.work").is_file() => true,
            "java"
                if ancestor.join("settings.gradle").is_file()
                    || ancestor.join("settings.gradle.kts").is_file() =>
            {
                true
            }
            "java"
                if fs::read_to_string(ancestor.join("pom.xml"))
                    .ok()
                    .is_some_and(|source| {
                        source.contains("<modules>") && source.contains("<module>")
                    }) =>
            {
                true
            }
            "rust" => fs::read_to_string(ancestor.join("Cargo.toml"))
                .ok()
                .is_some_and(|source| source.lines().any(|line| line.trim() == "[workspace]")),
            _ => false,
        };
        if marker {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

pub(crate) fn workspace_has_marker(language: &str, path: &Path) -> bool {
    let markers: &[&str] = match language {
        "rust" => &["Cargo.toml"],
        "go" => &["go.work", "go.mod"],
        "java" => &[
            "pom.xml",
            "settings.gradle",
            "settings.gradle.kts",
            "build.gradle",
            "build.gradle.kts",
        ],
        "dart" => &["pubspec.yaml"],
        "python" => &[
            "pyproject.toml",
            "pyrightconfig.json",
            "setup.py",
            "setup.cfg",
        ],
        _ => &[],
    };
    markers.iter().any(|marker| path.join(marker).is_file())
}

pub(crate) fn dart_dependency_metadata_gap(root: &Path) -> Option<String> {
    for ancestor in root.ancestors().take(8) {
        let manifest = ancestor.join("pubspec.yaml");
        let Ok(source) = fs::read_to_string(&manifest) else {
            continue;
        };
        let requires_workspace_resolution = source.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == "workspace:"
                || trimmed.starts_with("workspace:")
                || trimmed == "resolution: workspace"
                || trimmed == "flutter:"
                || trimmed == "flutter_test:"
                || (trimmed.starts_with("dependencies:") && !trimmed.contains("{}"))
                || (trimmed.starts_with("dev_dependencies:") && !trimmed.contains("{}"))
                || (trimmed.starts_with("dependency_overrides:") && !trimmed.contains("{}"))
        });
        if !requires_workspace_resolution {
            continue;
        }
        let package_config = ancestor.join(".dart_tool").join("package_config.json");
        if !package_config.is_file() {
            return Some(format!(
                "Dart dependency metadata is unavailable at {}; refusing analysis_server startup without the project's resolved .dart_tool/package_config.json (no dependency installation is performed)",
                ancestor.display()
            ));
        }
        return None;
    }
    None
}

pub(crate) fn dart_package_config(root: &Path) -> Result<PathBuf, String> {
    for ancestor in root.ancestors().take(8) {
        let path = ancestor.join(".dart_tool").join("package_config.json");
        if dart_package_config_is_valid(&path) {
            return Ok(path);
        }
    }

    let output = project_cache_root(root)
        .join("dart")
        .join("package_config.synthetic.json");
    let package_root = root
        .ancestors()
        .find(|ancestor| ancestor.join("melos.yaml").is_file())
        .unwrap_or(root);
    let mut manifests = Vec::new();
    collect_dart_package_manifests(package_root, &mut manifests);
    manifests.sort();
    manifests.dedup();

    let mut packages = Vec::new();
    let mut names = HashSet::new();
    for manifest in manifests {
        let Some(package_name) = fs::read_to_string(&manifest)
            .ok()
            .and_then(|source| dart_yaml_scalar(&source, "name"))
        else {
            continue;
        };
        let Some(package_root) = manifest.parent() else {
            continue;
        };
        if !names.insert(package_name.clone()) {
            continue;
        }
        let package_uri = if package_root.join("lib").is_dir() {
            "lib/"
        } else {
            "./"
        };
        packages.push(serde_json::json!({
            "name": package_name,
            "rootUri": path_to_uri(package_root),
            "packageUri": package_uri
        }));
    }

    if packages.is_empty() {
        return Err(format!(
            "Dart package map could not find a local package under {}",
            root.display()
        ));
    }
    let value = serde_json::json!({
        "configVersion": 2,
        "packages": packages
    });
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("cannot serialize Dart package map: {error}"))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Dart package map directory: {error}"))?;
    }
    fs::write(&output, bytes).map_err(|error| {
        format!(
            "cannot write Dart package map {}: {error}",
            output.display()
        )
    })?;
    Ok(output)
}
