#[cfg(test)]
mod tests {
    use super::connection::{
        complete_pending_with_error, initialization_options, request_timeout_scope,
    };
    use super::{
        bundled_java_home, canonicalize_lsp_symbols, collect_lsp_symbols,
        default_lsp_request_timeout, java_configuration_runtimes, java_home_is_usable,
        java_explicit_override_annotation, java_language_server_settings_for_network,
        fair_large_call_site_queries, find_lsp_symbol_at_range, jdtls_heap_mb,
        large_call_query_group, lsp_symbol_base_name,
        large_symbol_call_priority, large_symbol_is_map_boundary, large_workspace_workload,
        lsp_message_length_allowed, lsp_open_document_limit, lsp_reference_enrichment_enabled,
        lsp_request_batch_size, python_private_member_name, reconcile_lsp_symbol_owners,
        repair_java_lsp_symbol_selections,
        rust_analyzer_settings, rust_large_symbol_is_public, symbol_string, uri_to_relative_path,
        LargeCallSiteQuery, LspSymbol, LspSymbolParent, MAX_LSP_MESSAGE_BYTES,
    };
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::Path;

    #[test]
    fn lsp_message_size_is_bounded() {
        assert!(lsp_message_length_allowed(0));
        assert!(lsp_message_length_allowed(MAX_LSP_MESSAGE_BYTES));
        assert!(!lsp_message_length_allowed(MAX_LSP_MESSAGE_BYTES + 1));
    }

    #[test]
    fn cold_provider_start_has_a_reliable_default_response_window() {
        assert_eq!(default_lsp_request_timeout().as_secs(), 60);
    }

    #[test]
    fn large_workspace_lsp_pipeline_has_a_bounded_default_width() {
        assert!((1..=64).contains(&lsp_request_batch_size()));
    }

    #[test]
    fn large_workspace_type_extraction_cannot_issue_an_unplanned_query() {
        let uri = "file:///workspace/src/Service.java";
        let planned = HashSet::from([(uri.to_string(), 12, 8)]);

        assert!(super::type_resolution_query_is_planned(
            true,
            &planned,
            uri,
            &[12, 8, 12, 15]
        ));
        assert!(!super::type_resolution_query_is_planned(
            true,
            &planned,
            uri,
            &[40, 4, 40, 11]
        ));
        assert!(super::type_resolution_query_is_planned(
            false,
            &HashSet::new(),
            uri,
            &[40, 4, 40, 11]
        ));
    }

    #[test]
    fn a_stalled_batch_member_does_not_discard_completed_sibling_responses() {
        let mut pending = HashMap::from([(42, 1), (41, 3)]);
        let mut results = vec![
            Some(Ok(serde_json::json!({"uri":"file:///done.java"}))),
            None,
            Some(Err("provider returned an explicit error".to_string())),
            None,
        ];

        let cancelled = complete_pending_with_error(
            &mut pending,
            &mut results,
            "native LSP response timeout after 60000 ms",
        );

        assert_eq!(cancelled, vec![41, 42]);
        assert!(pending.is_empty());
        assert_eq!(
            results[0],
            Some(Ok(serde_json::json!({"uri":"file:///done.java"})))
        );
        assert_eq!(
            results[2],
            Some(Err("provider returned an explicit error".to_string()))
        );
        assert!(results[1]
            .as_ref()
            .is_some_and(|result| result.as_ref().is_err_and(|error| error.contains("timeout"))));
        assert!(results[3]
            .as_ref()
            .is_some_and(|result| result.as_ref().is_err_and(|error| error.contains("timeout"))));
    }

    #[test]
    fn repeated_timeouts_are_quarantined_by_method_and_document() {
        let first_definition = request_timeout_scope(
            "textDocument/definition",
            &serde_json::json!({
                "textDocument":{"uri":"file:///src/OutsideBuildPath.java"},
                "position":{"line":4,"character":8}
            }),
        );
        let second_definition = request_timeout_scope(
            "textDocument/definition",
            &serde_json::json!({
                "textDocument":{"uri":"file:///src/OutsideBuildPath.java"},
                "position":{"line":40,"character":2}
            }),
        );
        let call_query = request_timeout_scope(
            "textDocument/prepareCallHierarchy",
            &serde_json::json!({
                "textDocument":{"uri":"file:///src/OutsideBuildPath.java"},
                "position":{"line":4,"character":8}
            }),
        );

        assert_eq!(first_definition, second_definition);
        assert_ne!(first_definition, call_query);
    }

    #[test]
    fn java_source_only_fallback_opens_every_scheduled_document() {
        assert_eq!(lsp_open_document_limit("jdtls", true, false), Some(256));
        assert_eq!(lsp_open_document_limit("jdtls", true, true), None);
        assert_eq!(lsp_open_document_limit("jdtls", false, true), None);
    }

    #[test]
    fn lsp_symbol_identity_ignores_call_hierarchy_return_type_suffix() {
        assert_eq!(
            symbol_string("src/Client.java", "getOwner(int) : Mono<Owner>", 3, 8),
            symbol_string("src/Client.java", "getOwner(int)", 3, 8)
        );
        assert_eq!(
            symbol_string("src/Client.java", "getOwner(int)", 3, 8),
            "lsp . . . src.Client.java#getOwner(int)@3:8"
        );
    }

    #[test]
    fn python_name_mangled_members_are_not_treated_as_inherited_overrides() {
        assert!(python_private_member_name("__secret"));
        assert!(python_private_member_name("__secret(value)"));
        assert!(!python_private_member_name("execute"));
        assert!(!python_private_member_name("__init__"));
    }

    #[test]
    fn hierarchical_document_symbols_keep_the_exact_parent_after_deduplication() {
        let response = serde_json::json!({
            "name": "Box",
            "kind": 5,
            "range": {
                "start": {"line": 1, "character": 0},
                "end": {"line": 8, "character": 1}
            },
            "selectionRange": {
                "start": {"line": 1, "character": 6},
                "end": {"line": 1, "character": 9}
            },
            "children": [{
                "name": "get",
                "kind": 6,
                "range": {
                    "start": {"line": 4, "character": 2},
                    "end": {"line": 6, "character": 3}
                },
                "selectionRange": {
                    "start": {"line": 4, "character": 5},
                    "end": {"line": 4, "character": 8}
                }
            }]
        });
        let mut symbols = Vec::new();
        collect_lsp_symbols(&response, &mut symbols);
        let flat_duplicate = symbols[1].clone();
        symbols.push(super::LspSymbol {
            parent: None,
            ..flat_duplicate
        });

        canonicalize_lsp_symbols(&mut symbols);

        assert_eq!(symbols.len(), 2);
        let method = symbols.iter().find(|symbol| symbol.name == "get").unwrap();
        let parent = method.parent.as_ref().unwrap();
        assert_eq!(parent.name, "Box");
        assert_eq!((parent.selection_line, parent.selection_character), (1, 6));
    }

    #[test]
    fn duplicate_generic_constructor_labels_share_one_exact_definition() {
        let parent = Some(LspSymbolParent {
            name: "BoxValue".to_string(),
            selection_line: 13,
            selection_character: 6,
        });
        let mut symbols = vec![
            LspSymbol {
                name: "BoxValue<T>".to_string(),
                kind: 9,
                detail: None,
                range_start_line: 15,
                range_start_character: 13,
                range_end_line: 15,
                range_end_character: 21,
                selection_line: 15,
                selection_character: 13,
                parent: parent.clone(),
            },
            LspSymbol {
                name: "BoxValue".to_string(),
                kind: 9,
                detail: None,
                range_start_line: 15,
                range_start_character: 4,
                range_end_line: 15,
                range_end_character: 49,
                selection_line: 15,
                selection_character: 13,
                parent,
            },
        ];

        canonicalize_lsp_symbols(&mut symbols);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "BoxValue");
        assert_eq!(symbols[0].range_start_character, 4);
        assert_eq!(symbols[0].range_end_character, 49);
    }

    #[test]
    fn full_definition_range_selects_its_method_not_a_nested_symbol() {
        let method = LspSymbol {
            name: "hasText".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 10,
            range_start_character: 1,
            range_end_line: 15,
            range_end_character: 2,
            selection_line: 10,
            selection_character: 20,
            parent: None,
        };
        let nested = LspSymbol {
            name: "hasLength".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 12,
            range_start_character: 4,
            range_end_line: 12,
            range_end_character: 30,
            selection_line: 12,
            selection_character: 12,
            parent: None,
        };

        let symbols = [method, nested];
        let selected = find_lsp_symbol_at_range(&symbols, &[10, 1, 15, 2]).unwrap();
        assert_eq!(selected.name, "hasText");
    }

    #[test]
    fn target_selection_range_selects_the_exact_symbol_name() {
        let first = LspSymbol {
            name: "first".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 20,
            range_start_character: 1,
            range_end_line: 25,
            range_end_character: 2,
            selection_line: 20,
            selection_character: 10,
            parent: None,
        };
        let second = LspSymbol {
            name: "second".to_string(),
            selection_character: 30,
            ..first.clone()
        };

        let symbols = [first, second];
        let selected = find_lsp_symbol_at_range(&symbols, &[20, 30, 20, 36]).unwrap();
        assert_eq!(selected.name, "second");
    }

    #[test]
    fn impossible_selection_outside_its_declaration_cannot_be_a_target() {
        let malformed = LspSymbol {
            name: "setTarget".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 47,
            range_start_character: 4,
            range_end_line: 55,
            range_end_character: 5,
            selection_line: 0,
            selection_character: 0,
            parent: None,
        };

        assert!(find_lsp_symbol_at_range(&[malformed], &[0, 0, 0, 23]).is_none());
    }

    #[test]
    fn malformed_java_selection_is_repaired_only_by_one_name_inside_its_declaration() {
        let source = "package sample;\nclass Box {\n    public void setTarget(Object value) {\n        this.value = value;\n    }\n}\n";
        let mut symbols = vec![LspSymbol {
            name: "setTarget(Object)".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 2,
            range_start_character: 4,
            range_end_line: 4,
            range_end_character: 5,
            selection_line: 0,
            selection_character: 0,
            parent: None,
        }];

        let receipt = repair_java_lsp_symbol_selections(source, &mut symbols);

        assert_eq!(receipt, (1, 0));
        assert_eq!(symbols[0].selection_line, 2);
        assert_eq!(symbols[0].selection_character, 16);
        assert_eq!(lsp_symbol_base_name(&symbols[0].name), "setTarget");
    }

    #[test]
    fn malformed_java_declaration_end_keeps_an_exact_source_backed_selection() {
        let source = "class Box {\n    void replaceAdvisor() {\n    }\n}\n";
        let mut symbols = vec![LspSymbol {
            name: "replaceAdvisor()".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 1,
            range_start_character: 4,
            range_end_line: 0,
            range_end_character: 0,
            selection_line: 1,
            selection_character: 9,
            parent: None,
        }];

        let receipt = repair_java_lsp_symbol_selections(source, &mut symbols);

        assert_eq!(receipt, (1, 0));
        assert_eq!(symbols[0].selection_line, 1);
        assert_eq!(symbols[0].selection_character, 9);
        assert_eq!(symbols[0].range_start_line, 1);
        assert_eq!(symbols[0].range_start_character, 4);
        assert_eq!(symbols[0].range_end_line, 1);
        assert_eq!(symbols[0].range_end_character, 23);
    }

    #[test]
    fn valid_java_declaration_repairs_an_outside_use_to_its_unique_declared_name() {
        let source = "class Box {\n    void run() {}\n    void other() { run(); }\n}\n";
        let mut symbols = vec![LspSymbol {
            name: "run()".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 1,
            range_start_character: 4,
            range_end_line: 1,
            range_end_character: 17,
            selection_line: 2,
            selection_character: 19,
            parent: None,
        }];

        let receipt = repair_java_lsp_symbol_selections(source, &mut symbols);

        assert_eq!(receipt, (1, 0));
        assert_eq!(symbols[0].selection_line, 1);
        assert_eq!(symbols[0].selection_character, 9);
    }

    #[test]
    fn malformed_java_selection_is_rejected_when_source_repair_is_ambiguous() {
        let source = "class Box {\n    void run() { run(); }\n}\n";
        let mut symbols = vec![LspSymbol {
            name: "run()".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 1,
            range_start_character: 4,
            range_end_line: 1,
            range_end_character: 25,
            selection_line: 0,
            selection_character: 0,
            parent: None,
        }];

        let receipt = repair_java_lsp_symbol_selections(source, &mut symbols);

        assert_eq!(receipt, (0, 1));
        assert!(symbols.is_empty());
    }

    #[test]
    fn language_native_receiver_and_impl_owners_resolve_to_real_types() {
        fn symbol(name: &str, kind: u32, line: u32) -> LspSymbol {
            LspSymbol {
                name: name.to_string(),
                kind,
                detail: None,
                range_start_line: line,
                range_start_character: 0,
                range_end_line: line,
                range_end_character: 10,
                selection_line: line,
                selection_character: 1,
                parent: None,
            }
        }

        let mut go = vec![
            symbol("Box[T]", 23, 1),
            symbol("(Box[T]).Get", 6, 5),
        ];
        reconcile_lsp_symbol_owners("go", &mut go);
        assert_eq!(go[1].parent.as_ref().unwrap().name, "Box[T]");

        let mut rust = vec![symbol("User", 23, 1), symbol("id", 6, 5)];
        rust[1].parent = Some(LspSymbolParent {
            name: "impl Entity for User".to_string(),
            selection_line: 4,
            selection_character: 0,
        });
        reconcile_lsp_symbol_owners("rust", &mut rust);
        assert_eq!(rust[1].parent.as_ref().unwrap().name, "User");
    }

    #[test]
    fn rust_reference_enrichment_is_enabled_by_default() {
        assert!(lsp_reference_enrichment_enabled("rust"));
        assert!(!lsp_reference_enrichment_enabled("typescript"));
    }

    #[test]
    fn rust_public_impl_methods_remain_large_workspace_map_boundaries() {
        let source = "pub struct Runtime;\nimpl Runtime {\n    pub fn spawn(&self) {}\n    fn private(&self) {}\n}\n";
        let public_method = LspSymbol {
            name: "spawn".to_string(),
            kind: 6,
            detail: Some("fn(&self)".to_string()),
            range_start_line: 2,
            range_start_character: 4,
            range_end_line: 2,
            range_end_character: 26,
            selection_line: 2,
            selection_character: 11,
            parent: None,
        };
        let mut private_method = public_method.clone();
        private_method.name = "private".to_string();
        private_method.range_start_line = 3;
        private_method.range_end_line = 3;
        private_method.selection_line = 3;

        assert!(rust_large_symbol_is_public(source, &public_method));
        assert!(!rust_large_symbol_is_public(source, &private_method));
    }

    #[test]
    fn java_large_workspace_boundary_uses_real_modifier_tokens() {
        let source = "class Service {\n\tprivate void hidden() {}\n\tpublic void exposed() {}\n}\n";
        let mut symbol = LspSymbol {
            name: "hidden".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 1,
            range_start_character: 1,
            range_end_line: 1,
            range_end_character: 26,
            selection_line: 1,
            selection_character: 14,
            parent: None,
        };
        assert!(!large_symbol_is_map_boundary("java", source, &symbol));
        symbol.name = "exposed".to_string();
        symbol.range_start_line = 2;
        symbol.range_end_line = 2;
        symbol.selection_line = 2;
        assert!(large_symbol_is_map_boundary("java", source, &symbol));
    }

    #[test]
    fn java_large_call_budget_prioritizes_production_public_api() {
        let source = "class Service {\n\tpublic void execute() {}\n}\n";
        let symbol = LspSymbol {
            name: "execute".to_string(),
            kind: 6,
            detail: None,
            range_start_line: 1,
            range_start_character: 1,
            range_end_line: 1,
            range_end_character: 25,
            selection_line: 1,
            selection_character: 13,
            parent: None,
        };
        assert_eq!(
            large_symbol_call_priority("java", "src/main/java/Service.java", source, &symbol),
            0
        );
        assert_eq!(
            large_symbol_call_priority("java", "src/test/java/ServiceTests.java", source, &symbol),
            2
        );
    }

    #[test]
    fn java_large_call_budget_round_robins_modules_before_depth() {
        let candidate = |priority, group: &str, uri: &str, line| LargeCallSiteQuery {
            priority,
            group: group.to_string(),
            uri: uri.to_string(),
            line,
            character: 1,
        };
        let selected = fair_large_call_site_queries(
            vec![
                candidate(0, "spring-aop", "file:///spring-aop/A.java", 1),
                candidate(0, "spring-aop", "file:///spring-aop/A.java", 2),
                candidate(0, "spring-webmvc", "file:///spring-webmvc/W.java", 1),
                candidate(0, "spring-webmvc", "file:///spring-webmvc/W.java", 2),
                candidate(1, "spring-core", "file:///spring-core/C.java", 1),
            ],
            3,
        );

        assert_eq!(
            selected,
            vec![
                ("file:///spring-aop/A.java".to_string(), 1, 1),
                ("file:///spring-core/C.java".to_string(), 1, 1),
                ("file:///spring-webmvc/W.java".to_string(), 1, 1),
            ]
        );
    }

    #[test]
    fn java_large_call_budget_seeds_a_lower_priority_file_before_more_depth() {
        let candidate = |priority, group: &str, uri: &str, line| LargeCallSiteQuery {
            priority,
            group: group.to_string(),
            uri: uri.to_string(),
            line,
            character: 1,
        };
        let selected = fair_large_call_site_queries(
            vec![
                candidate(0, "Public.java", "file:///Public.java", 1),
                candidate(0, "Public.java", "file:///Public.java", 2),
                candidate(1, "Package.java", "file:///Package.java", 1),
            ],
            2,
        );

        assert_eq!(
            selected,
            vec![
                ("file:///Package.java".to_string(), 1, 1),
                ("file:///Public.java".to_string(), 1, 1),
            ]
        );
    }

    #[test]
    fn java_large_call_order_accepts_the_unbounded_budget_sentinel() {
        let candidates = vec![
            LargeCallSiteQuery {
                priority: 0,
                group: "A.java".to_string(),
                uri: "file:///A.java".to_string(),
                line: 1,
                character: 1,
            },
            LargeCallSiteQuery {
                priority: 1,
                group: "B.java".to_string(),
                uri: "file:///B.java".to_string(),
                line: 2,
                character: 1,
            },
        ];

        let selected = fair_large_call_site_queries(candidates, usize::MAX);

        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn java_provider_labels_reduce_to_source_call_names_without_guessing() {
        assert_eq!(lsp_symbol_base_name("doDispatch(HttpServletRequest)"), "doDispatch");
        assert_eq!(lsp_symbol_base_name("Box<T>"), "Box");
        assert_eq!(lsp_symbol_base_name("main"), "main");
    }

    #[test]
    fn large_call_group_keeps_monorepo_packages_independent() {
        assert_eq!(
            large_call_query_group("packages/platform-ws/src/server.ts"),
            "packages/platform-ws"
        );
        assert_eq!(
            large_call_query_group("spring-webmvc/src/main/java/Dispatcher.java"),
            "spring-webmvc"
        );
        assert_eq!(large_call_query_group("src/main/java/App.java"), ".");
    }

    #[test]
    fn java_large_workspace_local_override_requires_an_explicit_annotation() {
        let annotated = "class Child extends Parent {\n\t@Override\n\tpublic void execute(String value) {}\n}\n";
        let mut symbol = LspSymbol {
            name: "execute(String)".to_string(),
            kind: 6,
            detail: Some(" : void".to_string()),
            range_start_line: 1,
            range_start_character: 1,
            range_end_line: 2,
            range_end_character: 38,
            selection_line: 2,
            selection_character: 13,
            parent: None,
        };
        assert!(java_explicit_override_annotation(annotated, &symbol));

        let unannotated =
            "class Child extends Parent {\n\tpublic void execute(String value) {}\n}\n";
        symbol.range_start_line = 1;
        symbol.range_end_line = 1;
        symbol.selection_line = 1;
        assert!(!java_explicit_override_annotation(unannotated, &symbol));

        let comment_only = "class Child extends Parent {\n\t// @Override\n\tpublic void execute(String value) {}\n}\n";
        symbol.range_start_line = 1;
        symbol.range_end_line = 2;
        symbol.selection_line = 2;
        assert!(!java_explicit_override_annotation(comment_only, &symbol));
    }

    #[test]
    fn semantic_query_volume_promotes_few_large_files_to_large_workspace_mode() {
        assert!(!large_workspace_workload("rust-analyzer", 62, 500));
        assert!(large_workspace_workload("rust-analyzer", 62, 501));
        assert!(large_workspace_workload("gopls", 251, 0));
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_uri_drive_case_is_relative_to_the_workspace() {
        assert_eq!(
            uri_to_relative_path("file:///d:/Project/src/App.java", Path::new(r"D:\Project")),
            "src/App.java"
        );
    }

    #[test]
    fn bundled_java_home_supports_a_jdtls_bin_launcher() {
        let root =
            std::env::temp_dir().join(format!("code-memory-jdtls-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let launcher = root.join("jdtls").join("bin").join("jdtls.cmd");
        let runtime_bin = root.join("jdtls").join("runtime").join("bin");
        fs::create_dir_all(launcher.parent().expect("launcher parent")).expect("create launcher");
        fs::create_dir_all(&runtime_bin).expect("create runtime");
        fs::write(&launcher, b"launcher").expect("write launcher");
        assert_eq!(
            bundled_java_home(&launcher),
            Some(root.join("jdtls/runtime"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn java_source_only_settings_disable_build_importers() {
        let settings = java_language_server_settings_for_network(true, false, None);
        assert_eq!(
            settings.pointer("/java/import/gradle/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            settings.pointer("/java/import/maven/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            settings.pointer("/java/project/importOnFirstTimeStartup"),
            Some(&serde_json::Value::String("disabled".to_string()))
        );
        assert_eq!(
            settings.pointer("/java/diagnostic/filter/0"),
            Some(&serde_json::Value::String("**/*.java".to_string()))
        );
        assert_eq!(
            settings.pointer("/java/edit/validateAllOpenBuffersOnChanges"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn java_network_opt_in_reaches_gradle_and_maven_importers() {
        let offline = java_language_server_settings_for_network(false, false, None);
        assert_eq!(
            offline.pointer("/java/import/gradle/offline/enabled"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            offline.pointer("/java/import/gradle/wrapper/enabled"),
            Some(&serde_json::Value::Bool(false))
        );

        let online = java_language_server_settings_for_network(false, true, None);
        assert_eq!(
            online.pointer("/java/import/gradle/offline/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            online.pointer("/java/import/gradle/wrapper/enabled"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            online.pointer("/java/import/maven/offline/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            online.pointer("/java/import/gradle/arguments"),
            Some(&serde_json::Value::String("-x test".to_string()))
        );
    }

    #[test]
    fn java_managed_toolchains_reach_gradle_model_import() {
        let settings = java_language_server_settings_for_network(
            false,
            true,
            Some(r"C:\managed\jdk-21,C:\managed\jdk-25"),
        );
        assert_eq!(
            settings.pointer("/java/import/gradle/arguments"),
            Some(&serde_json::Value::String(
                "-x test -Porg.gradle.java.installations.paths=C:/managed/jdk-21,C:/managed/jdk-25"
                    .to_string()
            ))
        );
    }

    #[test]
    fn java_managed_toolchains_register_jdtls_system_libraries() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-java-runtimes-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let executable = if cfg!(windows) { "java.exe" } else { "java" };
        for (directory, version) in [("jdk-21", "21.0.11"), ("jdk-25", "25.0.4")] {
            let home = root.join(directory);
            fs::create_dir_all(home.join("bin")).expect("create managed JDK");
            fs::write(home.join("bin").join(executable), b"launcher")
                .expect("write managed Java launcher");
            fs::write(
                home.join("release"),
                format!("JAVA_VERSION=\"{version}\"\n"),
            )
            .expect("write managed JDK release metadata");
        }
        let paths = format!(
            "{},{}",
            root.join("jdk-21").display(),
            root.join("jdk-25").display()
        );
        let runtimes = java_configuration_runtimes(Some(&paths));
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0]["name"], "JavaSE-21");
        assert_eq!(runtimes[0]["default"], false);
        assert_eq!(runtimes[1]["name"], "JavaSE-25");
        assert_eq!(runtimes[1]["default"], true);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn jdtls_heap_scales_with_real_source_volume_and_respects_memory_budget() {
        assert_eq!(jdtls_heap_mb(0, Some(8_192)), 1_024);
        assert_eq!(jdtls_heap_mb(8_982, Some(6_546)), 4_018);
        assert_eq!(jdtls_heap_mb(100_000, Some(8_192)), 6_144);
        assert_eq!(jdtls_heap_mb(8_982, Some(1_024)), 768);
    }

    #[test]
    fn rust_restart_only_settings_are_sent_during_initialize() {
        let options = initialization_options("rust", &rust_analyzer_settings());
        assert_eq!(
            options.pointer("/cargo/sysroot"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(
            options.pointer("/checkOnSave/enable"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn java_home_requires_a_real_launcher() {
        let root =
            std::env::temp_dir().join(format!("code-memory-java-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(!java_home_is_usable(&root));
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create java bin");
        let executable = if cfg!(windows) { "java.exe" } else { "java" };
        fs::write(bin.join(executable), b"launcher").expect("write java launcher");
        assert!(java_home_is_usable(&root));
        let _ = fs::remove_dir_all(root);
    }
}
