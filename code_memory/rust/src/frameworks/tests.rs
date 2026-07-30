use super::*;
use crate::{DocumentOutput, OccurrenceOutput};

#[test]
fn route_parser_covers_registration_and_annotation_shapes() {
    let pack = FrameworkPack {
        id: "test".to_string(),
        language: "typescript".to_string(),
        name: "test".to_string(),
        kind: "web".to_string(),
        signals: vec!["test".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "registration-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    for (source, method) in [
        ("app.get(\"/x\", handler);", "GET"),
        ("@app.get(\"/x\")\ndef health(): pass", "GET"),
        ("@router.post(\n\"/x\"\n)\ndef health(): pass", "POST"),
        ("router.GET(\"/x\", handler);", "GET"),
        ("$app->post(\"/x\", handler);", "POST"),
        ("app.MapGet(\"/x\", handler);", "GET"),
        ("Route::delete(\"/x\", handler);", "DELETE"),
        ("path(\"/x\", handler);", "ANY"),
        ("routes = [Route(\"/x\", handler)]", "ANY"),
        ("router.route(\"/x\", get(handler));", "GET"),
        ("CROW_ROUTE(app, \"/x\");", "ANY"),
        ("get \"/x\", to: \"health\"", "GET"),
        ("@GetMapping(\"/x\") void health() {}", "GET"),
        ("#[get(\"/x\")] fn health() {}", "GET"),
    ] {
        if source.contains("Route(\"/x\"") {
            assert_eq!(route_method(source), Some(method), "route method: {source}");
            assert!(first_route_path(source).is_some(), "route path: {source}");
        }
        let mut facts = Vec::new();
        extract_routes(&pack, "src/test", source, &[], None, &mut facts);
        assert_eq!(facts.len(), 1, "source: {source}");
        assert_eq!(facts[0].method.as_deref(), Some(method));
        assert_eq!(facts[0].path.as_deref(), Some("/x"));
    }
}

#[test]
fn fastapi_nested_router_prefix_and_handler_resolve() {
    let sources = vec![
            (
                "contexts.py".to_string(),
                "from server.app.api.http.routes.context import catalog_router\nrouter = APIRouter(prefix=\"/api/v1/context\")\nrouter.include_router(catalog_router)\n".to_string(),
            ),
            (
                "context/__init__.py".to_string(),
                "from .catalog import router as catalog_router\n".to_string(),
            ),
            (
                "context/catalog.py".to_string(),
                "router = APIRouter()\n@router.get(\"/accounts\")\ndef list_accounts(): pass\n".to_string(),
            ),
        ];
    let source_refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    let context = build_fastapi_route_context(&source_refs);
    assert_eq!(
        context.prefix_for("context/catalog.py", "@router.get(\"/accounts\")"),
        Some("/api/v1/context")
    );
    let pack = FrameworkPack {
        id: "fastapi".to_string(),
        language: "python".to_string(),
        name: "FastAPI".to_string(),
        kind: "web".to_string(),
        signals: vec!["fastapi".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string(), "HANDLES".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "annotation-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    let documents = vec![DocumentOutput {
        language: "python".to_string(),
        path: "context/catalog.py".to_string(),
        symbols: Vec::new(),
        occurrences: vec![OccurrenceOutput {
            symbol: "lsp . . . context.catalog.py#list_accounts@2:4".to_string(),
            range: vec![2, 0, 2, 30],
            enclosing_range: Vec::new(),
            definition: true,
            import: false,
            read: false,
            write: false,
        }],
    }];
    let mut facts = Vec::new();
    extract_routes(
        &pack,
        "context/catalog.py",
        &sources[2].1,
        &documents,
        Some(&context),
        &mut facts,
    );
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].path.as_deref(), Some("/api/v1/context/accounts"));
    assert_eq!(
        facts[0].symbol.as_deref(),
        Some("lsp . . . context.catalog.py#list_accounts@2:4")
    );
}

#[test]
fn filesystem_frameworks_create_routes_from_file_conventions() {
    let pack = FrameworkPack {
        id: "nextjs".to_string(),
        language: "typescript".to_string(),
        name: "Next.js".to_string(),
        kind: "web".to_string(),
        signals: vec!["pages".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "filesystem-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    let mut facts = Vec::new();
    extract_routes(
        &pack,
        "src/app/users/[id]/page.tsx",
        "export async function GET() {}",
        &[],
        None,
        &mut facts,
    );
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].path.as_deref(), Some("/users/:id"));
    assert_eq!(facts[0].method.as_deref(), Some("GET"));
    assert_eq!(facts[0].evidence, vec!["filesystem_route_convention"]);
}

#[test]
fn annotation_and_macro_routes_find_the_following_handler() {
    assert_eq!(
        nearby_handler(&["@app.get(\"/health\")", "def health(): pass"], 0),
        Some("health".to_string())
    );
    assert_eq!(
        registration_handler(")(health)"),
        Some("health".to_string())
    );
    assert_eq!(
        fact_properties("EVENT_HANDLER", "bus.on(\"created\", handler);")
            .get("event")
            .map(String::as_str),
        Some("created")
    );
    assert_eq!(
        fact_target_name("EVENT_HANDLER", "bus.on(\"created\", handler);"),
        Some("handler".to_string())
    );
    assert_eq!(
        fact_target_name("EVENT_HANDLER", "onTap: handleTap,"),
        Some("handleTap".to_string())
    );
    assert_eq!(
        fact_target_name("EVENT_HANDLER", "ON_COMMAND(ID_CLICK, OnClick)"),
        Some("OnClick".to_string())
    );
    assert_eq!(
        fact_target_name(
            "EVENT_HANDLER",
            r#"<button @onclick=\"OnClick\">ok</button>"#
        ),
        Some("OnClick".to_string())
    );
    assert_eq!(
        fact_target_name(
            "EVENT_HANDLER",
            r#"template: '<button @click=\"handle\">ok</button>'"#
        ),
        Some("handle".to_string())
    );
    assert_eq!(
        event_call_last_argument("uv_read_start(stream, alloc, read_callback); uv_run(0, 1);"),
        Some("read_callback".to_string())
    );
    assert_eq!(
        fact_target_name("DEPENDENCY", "builder.Services.AddScoped<UserService>();"),
        Some("UserService".to_string())
    );
    assert_eq!(
        fact_target_name("COMPONENT", "export const App = defineComponent({});"),
        Some("App".to_string())
    );
    assert_eq!(
        fact_properties("RENDERS", "return <Widget />;")
            .get("target")
            .map(String::as_str),
        Some("Widget")
    );
    assert_eq!(
        fact_target_name("DEPENDENCY", "@Autowired UserService service;"),
        Some("UserService".to_string())
    );
    assert_eq!(
        route_method("@RequestMapping(method = RequestMethod.POST, path = \"/owners\")"),
        Some("POST")
    );
    assert_eq!(
        nearby_handler(
            &[
                "@PostMapping(\"/owners\")",
                "// unrelated annotation",
                "// another line",
                "// another line",
                "// another line",
                "// another line",
                "// another line",
                "// another line",
                "public Owner createOwner() {",
            ],
            0
        ),
        Some("createOwner".to_string())
    );
    assert_eq!(
        fact_target_name("MIDDLEWARE", "async def auth_middleware(request):"),
        None
    );
    assert!(source_signal_matches(
        "src/app.ts",
        "import express from \"express\";",
        "import:express"
    ));
    assert!(!source_signal_matches(
        "src/app.ts",
        "const name = \"express\";",
        "import:express"
    ));

    let pack = FrameworkPack {
        id: "spring-mvc".to_string(),
        language: "java".to_string(),
        name: "Spring MVC".to_string(),
        kind: "web".to_string(),
        signals: vec!["@RequestMapping".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "annotation-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    let mut facts = Vec::new();
    extract_routes(
        &pack,
        "src/Users.java",
        "@RequestMapping(\"/api\")\n@GetMapping(\"/users\") void list() {}",
        &[],
        None,
        &mut facts,
    );
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].path.as_deref(), Some("/api/users"));

    let flask = FrameworkPack {
        id: "flask".to_string(),
        language: "python".to_string(),
        name: "Flask".to_string(),
        kind: "web".to_string(),
        signals: vec!["@app.route".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "annotation-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    let doc = DocumentOutput {
        language: "python".to_string(),
        path: "src/app.py".to_string(),
        symbols: Vec::new(),
        occurrences: vec![OccurrenceOutput {
            symbol: "fixture/handler().".to_string(),
            range: vec![0, 0, 0, 1],
            enclosing_range: Vec::new(),
            definition: true,
            import: false,
            read: false,
            write: false,
        }],
    };
    let mut flask_facts = Vec::new();
    assert_eq!(
        nearby_handler(&["@app.route(\"/fixture\")", "def handler(): pass"], 0),
        Some("handler".to_string())
    );
    assert_eq!(
        resolve_symbol(std::slice::from_ref(&doc), "src/app.py", "handler").as_deref(),
        Some("fixture/handler().")
    );
    extract_routes(
        &flask,
        "src/app.py",
        "@app.route(\"/fixture\")\ndef handler(): pass",
        &[doc],
        None,
        &mut flask_facts,
    );
    assert_eq!(flask_facts[0].symbol.as_deref(), Some("fixture/handler()."));
}

#[test]
fn java_annotations_bind_to_the_first_method_not_a_later_void_method() {
    assert_eq!(
        nearby_handler(
            &[
                "@GetMapping(\"/petTypes\")",
                "public List<PetType> getPetTypes() {",
                "    return service.findAll();",
                "}",
                "@PutMapping(\"/{id}\")",
                "public void processUpdateForm() {}",
            ],
            0,
        ),
        Some("getPetTypes".to_string())
    );
}

#[test]
fn java_functional_route_uses_its_exact_registration_method() {
    let lines = [
        "@Bean",
        "RouterFunction<?> routerFunction() {",
        "    return RouterFunctions.resources(\"/**\", resource)",
        "        .andRoute(RequestPredicates.GET(\"/\"), request -> response);",
    ];
    assert_eq!(
        enclosing_java_method(&lines, 3),
        Some("routerFunction".to_string())
    );

    let pack = FrameworkPack {
        id: "spring-webflux".to_string(),
        language: "java".to_string(),
        name: "Spring WebFlux".to_string(),
        kind: "web".to_string(),
        signals: vec!["RouterFunction".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string(), "HANDLES".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "annotation-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    let documents = vec![DocumentOutput {
        language: "java".to_string(),
        path: "src/Api.java".to_string(),
        symbols: Vec::new(),
        occurrences: vec![OccurrenceOutput {
            symbol: "java#Api.routerFunction()".to_string(),
            range: vec![1, 0, 1, 42],
            enclosing_range: Vec::new(),
            definition: true,
            import: false,
            read: false,
            write: false,
        }],
    }];
    let mut facts = Vec::new();
    extract_routes(
        &pack,
        "src/Api.java",
        &lines.join("\n"),
        &documents,
        None,
        &mut facts,
    );
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].path.as_deref(), Some("/"));
    assert_eq!(
        facts[0].symbol.as_deref(),
        Some("java#Api.routerFunction()")
    );
}

#[test]
fn java_request_mapping_method_and_multiple_paths_are_all_emitted() {
    let pack = FrameworkPack {
        id: "spring-mvc".to_string(),
        language: "java".to_string(),
        name: "Spring MVC".to_string(),
        kind: "web".to_string(),
        signals: vec!["@RequestMapping".to_string()],
        outputs: vec!["HTTP_ROUTE".to_string()],
        rules: vec!["HTTP_ROUTE".to_string()],
        adapter: "annotation-routing".to_string(),
        fixture: FrameworkFixture::default(),
    };
    let mut facts = Vec::new();
    extract_routes(
        &pack,
        "src/Owners.java",
        "@RequestMapping(\"/api\")\n@RequestMapping(method = RequestMethod.GET, path = {\"/owners\", \"/users\"})\npublic void list() {}",
        &[],
        None,
        &mut facts,
    );
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].method.as_deref(), Some("GET"));
    assert_eq!(facts[0].path.as_deref(), Some("/api/owners"));
    assert_eq!(facts[1].path.as_deref(), Some("/api/users"));
}

#[test]
fn java_field_injection_annotation_can_be_on_the_previous_line() {
    assert_eq!(
        dependency_type_name("@Autowired @Qualifier(\"users\") private UserService service;"),
        Some("UserService".to_string())
    );
    assert_eq!(
        java_dependency_annotation_context(
            &[
                "@Autowired",
                "@Qualifier(\"users\")",
                "private UserService service;",
            ],
            0,
        ),
        Some("@Autowired @Qualifier(\"users\") private UserService service;".to_string())
    );
}

#[test]
fn cross_file_provider_reference_resolves_framework_handler() {
    let documents = vec![
        DocumentOutput {
            language: "typescript".to_string(),
            path: "src/routes.ts".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "project/src/handlers.ts/health().".to_string(),
                range: vec![3, 18, 3, 24],
                enclosing_range: vec![3, 0, 3, 30],
                definition: false,
                import: false,
                read: true,
                write: false,
            }],
        },
        DocumentOutput {
            language: "typescript".to_string(),
            path: "src/handlers.ts".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "project/src/handlers.ts/health().".to_string(),
                range: vec![0, 16, 0, 22],
                enclosing_range: Vec::new(),
                definition: true,
                import: false,
                read: false,
                write: false,
            }],
        },
    ];
    assert_eq!(
        resolve_symbol(&documents, "src/routes.ts", "health").as_deref(),
        Some("project/src/handlers.ts/health().")
    );
    assert_eq!(
        resolve_symbol_at(&documents, "src/routes.ts", "health", 3).as_deref(),
        Some("project/src/handlers.ts/health().")
    );
}

#[test]
fn symbol_short_name_handles_lsp_location_suffix() {
    assert_eq!(
        symbol_short_name("lsp . . . src.routes.rs#health@0:13"),
        "health"
    );
    assert_eq!(
        symbol_short_name("scip-php composer visualmap/test-php 1.0.0.0 App/Handler#index()."),
        "index"
    );
    assert_eq!(
        symbol_short_name("lsp . . . app.Owner#updateOwner(int, OwnerRequest)@84:16"),
        "updateOwner"
    );
    let documents = vec![DocumentOutput {
        language: "php".to_string(),
        path: "src/Fixture.php".to_string(),
        symbols: Vec::new(),
        occurrences: vec![OccurrenceOutput {
            symbol: "scip-php composer visualmap/test-php 1.0.0.0 App/Handler#index().".to_string(),
            range: vec![2, 32, 2, 37],
            enclosing_range: Vec::new(),
            definition: true,
            import: false,
            read: false,
            write: false,
        }],
    }];
    assert!(resolve_symbol(&documents, "app/Config/Routes.php", "Handler::index").is_some());
}

#[test]
fn ambiguous_cross_file_name_does_not_create_a_framework_target() {
    let documents = vec![
        DocumentOutput {
            language: "python".to_string(),
            path: "src/routes.py".to_string(),
            symbols: Vec::new(),
            occurrences: Vec::new(),
        },
        DocumentOutput {
            language: "python".to_string(),
            path: "src/a.py".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "project/src/a.py/handler().".to_string(),
                range: vec![0, 4, 0, 11],
                enclosing_range: Vec::new(),
                definition: true,
                import: false,
                read: false,
                write: false,
            }],
        },
        DocumentOutput {
            language: "python".to_string(),
            path: "src/b.py".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "project/src/b.py/handler().".to_string(),
                range: vec![0, 4, 0, 11],
                enclosing_range: Vec::new(),
                definition: true,
                import: false,
                read: false,
                write: false,
            }],
        },
    ];
    assert!(resolve_symbol(&documents, "src/routes.py", "handler").is_none());
}

#[test]
fn cpp_header_declaration_prefers_implementation_symbol() {
    let documents = vec![
        DocumentOutput {
            language: "cpp".to_string(),
            path: "main.cpp".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "lsp . . . handlers.cpp#health@2:5".to_string(),
                range: vec![5, 29, 5, 35],
                enclosing_range: vec![3, 0, 7, 1],
                definition: false,
                import: false,
                read: true,
                write: false,
            }],
        },
        DocumentOutput {
            language: "cpp".to_string(),
            path: "handlers.cpp".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "lsp . . . handlers.cpp#health@2:5".to_string(),
                range: vec![2, 0, 2, 16],
                enclosing_range: Vec::new(),
                definition: true,
                import: false,
                read: false,
                write: false,
            }],
        },
        DocumentOutput {
            language: "cpp".to_string(),
            path: "handlers.hpp".to_string(),
            symbols: Vec::new(),
            occurrences: vec![OccurrenceOutput {
                symbol: "lsp . . . handlers.hpp#health@2:5".to_string(),
                range: vec![2, 0, 2, 13],
                enclosing_range: Vec::new(),
                definition: true,
                import: false,
                read: false,
                write: false,
            }],
        },
    ];
    assert_eq!(
        resolve_symbol_at(&documents, "main.cpp", "health", 5).as_deref(),
        Some("lsp . . . handlers.cpp#health@2:5")
    );
}

#[test]
fn python_service_class_emits_service_fact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let pack = load_packs(root)
        .expect("framework packs should load")
        .into_iter()
        .find(|pack| pack.language == "python" && pack.id == "django")
        .expect("django pack");
    let mut facts = Vec::new();
    extract_generic_facts(
        &pack,
        "src/fixture.py",
        "class UserService:\n    pass",
        &[],
        &mut facts,
    );
    assert!(facts.iter().any(|fact| fact.kind == "SERVICE"));
}

#[test]
fn php_class_method_route_resolves_project_definition() {
    let documents = vec![DocumentOutput {
        language: "php".to_string(),
        path: "src/Fixture.php".to_string(),
        symbols: Vec::new(),
        occurrences: vec![OccurrenceOutput {
            symbol: "scip-php composer visualmap/test-php 1.0.0.0 App/Handler#index().".to_string(),
            range: vec![2, 32, 2, 37],
            enclosing_range: Vec::new(),
            definition: true,
            import: false,
            read: false,
            write: false,
        }],
    }];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let pack = load_packs(root)
        .expect("framework packs should load")
        .into_iter()
        .find(|pack| pack.id == "codeigniter")
        .expect("codeigniter pack");
    let mut facts = Vec::new();
    assert_eq!(
        registration_handler(", \"Handler::index\");"),
        Some("index".to_string())
    );
    assert!(resolve_symbol_at(&documents, "app/Config/Routes.php", "index", 1).is_some());
    extract_routes(
            &pack,
            "app/Config/Routes.php",
            "<?php\nfunction register_routes($routes) { $routes->get(\"/fixture\", \"Handler::index\"); }",
            &documents,
            None,
            &mut facts,
        );
    assert_eq!(facts.len(), 1);
    assert!(facts[0].symbol.is_some());
}

#[test]
fn php_attribute_class_route_resolves_project_definition() {
    let documents = vec![DocumentOutput {
        language: "php".to_string(),
        path: "src/Fixture.php".to_string(),
        symbols: Vec::new(),
        occurrences: vec![OccurrenceOutput {
            symbol: "scip-php composer visualmap/test-php 1.0.0.0 UserEndpoint#".to_string(),
            range: vec![5, 6, 5, 18],
            enclosing_range: Vec::new(),
            definition: true,
            import: false,
            read: false,
            write: false,
        }],
    }];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let pack = load_packs(root)
        .expect("framework packs should load")
        .into_iter()
        .find(|pack| pack.id == "api-platform")
        .expect("api platform pack");
    let mut facts = Vec::new();
    assert_eq!(route_method("#[Get(\"/fixture\")]"), Some("GET"));
    assert_eq!(
        annotation_handler_name("#[Get(\"/fixture\")] class UserEndpoint {}"),
        Some("UserEndpoint".to_string())
    );
    assert_eq!(
        nearby_handler(&["#[Get(\"/fixture\")]", "class UserEndpoint {}"], 0),
        Some("UserEndpoint".to_string())
    );
    assert!(resolve_symbol_at(&documents, "src/Fixture.php", "UserEndpoint", 0).is_some());
    extract_routes(
        &pack,
        "src/Fixture.php",
        "#[Get(\"/fixture\")]\nclass UserEndpoint {}",
        &documents,
        None,
        &mut facts,
    );
    assert_eq!(facts.len(), 1);
    assert!(facts[0].symbol.is_some());
}

#[test]
fn every_pack_is_detected_and_emits_its_declared_rules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let packs = load_packs(root).expect("framework packs should load");
    let temp =
        std::env::temp_dir().join(format!("code-memory-framework-gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("create framework fixture root");

    for pack in &packs {
        let fixture = temp.join(&pack.id);
        let mut documents = Vec::new();
        for file in &pack.fixture.files {
            let path = fixture.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create framework fixture directory");
            }
            fs::write(&path, &file.source).expect("write framework fixture file");
            if fixture_is_source_file(pack, &file.path) {
                documents.push(DocumentOutput {
                    language: pack.language.clone(),
                    path: file.path.clone(),
                    symbols: Vec::new(),
                    occurrences: Vec::new(),
                });
            }
        }
        let names = [
            "handler",
            "Handler",
            "health",
            "GET",
            "POST",
            "PUT",
            "PATCH",
            "DELETE",
            "UserService",
            "UserEndpoint",
            "User",
            "Endpoint",
            "Controller",
            "App",
            "onRequest",
            "default",
            "callback",
            "handle",
            "handleTap",
            "OnClick",
            "Increment",
            "list",
            "list_sessions",
            "run",
            "task",
            "service",
            "index",
        ];
        for document in &mut documents {
            document.occurrences = names
                .iter()
                .map(|name| OccurrenceOutput {
                    symbol: format!("fixture/{name}()."),
                    range: vec![0, 0, 0, 1],
                    enclosing_range: Vec::new(),
                    definition: true,
                    import: false,
                    read: false,
                    write: false,
                })
                .collect();
        }
        let analysis = analyze(&fixture, &documents, root).expect("analyze framework fixture");
        let output = analysis
            .frameworks
            .iter()
            .find(|output| output.id == pack.id && output.language == pack.language)
            .unwrap_or_else(|| panic!("{} was not detected", pack.id));
        for rule in &pack.fixture.expected_facts {
            assert!(
                output.facts.iter().any(|fact| fact.kind == *rule)
                    || (pack.language == "java"
                        && analysis.frameworks.iter().any(|framework| {
                            framework.language == "java"
                                && framework.facts.iter().any(|fact| fact.kind == *rule)
                        })),
                "{} did not emit {}",
                pack.id,
                rule
            );
        }
        assert!(
            output.facts.iter().all(|fact| fact.source_range.len() == 4),
            "{} emitted a fact without a source range",
            pack.id
        );
        for relation_kind in &pack.fixture.expected_relations {
            assert!(
                analysis.relations.iter().any(|relation| {
                    relation.framework == pack.id && relation.kind == *relation_kind
                }) || (pack.language == "java"
                    && *relation_kind == "HANDLES"
                    && analysis.relations.iter().any(|relation| {
                        relation.kind == "HANDLES"
                            && analysis.frameworks.iter().any(|framework| {
                                framework.language == "java"
                                    && framework.facts.iter().any(|fact| fact.id == relation.to)
                            })
                    })),
                "{} did not emit a {} relation",
                pack.id,
                relation_kind
            );
        }
        assert!(
            analysis
                .relations
                .iter()
                .filter(|relation| relation.framework == pack.id)
                .all(|relation| relation.range.len() == 4),
            "{} emitted a relation without a source range",
            pack.id
        );
        if pack.outputs.iter().any(|output| output == "HANDLES") {
            assert!(
                pack.fixture
                    .expected_relations
                    .iter()
                    .any(|kind| kind == "HANDLES"),
                "{} declares HANDLES without a fixture expectation",
                pack.id
            );
        }
    }
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn one_framework_signal_does_not_activate_a_pack() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let packs = load_packs(root).expect("framework packs should load");
    let temp = std::env::temp_dir().join(format!(
        "code-memory-framework-negative-gate-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp);

    for pack in &packs {
        let source_file = pack
            .fixture
            .files
            .iter()
            .find(|file| fixture_is_source_file(pack, &file.path))
            .expect("fixture source file");
        let extension = source_file
            .path
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .unwrap_or("src");
        for signal in &pack.signals {
            let case = temp.join(&pack.id).join(format!("negative.{extension}"));
            if let Some(parent) = case.parent() {
                fs::create_dir_all(parent).expect("create negative fixture directory");
            }
            fs::write(&case, single_signal_source(signal)).expect("write negative fixture");
            let analysis = analyze(&temp.join(&pack.id), &[], root)
                .expect("analyze negative framework fixture");
            assert!(
                analysis
                    .frameworks
                    .iter()
                    .all(|framework| framework.id != pack.id),
                "{} activated from one signal: {}",
                pack.id,
                signal
            );
            let _ = fs::remove_dir_all(temp.join(&pack.id));
        }
    }
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn metadata_signals_are_limited_to_the_pack_language() {
    assert!(metadata_matches_language("package.json", "javascript"));
    assert!(!metadata_matches_language("CMakeLists.txt", "javascript"));
    assert!(metadata_matches_language("Gemfile", "ruby"));
    assert!(!metadata_matches_language("CMakeLists.txt", "ruby"));
}

#[test]
fn prefixed_source_signals_do_not_match_only_a_path_name() {
    assert!(!source_signal_matches(
        "src/next_helpers.c",
        "void helper() {}",
        "import:next"
    ));
    assert!(source_signal_matches(
        "src/app.ts",
        "import next from 'next';",
        "import:next"
    ));
}

#[test]
fn c_project_metadata_does_not_activate_web_or_ruby_packs() {
    let pack_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = std::env::temp_dir().join(format!(
        "code-memory-framework-language-gate-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("create language gate fixture");
    fs::write(
        root.join("CMakeLists.txt"),
        "set(NEXT_TARGET pages)\nset(RACK_MODE run)\n",
    )
    .expect("write metadata fixture");
    fs::write(root.join("src/main.c"), "int main(void) { return 0; }\n").expect("write C fixture");
    let analysis = analyze(&root, &[], pack_root).expect("analyze language gate fixture");
    assert!(analysis
        .frameworks
        .iter()
        .all(|framework| !matches!(framework.id.as_str(), "nextjs" | "rack")));
    let _ = fs::remove_dir_all(root);
}

fn single_signal_source(signal: &str) -> String {
    let needle = signal_needle(signal);
    match signal.split_once(':').map(|(prefix, _)| prefix) {
        Some("import") => format!("import {needle};"),
        Some("require") => format!("require('{needle}');"),
        Some("include") => format!("#include {needle}"),
        _ => needle,
    }
}

#[test]
fn every_declared_shared_rule_has_an_evidence_pattern() {
    let pack = FrameworkPack {
        id: "test".to_string(),
        language: "typescript".to_string(),
        name: "test".to_string(),
        kind: "web".to_string(),
        signals: vec!["service".to_string(), "component".to_string()],
        outputs: vec![],
        rules: vec![],
        adapter: "component-events".to_string(),
        fixture: FrameworkFixture::default(),
    };
    for (rule, line) in [
        ("COMPONENT", "class App extends StatelessWidget {}"),
        ("RENDERS", "return <Widget />;"),
        ("EVENT_HANDLER", "bus.emit(\"created\");"),
        ("SERVICE", "@Service class UserService {}"),
        ("SERVICE", "class UserService:"),
        ("MIDDLEWARE", "app.use(authMiddleware);"),
        ("DEPENDENCY", "@Autowired UserService service;"),
        ("ASYNC_CALLS", "tokio::spawn(task);"),
        ("RPC_ENDPOINT", "service UserService {}"),
        ("SERVER_ACTION", "use server"),
        ("SCHEMA", "#[derive(GraphQLObject)]"),
        ("SCHEDULED_JOB", "@Scheduled(cron = \"* * * * *\")"),
    ] {
        assert!(output_evidence(&pack, rule, line).is_some(), "rule: {rule}");
    }
}

#[test]
fn every_declared_pack_has_an_executable_shared_rule() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let packs = load_packs(root).expect("framework packs should load");
    assert_eq!(packs.len(), 84);
    for pack in &packs {
        for rule in &pack.rules {
            if matches!(rule.as_str(), "HTTP_ROUTE" | "HANDLES") {
                continue;
            }
            let line =
                if pack.id == "tauri" && pack.language == "javascript" && rule == "ASYNC_CALLS" {
                    "await invoke(\"command\")"
                } else if pack.id == "tauri" && pack.language == "rust" && rule == "RPC_ENDPOINT" {
                    "#[tauri::command]"
                } else {
                    representative_line(rule)
                };
            assert!(
                output_evidence(pack, rule, line).is_some(),
                "{} has no evidence pattern for {}",
                pack.id,
                rule
            );
        }
        if pack.rules.iter().any(|rule| rule == "HTTP_ROUTE") {
            let mut facts = Vec::new();
            extract_routes(
                pack,
                "src/framework_fixture",
                representative_route(pack),
                &[],
                None,
                &mut facts,
            );
            assert!(
                !facts.is_empty(),
                "{} has no executable HTTP route shape",
                pack.id
            );
        }
    }
}

#[test]
fn duplicate_framework_routes_keep_one_fact_and_handle_relation() {
    let route = |id: &str, framework: &str| FrameworkFact {
        id: id.to_string(),
        kind: "HTTP_ROUTE".to_string(),
        framework: framework.to_string(),
        symbol: Some("java#Controller.handle".to_string()),
        method: Some("GET".to_string()),
        path: Some("/owners/{ownerId}".to_string()),
        source_file: "src/Controller.java".to_string(),
        source_line: 12,
        source_end_line: 12,
        source_range: vec![11, 0, 11, 40],
        evidence: vec!["http_route_syntax".to_string()],
        properties: BTreeMap::new(),
    };
    let output = |id: &str, framework: &str| FrameworkOutput {
        id: id.to_string(),
        language: "java".to_string(),
        name: framework.to_string(),
        kind: "web".to_string(),
        adapter: "annotation-routing".to_string(),
        status: "detected".to_string(),
        matched_signals: vec!["@Controller".to_string()],
        files: vec!["src/Controller.java".to_string()],
        facts: vec![route(&format!("{id}-route"), framework)],
    };
    let mut frameworks = vec![
        output("spring", "spring"),
        output("spring-mvc", "spring-mvc"),
    ];
    let mut relations = vec![
        FrameworkRelation {
            from: "java#Controller.handle".to_string(),
            to: "spring-route".to_string(),
            kind: "HANDLES".to_string(),
            framework: "spring".to_string(),
            path: "src/Controller.java".to_string(),
            range: vec![11, 0, 11, 40],
            evidence: vec!["http_route_syntax".to_string()],
        },
        FrameworkRelation {
            from: "java#Controller.handle".to_string(),
            to: "spring-mvc-route".to_string(),
            kind: "HANDLES".to_string(),
            framework: "spring-mvc".to_string(),
            path: "src/Controller.java".to_string(),
            range: vec![11, 0, 11, 40],
            evidence: vec!["http_route_syntax".to_string()],
        },
    ];

    dedupe_java_facts(&mut frameworks, &mut relations);

    assert_eq!(
        frameworks
            .iter()
            .map(|item| item.facts.len())
            .sum::<usize>(),
        1
    );
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].to, "spring-route");
}

#[test]
fn java_constructor_dependencies_require_the_enclosing_type() {
    let lines = [
        "public class ApiGatewayController {",
        "    public ApiGatewayController(CustomersServiceClient customers,",
        "                                VisitsServiceClient visits) {",
        "    }",
        "    public String getOwner(String ownerId) {",
        "        return ownerId;",
        "    }",
    ];
    assert_eq!(
        java_constructor_dependency_types(&lines, 1),
        vec![
            "CustomersServiceClient".to_string(),
            "VisitsServiceClient".to_string()
        ]
    );
    assert!(java_constructor_dependency_types(&lines, 4).is_empty());
}

#[test]
fn java_package_private_constructor_dependencies_are_indexed() {
    let lines = [
        "class ApiGatewayController {",
        "    ApiGatewayController(CustomersServiceClient customers) {",
        "    }",
    ];
    assert_eq!(
        java_constructor_dependency_types(&lines, 1),
        vec!["CustomersServiceClient".to_string()]
    );
}

#[test]
fn java_route_paths_ignore_non_path_annotation_values() {
    assert_eq!(
        java_route_paths(
            r#"@RequestMapping(path = {"/owners", "/pets"}, headers = "X-Route=/header", produces = "/mime")"#
        ),
        vec!["/owners".to_string(), "/pets".to_string()]
    );
    assert_eq!(
        java_route_paths(r#"@GetMapping(value = "/owners", params = "/debug")"#),
        vec!["/owners".to_string()]
    );
}

#[test]
fn java_dependency_facts_require_injection_context() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let spring = load_packs(root)
        .expect("framework packs should load")
        .into_iter()
        .find(|pack| pack.id == "spring")
        .expect("spring pack");
    let source = "@Component\nclass Client {\n    Client(WebClient webClient) {}\n}\nclass OwnerRequest {\n    OwnerRequest(String ownerId) {}\n}\n";
    let mut facts = Vec::new();
    extract_generic_facts(&spring, "src/Client.java", source, &[], &mut facts);
    let dependencies = facts
        .iter()
        .filter(|fact| fact.kind == "DEPENDENCY")
        .filter_map(|fact| fact.properties.get("target"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(dependencies, vec!["WebClient".to_string()]);
}

#[test]
fn java_service_facts_for_one_type_are_deduplicated_across_lines() {
    let fact = |id: &str, line: usize| FrameworkFact {
        id: id.to_string(),
        kind: "SERVICE".to_string(),
        framework: "spring".to_string(),
        symbol: Some("java#Client".to_string()),
        method: None,
        path: None,
        source_file: "src/Client.java".to_string(),
        source_line: line,
        source_end_line: line,
        source_range: vec![line as i32 - 1, 0, line as i32 - 1, 10],
        evidence: vec!["service_annotation".to_string()],
        properties: BTreeMap::new(),
    };
    let mut frameworks = vec![FrameworkOutput {
        id: "spring".to_string(),
        language: "java".to_string(),
        name: "Spring".to_string(),
        kind: "web".to_string(),
        adapter: "annotation-routing".to_string(),
        status: "detected".to_string(),
        matched_signals: Vec::new(),
        files: vec!["src/Client.java".to_string()],
        facts: vec![fact("service-1", 1), fact("service-2", 2)],
    }];
    let mut relations = vec![
        FrameworkRelation {
            from: "java#Client".to_string(),
            to: "service-1".to_string(),
            kind: "DECLARES_SERVICE".to_string(),
            framework: "spring".to_string(),
            path: "src/Client.java".to_string(),
            range: vec![0, 0, 0, 10],
            evidence: vec!["service_annotation".to_string()],
        },
        FrameworkRelation {
            from: "java#Client".to_string(),
            to: "service-2".to_string(),
            kind: "DECLARES_SERVICE".to_string(),
            framework: "spring".to_string(),
            path: "src/Client.java".to_string(),
            range: vec![1, 0, 1, 10],
            evidence: vec!["service_annotation".to_string()],
        },
    ];
    dedupe_java_facts(&mut frameworks, &mut relations);
    assert_eq!(frameworks[0].facts.len(), 1);
    assert_eq!(relations.len(), 1);
}

fn representative_line(rule: &str) -> &'static str {
    match rule {
        "COMPONENT" => "@Component class App extends Component {}",
        "RENDERS" => "return <Widget />;",
        "EVENT_HANDLER" => "bus.on(\"created\", handler);",
        "SERVICE" => "@Service class UserService {}",
        "MIDDLEWARE" => "app.use(authMiddleware);",
        "DEPENDENCY" => "@Autowired UserService service;",
        "ASYNC_CALLS" => "tokio::spawn(task);",
        "RPC_ENDPOINT" => "service UserService {}",
        "SERVER_ACTION" => "use server",
        "SCHEMA" => "#[derive(GraphQLObject)] struct User {}",
        "SCHEDULED_JOB" => "@Scheduled(cron = \"* * * * *\") void run() {}",
        _ => "",
    }
}

fn representative_route(pack: &FrameworkPack) -> &'static str {
    match pack.language.as_str() {
        "typescript" | "javascript" => "app.get(\"/fixture\", handler);",
        "python" => "@app.get(\"/fixture\")\ndef handler(): pass",
        "java" => "@GetMapping(\"/fixture\") void handler() {}",
        "csharp" => "app.MapGet(\"/fixture\", handler);",
        "cpp" | "c" => "CROW_ROUTE(app, \"/fixture\")(handler);",
        "go" => "router.GET(\"/fixture\", handler)",
        "rust" => "router.route(\"/fixture\", get(handler));",
        "php" => "Route::get('/fixture', 'handler');",
        "ruby" => "get \"/fixture\" do; end",
        "dart" => "router.get('/fixture', handler);",
        _ => "app.get(\"/fixture\", handler);",
    }
}
