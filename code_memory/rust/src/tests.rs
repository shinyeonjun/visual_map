use super::*;

#[test]
fn registry_has_exactly_twelve_languages() {
    assert_eq!(LANGUAGES.len(), 12);
    assert!(LANGUAGES.iter().any(|lang| lang.id == "typescript"));
    assert!(LANGUAGES.iter().any(|lang| lang.id == "php"));
    assert!(!LANGUAGES.iter().any(|lang| lang.id == "swift"));
}

#[test]
fn strict_gate_does_not_fail_explicit_source_exclusions() {
    let output = IndexOutput {
        schema: "test",
        project_root: String::new(),
        languages: vec![LanguageOutput {
            id: "c".to_string(),
            name: "C".to_string(),
            provider: "native-lsp",
            files_found: 1,
            files_indexed: 0,
            files_excluded: 1,
            files_missing: 0,
            status: "excluded",
        }],
        coverage: Vec::new(),
        documents: Vec::new(),
        relations: Vec::new(),
        file_relations: Vec::new(),
        project_model_files: Vec::new(),
        frameworks: Vec::new(),
        framework_relations: Vec::new(),
        diagnostics: Vec::new(),
        timings: Vec::new(),
        analysis_units: Vec::new(),
    };
    assert!(enforce_quality_gate(&output).is_ok());
}

#[test]
fn strict_gate_still_fails_provider_missing_files() {
    let output = IndexOutput {
        schema: "test",
        project_root: String::new(),
        languages: vec![LanguageOutput {
            id: "dart".to_string(),
            name: "Dart".to_string(),
            provider: "native-lsp",
            files_found: 2,
            files_indexed: 1,
            files_excluded: 0,
            files_missing: 1,
            status: "indexed-partial",
        }],
        coverage: Vec::new(),
        documents: Vec::new(),
        relations: Vec::new(),
        file_relations: Vec::new(),
        project_model_files: Vec::new(),
        frameworks: Vec::new(),
        framework_relations: Vec::new(),
        diagnostics: Vec::new(),
        timings: Vec::new(),
        analysis_units: Vec::new(),
    };
    assert!(enforce_quality_gate(&output).is_err());
}

#[test]
fn every_language_has_a_provider() {
    assert!(LANGUAGES
        .iter()
        .all(|lang| !lang.tool.is_empty() && !lang.extensions.is_empty()));
}

#[test]
fn large_rust_semantic_enrichment_stays_at_public_boundaries() {
    let public = LspSymbol {
        name: "run".to_string(),
        kind: 12,
        detail: Some("pub fn run()".to_string()),
        range_start_line: 0,
        range_start_character: 0,
        range_end_line: 0,
        range_end_character: 12,
        selection_line: 0,
        selection_character: 4,
    };
    let private = LspSymbol {
        name: "helper".to_string(),
        kind: 12,
        detail: Some("fn helper()".to_string()),
        range_start_line: 0,
        range_start_character: 0,
        range_end_line: 0,
        range_end_character: 12,
        selection_line: 0,
        selection_character: 3,
    };
    assert!(rust_large_symbol_is_public("pub fn run() {}", &public));
    assert!(!rust_large_symbol_is_public("fn helper() {}", &private));
    assert!(!rust_large_symbol_is_public(
        "    pub fn method() {}",
        &public
    ));
}

#[test]
fn large_map_enrichment_uses_language_visibility_without_guessing_targets() {
    let exported_go = LspSymbol {
        name: "Serve".to_string(),
        kind: 12,
        detail: None,
        range_start_line: 0,
        range_start_character: 0,
        range_end_line: 0,
        range_end_character: 12,
        selection_line: 0,
        selection_character: 4,
    };
    let private_go = LspSymbol {
        name: "serve".to_string(),
        kind: 12,
        detail: None,
        range_start_line: 0,
        range_start_character: 0,
        range_end_line: 0,
        range_end_character: 12,
        selection_line: 0,
        selection_character: 4,
    };
    assert!(large_symbol_is_map_boundary(
        "go",
        "func Serve() {}",
        &exported_go
    ));
    assert!(!large_symbol_is_map_boundary(
        "go",
        "func (s Server) Serve() {}",
        &exported_go
    ));
    assert!(!large_symbol_is_map_boundary(
        "go",
        "func serve() {}",
        &private_go
    ));
    assert!(large_symbol_is_map_boundary(
        "python",
        "def run():\n    pass",
        &LspSymbol {
            name: "run".to_string(),
            kind: 12,
            detail: None,
            range_start_line: 0,
            range_start_character: 0,
            range_end_line: 1,
            range_end_character: 8,
            selection_line: 0,
            selection_character: 4,
        },
    ));
}

#[test]
fn windows_file_uri_uses_a_canonical_drive_letter() {
    assert_eq!(
        path_to_uri(Path::new(r"D:\project\src\main.rs")),
        "file:///d:/project/src/main.rs"
    );
}

#[test]
fn file_coverage_keeps_missing_files_visible() {
    let root =
        std::env::temp_dir().join(format!("code-memory-file-coverage-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("create coverage root");
    let indexed = root.join("src/indexed.py");
    let missing = root.join("src/missing.py");
    fs::write(&indexed, "def indexed():\n    pass\n").expect("write indexed file");
    fs::write(&missing, "def missing():\n    pass\n").expect("write missing file");
    let documents = vec![DocumentOutput {
        language: "python".to_string(),
        path: "src/indexed.py".to_string(),
        symbols: Vec::new(),
        occurrences: Vec::new(),
    }];
    let languages = vec![LanguageOutput {
        id: "python".to_string(),
        name: "Python".to_string(),
        provider: "native-lsp",
        files_found: 2,
        files_indexed: 1,
        files_excluded: 0,
        files_missing: 1,
        status: "indexed-partial",
    }];
    let coverage = build_file_coverage(
        &root,
        &[
            ("python".to_string(), indexed),
            ("python".to_string(), missing),
        ],
        &documents,
        &languages,
        &[],
    );
    assert_eq!(coverage.len(), 2);
    assert_eq!(coverage[0].status, "indexed");
    assert_eq!(coverage[1].status, "missing");
    assert_eq!(
        coverage[1].reason.as_deref(),
        Some("not-returned-by-provider")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn merged_language_status_cannot_hide_provider_missing_files() {
    let analysis = LanguageAnalysis {
        language: LanguageOutput {
            id: "go".to_string(),
            name: "Go".to_string(),
            provider: "native-lsp",
            files_found: 10,
            files_indexed: 2,
            files_excluded: 0,
            files_missing: 8,
            status: "indexed",
        },
        documents: Vec::new(),
        relations: Vec::new(),
        diagnostics: Vec::new(),
        project_excluded_files: 0,
    };
    let (languages, _, _, _) = module_plan::merge_language_analyses(vec![analysis]);
    assert_eq!(languages[0].files_missing, 8);
    assert_eq!(languages[0].status, "indexed-partial");
}

#[test]
fn typescript_coverage_separates_project_config_from_provider_gap() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-project-config-coverage-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("create project-config root");
    let modeled = root.join("src/modeled.ts");
    let excluded = root.join("src/excluded.ts");
    fs::write(&modeled, "export const modeled = 1;\n").expect("write modeled file");
    fs::write(&excluded, "export const excluded = 1;\n").expect("write excluded file");
    let coverage = build_file_coverage(
        &root,
        &[
            ("typescript".to_string(), modeled),
            ("typescript".to_string(), excluded),
        ],
        &[],
        &[LanguageOutput {
            id: "typescript".to_string(),
            name: "TypeScript".to_string(),
            provider: "scip",
            files_found: 2,
            files_indexed: 0,
            files_excluded: 0,
            files_missing: 2,
            status: "indexed-partial",
        }],
        &["src/modeled.ts".to_string()],
    );
    assert_eq!(coverage[0].reason.as_deref(), Some("project-config"));
    assert_eq!(
        coverage[1].reason.as_deref(),
        Some("not-returned-by-provider")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn provider_root_is_checked_before_path() {
    let root =
        std::env::temp_dir().join(format!("code-memory-provider-root-{}", std::process::id()));
    let bin = root.join("scip-typescript").join("bin");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&bin).expect("create provider root");
    let executable = if cfg!(windows) {
        bin.join("scip-typescript.cmd")
    } else {
        bin.join("scip-typescript")
    };
    fs::write(&executable, b"provider").expect("write provider placeholder");
    assert_eq!(find_tool("scip-typescript", Some(&root)), Some(executable));
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(windows)]
fn windows_root_launcher_precedes_non_windows_bin_script() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-provider-launcher-order-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).expect("create launcher root");
    let root_launcher = root.join("jdtls.cmd");
    let non_windows_script = root.join("bin").join("jdtls");
    fs::write(&root_launcher, b"launcher").expect("write Windows launcher");
    fs::write(&non_windows_script, b"script").expect("write non-Windows script");
    assert_eq!(find_tool("jdtls", Some(&root)), Some(root_launcher));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn provider_manifest_resolves_a_different_directory_name() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-provider-manifest-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("jdtls/bin")).expect("create manifest provider root");
    fs::write(root.join("jdtls/bin/jdtls.cmd"), b"provider").expect("write provider");
    fs::write(
            root.join("manifest.json"),
            r#"{"schema":"code-memory.provider-manifest.v1","providers":[{"command":"jdtls","path":"jdtls/bin/jdtls.cmd"}]}"#,
        )
        .expect("write provider manifest");
    assert_eq!(
        find_tool("jdtls", Some(&root)),
        Some(root.join("jdtls/bin/jdtls.cmd"))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn managed_provider_root_is_executable_from_a_changed_working_directory() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-provider-canonical-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create canonical provider root");
    let managed = managed_provider_root(&root);
    assert!(managed.is_absolute());
    assert!(!managed.to_string_lossy().starts_with("\\\\?\\"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_ranges_support_scip_short_and_lsp_long_forms() {
    assert!(range_contains(&[2, 0, 4, 0], &[3, 2, 5]));
    assert!(range_contains(&[2, 0, 2, 10], &[2, 2, 2, 5]));
    assert!(!range_contains(&[2, 0, 2, 10], &[1, 2, 1, 5]));
}

#[test]
fn scip_absolute_document_paths_are_project_relative() {
    let root = std::env::temp_dir().join(format!("code-memory-scip-path-{}", std::process::id()));
    let file = root.join("src").join("fixture.php");
    fs::create_dir_all(file.parent().unwrap()).expect("create SCIP path fixture");
    fs::write(&file, "<?php").expect("write SCIP path fixture");
    let raw = file.to_string_lossy().replace('\\', "/");
    assert_eq!(normalize_scip_path(&raw, &root), "src/fixture.php");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_call_detection_uses_source_range() {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("scip-typescript");
    let source = fs::read_to_string(fixture_root.join("src/main.ts")).unwrap();
    assert!(is_call_occurrence(Some(&source), &[8, 9, 12]));
}

#[test]
fn scip_numeric_language_uses_worker_language() {
    assert_eq!(normalize_scip_language("19", "php"), "php");
    assert_eq!(normalize_scip_language("", "rust"), "rust");
    assert_eq!(
        normalize_scip_language("typescript", "javascript"),
        "typescript"
    );
}

#[test]
fn lexical_call_candidates_scan_identifiers_once() {
    let symbols = vec![LspSymbol {
        name: "defined".to_string(),
        kind: 12,
        detail: None,
        range_start_line: 0,
        range_start_character: 0,
        range_end_line: 0,
        range_end_character: 7,
        selection_line: 0,
        selection_character: 0,
    }];
    let candidates = lexical_call_candidates(
        "defined()\n  defined(); unrelated();",
        &symbols,
        &["defined".to_string()],
    );
    assert_eq!(candidates, vec![(1, 2, "defined".to_string())]);
}

#[test]
fn allowed_document_paths_are_project_relative() {
    let root =
        std::env::temp_dir().join(format!("code-memory-allowed-paths-{}", std::process::id()));
    let file = root.join("src").join("main.ts");
    fs::create_dir_all(file.parent().unwrap()).expect("create allowed path fixture");
    let allowed = allowed_document_paths(&root, std::slice::from_ref(&file));
    assert!(allowed.contains("src/main.ts"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn language_cache_key_changes_when_one_source_file_changes() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-file-fingerprint-{}",
        std::process::id()
    ));
    let file = root.join("src").join("main.ts");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(file.parent().unwrap()).expect("create fingerprint fixture");
    fs::write(&file, "export const value = 1;\n").expect("write initial fingerprint fixture");
    let files = vec![file.clone()];
    let first_snapshot = load_source_snapshot_from_files(&root, &files);
    let language = LANGUAGES
        .iter()
        .find(|language| language.id == "typescript")
        .copied()
        .expect("typescript language");
    let first = language_cache_key(&root, &language, &files, None, 0, &first_snapshot);
    fs::write(&file, "export const value = 2;\n").expect("write changed fingerprint fixture");
    let second_snapshot = load_source_snapshot_from_files(&root, &files);
    let second = language_cache_key(&root, &language, &files, None, 0, &second_snapshot);
    assert_ne!(first, second);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn source_snapshot_loads_contents_only_when_requested() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-lazy-source-snapshot-{}",
        std::process::id()
    ));
    let file = root.join("src").join("main.ts");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(file.parent().unwrap()).expect("create lazy snapshot fixture");
    fs::write(&file, "export const value = 1;\n").expect("write lazy snapshot fixture");
    let mut snapshot = load_source_snapshot_metadata_from_files(&root, std::slice::from_ref(&file));
    assert!(snapshot.files.is_empty());
    assert!(snapshot.file_hashes.contains_key("src/main.ts"));
    load_source_contents(&root, &mut snapshot);
    assert_eq!(
        snapshot.files,
        vec![(
            "src/main.ts".to_string(),
            "export const value = 1;\n".to_string()
        )]
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cache_impact_includes_importers_of_changed_files() {
    let root =
        std::env::temp_dir().join(format!("code-memory-cache-impact-{}", std::process::id()));
    let importer = root.join("src").join("main.ts");
    let dependency = root.join("src").join("types.ts");
    let output = root.join("previous.json");
    let architecture = root.join("previous.architecture.json");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(importer.parent().unwrap()).expect("create impact fixture");
    fs::write(&importer, "import { Value } from './types';\n").expect("write importer");
    fs::write(&dependency, "export type Value = string;\n").expect("write dependency");
    let first = load_source_snapshot_from_files(&root, &[importer.clone(), dependency.clone()]);
    write_source_manifest(&root, &first).expect("write source manifest");
    fs::write(&dependency, "export type Value = number;\n").expect("change dependency");
    let current = load_source_snapshot_metadata_from_files(&root, &[importer, dependency]);
    fs::write(
        &output,
        serde_json::json!({
            "file_relations": [{
                "from": "src/main.ts",
                "to": "src/types.ts",
                "kind": "IMPORTS",
                "path": "src/main.ts",
                "range": [],
                "properties": {}
            }]
        })
        .to_string(),
    )
    .expect("write previous output");
    fs::write(&architecture, "{\"edges\":[]}").expect("write previous architecture");
    let impact = cache_impact(&root, &output, &architecture, &current);
    assert!(!impact.force_all);
    assert!(impact.affected_paths.contains("src/types.ts"));
    assert!(impact.affected_paths.contains("src/main.ts"));
    let _ = fs::remove_dir_all(project_cache_root(&root));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn language_cache_path_isolated_per_module_key() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-module-cache-path-{}",
        std::process::id()
    ));
    let language = LANGUAGES
        .iter()
        .find(|language| language.id == "dart")
        .copied()
        .expect("dart language");
    let first = crate::cache::language_cache_path(&root, &language, "chunk-0");
    let second = crate::cache::language_cache_path(&root, &language, "chunk-1");
    assert_ne!(first, second);
}

#[test]
fn large_source_snapshot_keeps_the_path_without_loading_content() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-large-source-snapshot-{}",
        std::process::id()
    ));
    let file = root.join("src").join("generated.dart");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(file.parent().unwrap()).expect("create large snapshot fixture");
    fs::write(&file, vec![b'x'; 1_000_001]).expect("write large snapshot fixture");
    let snapshot = load_source_snapshot_from_files(&root, std::slice::from_ref(&file));
    assert_eq!(
        snapshot.files,
        vec![("src/generated.dart".to_string(), String::new())]
    );
    assert!(snapshot.file_hashes.contains_key("src/generated.dart"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn incomplete_provider_documents_are_marked_partial() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-partial-documents-{}",
        std::process::id()
    ));
    let first = root.join("src").join("first.ts");
    let second = root.join("src").join("second.ts");
    fs::create_dir_all(first.parent().unwrap()).expect("create partial fixture");
    fs::write(&first, "export function first() {}").expect("write first fixture");
    fs::write(&second, "export function second() {}").expect("write second fixture");
    let documents = vec![DocumentOutput {
        language: "typescript".to_string(),
        path: "src/first.ts".to_string(),
        symbols: Vec::new(),
        occurrences: Vec::new(),
    }];
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "typescript")
        .copied()
        .expect("typescript language");
    let (status, diagnostics) =
        classify_language_documents(&root, &lang, &[first, second], &documents);
    assert_eq!(status, "indexed-partial");
    assert_eq!(diagnostics.len(), 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn go_build_constraint_is_not_counted_as_provider_missing() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-go-build-constraint-{}",
        std::process::id()
    ));
    let file = root.join("tools.go");
    fs::create_dir_all(&root).expect("create go constraint fixture");
    fs::write(&file, "//go:build tools\n\npackage tools\n").expect("write go constraint fixture");
    assert_eq!(source_exclusion_reason(&file), Some("go-build-constraint"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn go_package_marker_is_not_counted_as_provider_missing() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-go-package-marker-{}",
        std::process::id()
    ));
    let file = root.join("doc.go");
    fs::create_dir_all(&root).expect("create go package marker fixture");
    fs::write(&file, "// package documentation\n\npackage tools\n")
        .expect("write go package marker fixture");
    assert_eq!(source_exclusion_reason(&file), Some("go-package-marker"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clangd_header_extensions_are_recognized() {
    assert!(is_cpp_header(Path::new("include/api.hpp")));
    assert!(is_cpp_header(Path::new("include/api.h")));
    assert!(is_cpp_header_fragment(Path::new("include/template.tpp")));
    assert!(!is_cpp_header(Path::new("src/api.cpp")));
}

#[test]
fn compile_database_can_live_in_a_build_directory() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-compile-database-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let build = root.join("cmake-build-debug");
    fs::create_dir_all(&build).expect("create compile database fixture");
    fs::write(build.join("compile_commands.json"), "[]").expect("write compile database");

    assert_eq!(
        compile_database_dir(&root),
        Some(build.canonicalize().unwrap())
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn nested_module_finds_ancestor_build_directory() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-nested-compile-database-{}",
        std::process::id()
    ));
    let module = root.join("lib");
    let build = root.join("build");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&module).expect("create nested module");
    fs::create_dir_all(&build).expect("create ancestor build");
    fs::write(build.join("compile_commands.json"), "[]").expect("write compile database");

    assert_eq!(
        compile_database_dir(&module),
        Some(build.canonicalize().unwrap())
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clangd_project_context_can_come_from_clangd_config() {
    let root =
        std::env::temp_dir().join(format!("code-memory-clangd-context-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create clangd context fixture");
    fs::write(root.join(".clangd"), "CompileFlags:\n  Add: [-DTEST]\n")
        .expect("write clangd config");

    assert!(has_compile_context(&root));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_database_context_can_cover_a_partial_file_set() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-compile-coverage-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create compile coverage fixture");
    let source = root.join("main.c");
    let missing = root.join("missing.c");
    fs::write(&source, "int main(void) { return 0; }\n").expect("write source");
    fs::write(
        root.join("compile_commands.json"),
        format!(
            "[{{\"directory\":\"{}\",\"command\":\"clang -c main.c\",\"file\":\"{}\"}}]",
            root.display().to_string().replace('\\', "/"),
            source.display().to_string().replace('\\', "/")
        ),
    )
    .expect("write compile database");

    assert!(has_compile_context_for_files(
        &root,
        std::slice::from_ref(&source)
    ));
    assert!(has_compile_context_for_files(&root, &[source, missing]));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn nested_lsp_workspace_is_selected_only_for_one_module() {
    let root =
        std::env::temp_dir().join(format!("code-memory-workspace-root-{}", std::process::id()));
    let module = root.join("module");
    let file = module.join("main.go");
    fs::create_dir_all(&module).expect("create workspace fixture");
    fs::write(module.join("go.mod"), "module example.com/test\n").expect("write go.mod fixture");
    fs::write(&file, "package main\n").expect("write go fixture");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "go")
        .copied()
        .expect("go language");
    assert_eq!(
        lsp_workspace_root(&lang, &root, std::slice::from_ref(&file)),
        module
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn go_module_uses_ancestor_go_workspace_when_present() {
    let root =
        std::env::temp_dir().join(format!("code-memory-go-workspace-{}", std::process::id()));
    let module = root.join("services").join("api");
    let file = module.join("main.go");
    fs::create_dir_all(&module).expect("create go workspace fixture");
    fs::write(root.join("go.work"), "go 1.22\nuse ./services/api\n")
        .expect("write go.work fixture");
    fs::write(module.join("go.mod"), "module example.com/api\n").expect("write go.mod fixture");
    fs::write(&file, "package api\n").expect("write go fixture");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "go")
        .copied()
        .expect("go language");
    assert_eq!(
        lsp_workspace_root(&lang, &module, std::slice::from_ref(&file)),
        root
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn java_module_prefers_its_explicit_package_root() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-java-workspace-root-{}",
        std::process::id()
    ));
    let module = root.join("services").join("api");
    let file = module.join("src/main/java/Main.java");
    fs::create_dir_all(file.parent().unwrap()).expect("create java workspace fixture");
    fs::write(
        root.join("pom.xml"),
        "<project><modules><module>services/api</module></modules></project>",
    )
    .expect("write maven reactor fixture");
    fs::write(module.join("pom.xml"), "<project></project>").expect("write module pom");
    fs::write(&file, "class Main {}\n").expect("write java fixture");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "java")
        .copied()
        .expect("java language");
    assert_eq!(
        lsp_workspace_root(&lang, &module, std::slice::from_ref(&file)),
        module
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn multiple_nested_lsp_modules_keep_caller_root() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-workspace-multi-{}",
        std::process::id()
    ));
    let first = root.join("first");
    let second = root.join("second");
    let first_file = first.join("main.go");
    let second_file = second.join("main.go");
    fs::create_dir_all(&first).expect("create first module");
    fs::create_dir_all(&second).expect("create second module");
    fs::write(first.join("go.mod"), "module example.com/first\n").expect("write first go.mod");
    fs::write(second.join("go.mod"), "module example.com/second\n").expect("write second go.mod");
    fs::write(&first_file, "package main\n").expect("write first go fixture");
    fs::write(&second_file, "package main\n").expect("write second go fixture");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "go")
        .copied()
        .expect("go language");
    assert_eq!(
        lsp_workspace_root(&lang, &root, &[first_file, second_file]),
        root
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn module_planner_assigns_files_to_deepest_project_module() {
    let root = std::env::temp_dir().join(format!("code-memory-module-plan-{}", std::process::id()));
    let nested = root.join("services").join("api");
    fs::create_dir_all(&nested).expect("create module fixture");
    fs::write(root.join("go.work"), "go 1.22\n").expect("write workspace marker");
    fs::write(nested.join("go.mod"), "module example.com/api\n").expect("write module marker");
    let root_file = root.join("main.go");
    let nested_file = nested.join("main.go");
    fs::write(&root_file, "package main\n").expect("write root source");
    fs::write(&nested_file, "package api\n").expect("write nested source");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "go")
        .copied()
        .expect("go language");
    let modules = plan_language_modules(&root, lang, &[root_file, nested_file.clone()]);
    assert_eq!(modules.len(), 2);
    assert!(modules
        .iter()
        .any(|module| module.files == vec![nested_file.clone()]));
    assert!(modules.iter().any(|module| module.id == "root"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rust_workspace_uses_one_provider_module() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-rust-workspace-plan-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("crates/app/src")).expect("create rust workspace");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/app\"]\n",
    )
    .expect("write workspace manifest");
    let file = root.join("crates/app/src/main.rs");
    fs::write(&file, "fn main() {}\n").expect("write rust source");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "rust")
        .copied()
        .unwrap();
    let modules = module_plan::plan_language_modules(&root, lang, std::slice::from_ref(&file));
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id, "root");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn java_maven_reactor_uses_one_provider_module() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-java-reactor-plan-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("service/src")).expect("create java reactor");
    fs::write(
        root.join("pom.xml"),
        "<project><modules><module>service</module></modules></project>",
    )
    .expect("write reactor manifest");
    let file = root.join("service/src/Main.java");
    fs::write(&file, "class Main {}\n").expect("write java source");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "java")
        .copied()
        .unwrap();
    let modules = module_plan::plan_language_modules(&root, lang, std::slice::from_ref(&file));
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].id, "root");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dart_workspace_uses_one_provider_module() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-dart-workspace-plan-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("packages/app/lib")).expect("create dart workspace");
    fs::write(
        root.join("pubspec.yaml"),
        "name: workspace\nworkspace:\n  - packages/app\n",
    )
    .expect("write dart workspace manifest");
    let file = root.join("packages/app/lib/main.dart");
    fs::write(&file, "void main() {}\n").expect("write dart source");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "dart")
        .expect("dart language");

    let modules = plan_language_modules(&root, *lang, std::slice::from_ref(&file));
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].root, root);
    assert_eq!(modules[0].files, vec![file]);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn dart_melos_workspace_keeps_package_lsp_root() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-dart-melos-root-{}",
        std::process::id()
    ));
    let module = root.join("packages/app");
    let file = module.join("lib/main.dart");
    fs::create_dir_all(file.parent().expect("Dart file parent")).expect("create Dart module");
    fs::write(
        root.join("melos.yaml"),
        "name: workspace\npackages:\n  - packages/**\n",
    )
    .expect("write melos manifest");
    fs::write(module.join("pubspec.yaml"), "name: app\n").expect("write package manifest");
    fs::write(&file, "void main() {}\n").expect("write Dart file");
    let lang = LANGUAGES
        .iter()
        .find(|lang| lang.id == "dart")
        .copied()
        .expect("Dart language");
    assert_eq!(
        lsp_workspace_root(&lang, &module, std::slice::from_ref(&file)),
        module
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn large_dart_package_is_split_without_changing_workspace_root() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-dart-large-plan-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create large Dart plan root");
    fs::write(root.join("pubspec.yaml"), "name: sample\n").expect("write Dart manifest");
    let lang = *LANGUAGES
        .iter()
        .find(|lang| lang.id == "dart")
        .expect("Dart language");
    let files = (0..513)
        .map(|index| root.join(format!("lib/file_{index}.dart")))
        .collect::<Vec<_>>();
    let modules = plan_language_modules(&root, lang, &files);
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[0].files.len(), 512);
    assert_eq!(modules[1].files.len(), 1);
    assert!(modules.iter().all(|module| module.root == root));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dart_dependency_gap_is_reported_before_server_start() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-dart-dependency-gap-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create dart dependency fixture");
    fs::write(
        root.join("pubspec.yaml"),
        "name: flutter_app\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .expect("write dart dependency manifest");
    let gap =
        dart_dependency_metadata_gap(&root).expect("missing package config should be reported");
    assert!(gap.contains("package_config.json"));
    fs::create_dir_all(root.join(".dart_tool")).expect("create dart metadata directory");
    fs::write(root.join(".dart_tool/package_config.json"), "{}\n")
        .expect("write dart package config");
    assert!(dart_dependency_metadata_gap(&root).is_none());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn dart_plain_package_dependencies_also_require_resolved_metadata() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-dart-plain-dependency-gap-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create dart dependency fixture");
    fs::write(
        root.join("pubspec.yaml"),
        "name: sample\ndependencies:\n  http: ^1.0.0\n",
    )
    .expect("write dart dependency manifest");
    assert!(dart_dependency_metadata_gap(&root).is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn typescript_config_discovery_includes_nested_and_named_configs() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-typescript-configs-{}",
        std::process::id()
    ));
    let nested = root.join("packages").join("web");
    fs::create_dir_all(&nested).expect("create config fixture");
    fs::write(root.join("tsconfig.json"), "{}").expect("write root tsconfig");
    fs::write(root.join("tsconfig.build.json"), "{}").expect("write named tsconfig");
    fs::write(nested.join("jsconfig.json"), "{}").expect("write nested jsconfig");
    let test_config = root.join("tests").join("tsconfig.test.json");
    fs::create_dir_all(test_config.parent().unwrap()).expect("create test config directory");
    fs::write(&test_config, "{}").expect("write test config");
    fs::create_dir_all(root.join("node_modules")).expect("create excluded directory");
    fs::write(root.join("node_modules").join("tsconfig.json"), "{}")
        .expect("write excluded config");

    let configs = typescript_config_files(&root);
    assert_eq!(configs.len(), 4);
    assert!(configs
        .iter()
        .all(|path| !path.starts_with(root.join("node_modules"))));
    assert!(configs
        .iter()
        .any(|path| path.ends_with("tsconfig.build.json")));
    assert!(configs.iter().any(|path| path.ends_with("jsconfig.json")));
    assert!(configs
        .iter()
        .any(|path| path.ends_with("tsconfig.test.json")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn only_known_rust_file_watch_warning_is_filtered() {
    let warning = "WARN notify error: Input watch path is neither a file nor a directory.";
    assert!(is_benign_provider_stderr("rust-analyzer", warning));
    assert!(!is_benign_provider_stderr("gopls", warning));
    assert!(!is_benign_provider_stderr(
        "rust-analyzer",
        "error: failed to load Cargo workspace"
    ));
}

#[test]
fn clangd_internal_logs_are_filtered_but_errors_are_not() {
    assert!(is_benign_provider_stderr(
        "clangd",
        "I[12:00:00] AST worker"
    ));
    assert!(is_benign_provider_stderr(
        "clangd",
        "I[12:00:00] Found definition heuristically using nearby identifier foo"
    ));
    assert!(is_benign_provider_stderr("clangd", "argv[0]: clangd"));
    assert!(!is_benign_provider_stderr(
        "clangd",
        "E[12:00:00] IncludeCleaner: missing header"
    ));
    assert!(!is_benign_provider_stderr(
        "scip-typescript",
        "error: no files got indexed"
    ));
}

#[test]
fn framework_symbol_short_name_handles_scip_paths() {
    assert_eq!(
        frameworks::symbol_short_name("project/src/`app.ts`/health()."),
        "health"
    );
}
