fn dart_package_config_is_valid(path: &Path) -> bool {
    let Ok(value) = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or(())
    else {
        return false;
    };
    value.get("configVersion").and_then(Value::as_u64) == Some(2)
        && value.get("packages").and_then(Value::as_array).is_some()
}

fn collect_dart_package_manifests(root: &Path, output: &mut Vec<PathBuf>) {
    if crate::source::is_managed_provider_root(root) {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() && name == "pubspec.yaml" {
            output.push(path);
        } else if file_type.is_dir()
            && !matches!(
                name.as_str(),
                ".git" | ".dart_tool" | "build" | "node_modules" | "vendor" | "target"
            )
        {
            collect_dart_package_manifests(&path, output);
        }
    }
}

fn dart_yaml_scalar(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        if left.trim() != key {
            return None;
        }
        let value = right.trim().trim_matches(['"', '\'']);
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn compact_dart_synthetic_diagnostics(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut output = Vec::new();
    let mut missing_packages = HashSet::new();
    let mut suppressed = 0usize;
    for diagnostic in diagnostics {
        if let Some(package) = dart_missing_package(&diagnostic.message) {
            if missing_packages.insert(package) {
                output.push(diagnostic);
            } else {
                suppressed += 1;
            }
        } else if dart_external_cascade(&diagnostic.message) {
            suppressed += 1;
        } else {
            output.push(diagnostic);
        }
    }
    if suppressed > 0 {
        output.push(Diagnostic {
            language: "dart".to_string(),
            level: "warning",
            code: DiagnosticCode::ProviderDiagnostic,
            message: format!(
                "{suppressed} Dart provider diagnostics were collapsed because local-only package analysis cannot resolve external package symbols"
            ),
            detail: None,
            path: None,
            line: None,
        });
    }
    output
}

fn compact_large_workspace_diagnostics(
    diagnostics: Vec<Diagnostic>,
    language: &str,
) -> Vec<Diagnostic> {
    let mut seen = HashSet::new();
    let mut suppressed = 0usize;
    let mut output = Vec::new();
    for diagnostic in diagnostics {
        let key = format!(
            "{}:{}:{}",
            diagnostic.language, diagnostic.level, diagnostic.message
        );
        if seen.insert(key) {
            output.push(diagnostic);
        } else {
            suppressed += 1;
        }
    }
    if suppressed > 0 {
        output.push(Diagnostic {
            language: language.to_string(),
            level: "warning",
            code: DiagnosticCode::LargeWorkspacePartial,
            message: format!(
                "{suppressed} repeated Java provider diagnostics were collapsed for the large-workspace view"
            ),
            detail: None,
            path: None,
            line: None,
        });
    }
    output
}

fn dart_missing_package(message: &str) -> Option<String> {
    let start = message.find("package:")? + "package:".len();
    let remainder = &message[start..];
    let end = remainder.find(['/', '\'', '"']).unwrap_or(remainder.len());
    (!remainder[..end].is_empty()).then(|| remainder[..end].to_string())
}

fn dart_external_cascade(message: &str) -> bool {
    [
        "Undefined name",
        "Undefined class",
        "Undefined getter",
        "Undefined setter",
        "Undefined operator",
        "The function '",
        "The method '",
        "The name '",
        "Classes can only extend",
        "No associated named super constructor",
        "doesn't override an inherited method",
        "Method invocation or property access on a 'dynamic' target",
    ]
    .iter()
    .any(|prefix| message.contains(prefix))
}

static NEXT_LSP_ID: AtomicI64 = AtomicI64::new(1);
