use super::*;
use crate::{
    Diagnostic, DiagnosticCode, DocumentOutput, FileRelationOutput, IndexOutput, LanguageOutput,
    OccurrenceOutput, RelationOutput, SymbolOutput,
};
use std::collections::BTreeMap;
use std::fs;

fn sample_output() -> IndexOutput {
    IndexOutput {
        schema: "code-memory.language-index.v2",
        project_root: "fixture".to_string(),
        provider_provenance: Vec::new(),
        languages: vec![LanguageOutput {
            id: "python".to_string(),
            name: "Python".to_string(),
            provider: "native-lsp",
            files_found: 1,
            files_indexed: 1,
            files_excluded: 0,
            files_missing: 0,
            status: "indexed",
        }],
        coverage: Vec::new(),
        documents: vec![DocumentOutput {
            language: "python".to_string(),
            path: "app/routes.py".to_string(),
            symbols: vec![],
            occurrences: vec![OccurrenceOutput {
                symbol: "fixture/handler().".to_string(),
                range: vec![1, 0, 1, 7],
                enclosing_range: Vec::new(),
                definition: true,
                import: false,
                read: false,
                write: false,
            }],
        }],
        relations: vec![RelationOutput {
            from: "fixture/handler().".to_string(),
            to: "fixture/service().".to_string(),
            kind: "CALLS".to_string(),
            path: "app/routes.py".to_string(),
            range: vec![2, 4, 2, 12],
            confidence: Some(1.0),
            strategy: Some("provider-symbol-resolution".to_string()),
        }],
        file_relations: Vec::new(),
        project_model_files: Vec::new(),
        frameworks: Vec::new(),
        framework_relations: Vec::new(),
        diagnostics: Vec::new(),
        timings: Vec::new(),
        analysis_units: Vec::new(),
    }
}

#[test]
fn excluded_language_reason_is_stable_for_ui_consumers() {
    let diagnostics = vec![Diagnostic {
        language: "php".to_string(),
        level: "warning",
        code: DiagnosticCode::MissingDependencyMetadata,
        message:
            "PHP semantic analysis skipped because Composer dependency metadata is unavailable"
                .to_string(),
        detail: None,
        path: None,
        line: None,
    }];
    assert_eq!(
        language_exclusion_reason("php", "excluded", &diagnostics).as_deref(),
        Some("missing-dependency")
    );
    assert_eq!(
        language_exclusion_reason("php", "indexed", &diagnostics),
        None
    );
}

#[test]
fn diagnostic_code_does_not_depend_on_human_message_wording() {
    let diagnostics = vec![Diagnostic {
        language: "php".to_string(),
        level: "warning",
        code: DiagnosticCode::MissingDependencyMetadata,
        message: "provider wording changed in a future release".to_string(),
        detail: None,
        path: None,
        line: None,
    }];

    assert_eq!(
        language_exclusion_reason("php", "excluded", &diagnostics).as_deref(),
        Some("missing-dependency")
    );
}

#[test]
fn architecture_exposes_language_and_framework_quality_summary() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-quality-summary-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let mut output = sample_output();
    output.frameworks.push(crate::frameworks::FrameworkOutput {
        id: "fastapi".to_string(),
        language: "python".to_string(),
        name: "FastAPI".to_string(),
        kind: "web".to_string(),
        adapter: "registration-routing".to_string(),
        status: "detected".to_string(),
        matched_signals: vec!["app.get".to_string()],
        files: vec!["app/routes.py".to_string()],
        facts: vec![crate::frameworks::FrameworkFact {
            id: "route:fastapi:app/routes.py:1:/health".to_string(),
            kind: "HTTP_ROUTE".to_string(),
            framework: "fastapi".to_string(),
            symbol: None,
            method: Some("GET".to_string()),
            path: Some("/health".to_string()),
            source_file: "app/routes.py".to_string(),
            source_line: 1,
            source_end_line: 1,
            source_range: vec![0, 0, 0, 20],
            evidence: vec!["app.get".to_string()],
            properties: BTreeMap::new(),
        }],
    });

    let architecture = build(&root, &output);

    assert_eq!(architecture.schema, "code-memory.architecture-index.v3");
    assert_eq!(architecture.languages[0].id, "python");
    assert_eq!(architecture.languages[0].status, "indexed");
    assert_eq!(architecture.frameworks[0].id, "fastapi");
    assert_eq!(architecture.frameworks[0].fact_count, 1);
    assert_eq!(architecture.frameworks[0].relation_count, 0);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unresolved_framework_route_is_kept_as_an_endpoint() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-unresolved-route-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("app/routes.py"),
        "@app.get(\"/health\")\ndef health():\n    return {}\n",
    )
    .unwrap();

    let mut output = sample_output();
    output.frameworks.push(crate::frameworks::FrameworkOutput {
        id: "fastapi".to_string(),
        language: "python".to_string(),
        name: "FastAPI".to_string(),
        kind: "web".to_string(),
        adapter: "registration-routing".to_string(),
        status: "detected".to_string(),
        matched_signals: vec!["app.get".to_string()],
        files: vec!["app/routes.py".to_string()],
        facts: vec![crate::frameworks::FrameworkFact {
            id: "route:fastapi:app/routes.py:1:/health".to_string(),
            kind: "HTTP_ROUTE".to_string(),
            framework: "fastapi".to_string(),
            symbol: None,
            method: Some("GET".to_string()),
            path: Some("/health".to_string()),
            source_file: "app/routes.py".to_string(),
            source_line: 1,
            source_end_line: 1,
            source_range: vec![0, 0, 0, 24],
            evidence: vec!["test".to_string()],
            properties: BTreeMap::new(),
        }],
    });

    let result = build(&root, &output);
    let endpoint = result
        .nodes
        .iter()
        .find(|node| node.kind == "ENDPOINT")
        .expect("route endpoint");
    assert_eq!(endpoint.name, "GET /health");
    assert_eq!(endpoint.label, "fastapi: GET /health");
    assert_eq!(
        endpoint.properties.get("method").map(String::as_str),
        Some("GET")
    );
    assert_eq!(
        endpoint.properties.get("routeMethod").map(String::as_str),
        Some("GET")
    );
    assert_eq!(
        endpoint.properties.get("routePath").map(String::as_str),
        Some("/health")
    );
    assert_eq!(
        endpoint
            .properties
            .get("handler_resolution")
            .map(String::as_str),
        Some("unresolved")
    );
    assert_eq!(
        endpoint
            .properties
            .get("runtime_reachability")
            .map(String::as_str),
        Some("not-assessed")
    );
    assert!(!result.edges.iter().any(|edge| edge.kind == "ENTRYPOINT_TO"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn go_import_parser_ignores_quoted_strings() {
    let imports = parse_imports(
        "client/pkg/testutil/leak.go",
        "go",
        "package testutil\n\nimport (\n\t\"context\"\n\talias \"example.com/alias\"\n)\n\nvar messages = []string{\"created by testing.RunTests\"}\n",
    );
    let packages: Vec<_> = imports.into_iter().map(|import| import.package).collect();
    assert_eq!(packages, vec!["context", "example.com/alias"]);
}

#[test]
fn external_import_becomes_a_library_boundary() {
    let root =
        std::env::temp_dir().join(format!("code-memory-architecture-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("app/routes.py"),
        "import pandas as pd\ndf = pd.read_csv('x.csv')\n",
    )
    .unwrap();
    let output = sample_output();
    let result = build(&root, &output);
    assert!(result.nodes.iter().any(|node| {
        node.id == "external:pypi:pandas"
            && node.label == "pandas 라이브러리"
            && node.name == "pandas"
    }));
    assert!(result
        .edges
        .iter()
        .any(|edge| edge.kind == "USES_LIBRARY" && edge.to == "external:pypi:pandas"));
    assert!(!result.edges.iter().any(|edge| {
        edge.to == "external:pypi:pandas" && edge.properties.contains_key("operation")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn resolved_project_import_becomes_an_internal_module_edge() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-project-import-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app/routes")).unwrap();
    fs::create_dir_all(root.join("app/service")).unwrap();
    fs::write(
        root.join("app/routes/routes.py"),
        "from app.service import run\n",
    )
    .unwrap();
    fs::write(
        root.join("app/routes/helper.py"),
        "def route_helper(): pass\n",
    )
    .unwrap();
    fs::write(root.join("app/service/service.py"), "def run(): pass\n").unwrap();
    fs::write(
        root.join("app/service/helper.py"),
        "def service_helper(): pass\n",
    )
    .unwrap();
    let mut output = sample_output();
    output.file_relations.push(FileRelationOutput {
        from: "app/routes/routes.py".to_string(),
        to: "app/service/service.py".to_string(),
        kind: "IMPORTS".to_string(),
        path: "app/routes/routes.py".to_string(),
        range: vec![0, 0, 0, 10],
        properties: std::collections::BTreeMap::from([
            ("resolution".to_string(), "internal".to_string()),
            (
                "source".to_string(),
                "typescript-module-resolution".to_string(),
            ),
        ]),
    });
    let result = build(&root, &output);
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from.contains("app")
            && edge.to.contains("app")
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cross_file_provider_call_is_visible_below_the_module_overview() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-file-flow-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(root.join("app/main.py"), "def main(): return service()\n").unwrap();
    fs::write(root.join("app/service.py"), "def service(): return 1\n").unwrap();
    let mut output = sample_output();
    output.documents = vec![
        DocumentOutput {
            language: "python".to_string(),
            path: "app/main.py".to_string(),
            symbols: vec![SymbolOutput {
                symbol: "lsp . . . app.main.py#main@1:4".to_string(),
                kind: "Function".to_string(),
                display_name: None,
                documentation: Vec::new(),
                signature: None,
                enclosing_symbol: None,
            }],
            occurrences: Vec::new(),
        },
        DocumentOutput {
            language: "python".to_string(),
            path: "app/service.py".to_string(),
            symbols: vec![SymbolOutput {
                symbol: "lsp . . . app.service.py#service@1:4".to_string(),
                kind: "Function".to_string(),
                display_name: None,
                documentation: Vec::new(),
                signature: None,
                enclosing_symbol: None,
            }],
            occurrences: Vec::new(),
        },
    ];
    output.relations = vec![RelationOutput {
        from: "lsp . . . app.main.py#main@1:4".to_string(),
        to: "lsp . . . app.service.py#service@1:4".to_string(),
        kind: "CALLS".to_string(),
        path: "app/main.py".to_string(),
        range: vec![0, 20, 0, 27],
        confidence: Some(1.0),
        strategy: Some("provider-symbol-resolution".to_string()),
    }];
    let result = build(&root, &output);
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "CALLS" && edge.from == "file:app/main.py" && edge.to == "file:app/service.py"
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn python_project_import_becomes_a_file_import_edge() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-python-import-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("app/main.py"),
        "from app.service import run\nrun()\n",
    )
    .unwrap();
    fs::write(root.join("app/service.py"), "def run(): return 1\n").unwrap();
    let result = build(
        &root,
        &IndexOutput {
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from == "file:app/main.py"
            && edge.to == "file:app/service.py"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn python_from_package_submodule_resolves_only_when_file_exists() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-python-submodule-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("main.py"),
        "from app import service\nservice.run()\n",
    )
    .unwrap();
    fs::write(root.join("app/service.py"), "def run(): return 1\n").unwrap();

    let result = build(
        &root,
        &IndexOutput {
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from == "file:main.py"
            && edge.to == "file:app/service.py"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn ambiguous_import_suffix_stays_external() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-ambiguous-import-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("one")).unwrap();
    fs::create_dir_all(root.join("two")).unwrap();
    fs::write(root.join("main.py"), "import service\nservice.run()\n").unwrap();
    fs::write(root.join("one/service.py"), "def run(): return 1\n").unwrap();
    fs::write(root.join("two/service.py"), "def run(): return 2\n").unwrap();

    let result = build(
        &root,
        &IndexOutput {
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(!result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rust_use_crate_path_resolves_project_module() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-rust-use-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "use crate::service::run;\n").unwrap();
    fs::write(root.join("src/service.rs"), "pub fn run() {}\n").unwrap();

    let result = build(
        &root,
        &IndexOutput {
            languages: vec![LanguageOutput {
                id: "rust".to_string(),
                name: "Rust".to_string(),
                provider: "native-lsp",
                files_found: 2,
                files_indexed: 0,
                files_excluded: 0,
                files_missing: 0,
                status: "indexed",
            }],
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from == "file:src/main.rs"
            && edge.to == "file:src/service.rs"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rust_use_local_module_without_crate_prefix_resolves_project_module() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-rust-local-use-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "use service::run;\n").unwrap();
    fs::write(root.join("src/service.rs"), "pub fn run() {}\n").unwrap();

    let result = build(
        &root,
        &IndexOutput {
            languages: vec![LanguageOutput {
                id: "rust".to_string(),
                name: "Rust".to_string(),
                provider: "native-lsp",
                files_found: 2,
                files_indexed: 0,
                files_excluded: 0,
                files_missing: 0,
                status: "indexed",
            }],
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from == "file:src/main.rs"
            && edge.to == "file:src/service.rs"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rust_mod_declaration_resolves_project_module() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-rust-mod-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "mod service;\n").unwrap();
    fs::write(root.join("src/service.rs"), "pub fn run() {}\n").unwrap();

    let result = build(
        &root,
        &IndexOutput {
            languages: vec![LanguageOutput {
                id: "rust".to_string(),
                name: "Rust".to_string(),
                provider: "native-lsp",
                files_found: 2,
                files_indexed: 0,
                files_excluded: 0,
                files_missing: 0,
                status: "indexed",
            }],
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from == "file:src/main.rs"
            && edge.to == "file:src/service.rs"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn project_metadata_recognizes_python_and_gradle_manifests() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-metadata-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("python")).unwrap();
    fs::create_dir_all(root.join("java")).unwrap();
    fs::write(
        root.join("python/setup.cfg"),
        "[metadata]\nname = sample-python\nversion = 1.2.3\n",
    )
    .unwrap();
    fs::write(
        root.join("java/build.gradle"),
        "rootProject.name = 'sample-java'\nversion = '2.0.0'\n",
    )
    .unwrap();
    let packages = load_packages(&root);
    assert!(packages.iter().any(|package| {
        package.ecosystem == "pypi"
            && package.name == "sample-python"
            && package.version.as_deref() == Some("1.2.3")
    }));
    assert!(packages.iter().any(|package| {
        package.ecosystem == "gradle"
            && package.name == "sample-java"
            && package.version.as_deref() == Some("2.0.0")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn php_namespace_import_resolves_psr4_style_project_file() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-php-namespace-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/Service")).unwrap();
    fs::write(
        root.join("routes.php"),
        "<?php\nuse App\\Service\\UserService;\n",
    )
    .unwrap();
    fs::write(
        root.join("src/Service/UserService.php"),
        "<?php\nnamespace App\\Service;\nclass UserService {}\n",
    )
    .unwrap();
    let result = build(
        &root,
        &IndexOutput {
            languages: vec![],
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "IMPORTS"
            && edge.from == "file:routes.php"
            && edge.to == "file:src/Service/UserService.php"
            && edge.properties.get("resolution").map(String::as_str) == Some("internal")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn c_headers_use_external_boundaries_but_local_headers_do_not() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-c-headers-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.c"),
        "#include <stdio.h>\n#include \"local.h\"\nint main(void) { return 0; }\n",
    )
    .unwrap();
    fs::write(root.join("src/local.h"), "int local(void);\n").unwrap();

    let result = build(
        &root,
        &IndexOutput {
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result
        .nodes
        .iter()
        .any(|node| node.id == "external:system:stdio.h"));
    assert!(!result
        .nodes
        .iter()
        .any(|node| node.id == "external:system:local.h"));
    assert!(result.edges.iter().any(|edge| {
        edge.kind == "USES_LIBRARY"
            && edge.to == "external:system:stdio.h"
            && edge.properties.get("resolution").map(String::as_str) == Some("external")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn explicit_dynamic_calls_become_runtime_boundaries() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-dynamic-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("app/service.py"),
        "def run(service, name):\n    return getattr(service, name)()\n",
    )
    .unwrap();
    let result = build(
        &root,
        &IndexOutput {
            documents: Vec::new(),
            ..sample_output()
        },
    );
    assert!(result.nodes.iter().any(|node| {
        node.kind == "DYNAMIC_BOUNDARY"
            && node.properties.get("marker").map(String::as_str) == Some("getattr(")
    }));
    assert!(result.edges.iter().any(|edge| edge.kind == "DYNAMIC_CALL"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn output_is_deterministic_and_has_a_tree() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-tree-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(root.join("app/routes.py"), "def handler():\n    return 1\n").unwrap();
    fs::write(root.join("app/__init__.py"), "").unwrap();
    let output = sample_output();
    let first = serde_json::to_vec(&build(&root, &output)).unwrap();
    let second = serde_json::to_vec(&build(&root, &output)).unwrap();
    assert_eq!(first, second);
    let result = build(&root, &output);
    assert!(result.edges.iter().any(|edge| edge.kind == "CONTAINS"));
    assert_eq!(
        result
            .nodes
            .iter()
            .find(|node| node.path.as_deref() == Some("app/__init__.py"))
            .and_then(|node| node.properties.get("semantic"))
            .map(String::as_str),
        Some("empty")
    );
    assert_eq!(
        result
            .nodes
            .iter()
            .find(|node| node.kind == "MODULE" && node.path.as_deref() == Some("app"))
            .and_then(|node| node.properties.get("semantic"))
            .map(String::as_str),
        Some("indexed")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn module_with_only_empty_source_files_is_marked_empty() {
    let root = std::env::temp_dir().join(format!(
        "code-memory-architecture-empty-module-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("empty")).unwrap();
    fs::write(root.join("empty/__init__.py"), "").unwrap();
    let result = build(
        &root,
        &IndexOutput {
            documents: Vec::new(),
            ..sample_output()
        },
    );
    let module = result
        .nodes
        .iter()
        .find(|node| node.kind == "MODULE" && node.path.as_deref() == Some("empty"))
        .expect("empty module node");
    assert_eq!(
        module.properties.get("semantic").map(String::as_str),
        Some("empty")
    );
    assert_eq!(
        module.properties.get("source_files").map(String::as_str),
        Some("1")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn short_symbol_removes_scip_suffix() {
    assert_eq!(
        short_symbol("scip-typescript npm app src/components/Header.jsx/Header()."),
        "Header"
    );
}

#[test]
fn language_for_path_accepts_case_variants() {
    assert_eq!(language_for_path("src/App.TSX"), Some("typescript"));
    assert_eq!(language_for_path("src/App.VUE"), Some("typescript"));
    assert_eq!(language_for_path("src/Main.JAVA"), Some("java"));
}

#[test]
fn case_insensitive_boundary_scan_does_not_allocate_uppercase_copy() {
    assert!(contains_any_ascii_case_insensitive(
        "  SeLeCt * from users",
        &["SELECT "]
    ));
    assert!(!contains_any_ascii_case_insensitive(
        "selective value",
        &["SELECT "]
    ));
}

#[test]
fn database_boundary_ignores_comments_strings_and_dynamic_sql() {
    assert_eq!(
        static_database_operation("sql = \"SELECT id FROM orders\""),
        Some("READ")
    );
    assert_eq!(static_database_operation("# SELECT id FROM orders"), None);
    assert_eq!(
        static_database_operation("COMMENT_SQL = \"SELECT id FROM orders\""),
        None
    );
    assert_eq!(
        static_database_operation("logger.info(\"SELECT id FROM orders\")"),
        None
    );
    assert_eq!(
        static_database_operation("logger.query(\"SELECT id FROM orders\")"),
        None
    );
    assert_eq!(static_database_operation("table = \"orders\""), None);
    assert_eq!(
        static_database_operation("sql = \"SELECT id FROM \" + table"),
        None
    );
    assert_eq!(
        static_database_operation("cursor.execute(sql)"),
        Some("DB_CALL")
    );
}
