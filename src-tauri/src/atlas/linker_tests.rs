#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atlas::model::{InventorySnapshot, SnapshotMetadata},
        workspace::FocusedCodeSearchTotals,
    };

    #[test]
    fn tokens_respect_camel_case_and_compound_table_names() {
        assert_eq!(
            identifier_tokens("src/orderItems/HTTPOrderRepository.ts"),
            ["src", "order", "items", "http", "order", "repository", "ts"]
        );
        assert!(identifier_terms("OrderItemRepository").contains("order_item"));
        assert!(!identifier_terms("OrderRepository").contains("order_item"));
    }

    #[test]
    fn pluralization_is_bounded_and_does_not_corrupt_status() {
        assert_eq!(singularize("orders"), "order");
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("statuses"), "status");
        assert_eq!(singularize("status"), "status");
        assert_eq!(singularize("analysis"), "analysis");
    }

    #[test]
    fn generic_single_terms_are_not_candidate_evidence() {
        assert!(is_generic_term("data"));
        assert!(!is_generic_term("order_item"));
        assert!(!is_generic_term("orders"));
    }

    #[test]
    fn large_candidate_inventory_is_bounded_per_code_item() {
        let mut items = (0..200)
            .map(|index| {
                test_item(
                    format!("db:table:domain_{index}_records"),
                    "table",
                    format!("domain_{index}_records"),
                    "db",
                    None,
                )
            })
            .collect::<Vec<_>>();
        items.extend((0..10_000).map(|index| {
            let table_index = index % 200;
            test_item(
                format!("code:repository:{index}"),
                "repository",
                format!("domain_{table_index}_record_repository"),
                "code",
                Some(format!("src/domain_{table_index}/repository_{index}.rs")),
            )
        }));
        let snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "large-candidate-fixture".to_string(),
            saved_at: "0".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items,
        };

        let links = candidate_links(&snapshot);

        assert_eq!(links.len(), 10_000);
        assert!(links.iter().all(|link| link.confidence == "high"));
    }

    #[test]
    fn candidate_cache_reuses_base_and_enriched_snapshot_variants() {
        let code = test_item(
            "code:repository:orders".to_string(),
            "repository",
            "OrderRepository".to_string(),
            "code",
            Some("src/orders/repository.rs".to_string()),
        );
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "candidate-cache-fixture".to_string(),
            saved_at: "1".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![
                code,
                test_item(
                    "db:table:orders".to_string(),
                    "table",
                    "orders".to_string(),
                    "db",
                    None,
                ),
                test_item(
                    "db:table:payments".to_string(),
                    "table",
                    "payments".to_string(),
                    "db",
                    None,
                ),
            ],
        };

        let first = candidate_links(&snapshot);
        let second = candidate_links(&snapshot);
        assert!(Arc::ptr_eq(&first, &second));

        snapshot.items[1].name = "unrelated".to_string();
        let renamed = candidate_links(&snapshot);
        assert!(!Arc::ptr_eq(&first, &renamed));
        assert!(!renamed.iter().any(|link| link.to == "db:table:orders"));

        snapshot.links.push(SnapshotLink {
            id: "text:orders->payments".to_string(),
            from: "code:repository:orders".to_string(),
            to: "db:table:payments".to_string(),
            kind: "code_db_text_reference".to_string(),
            label: None,
            truth_class: "candidate".to_string(),
            direction: "outbound".to_string(),
            engine_edge_type: None,
            evidence: vec![Evidence {
                kind: "code-search-exact-token".to_string(),
                text: "payments exact token".to_string(),
            }],
        });
        let enriched = candidate_links(&snapshot);
        let enriched_again = candidate_links(&snapshot);

        assert!(!Arc::ptr_eq(&first, &enriched));
        assert!(Arc::ptr_eq(&enriched, &enriched_again));
        assert!(enriched.iter().any(|link| link.to == "db:table:payments"));

        invalidate_candidate_links(&snapshot.workspace_id);
    }

    #[test]
    fn focused_search_evidence_merges_as_high_candidate_without_source_body() {
        let mut code = test_item(
            "code:repo.loadOrders".to_string(),
            "repository",
            "OrderRepository".to_string(),
            "code",
            Some("src/orders/repository.ts".to_string()),
        );
        code.qualified_name = Some("repo.loadOrders".to_string());
        code.engine_label = Some("Function".to_string());
        code.location = Some(super::SourceLocation {
            path: "src/orders/repository.ts".to_string(),
            line: Some(10),
            column: Some(2),
            end_line: Some(20),
            end_column: Some(8),
        });
        let table = test_item(
            "db:table:orders".to_string(),
            "table",
            "orders".to_string(),
            "db",
            None,
        );
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "focused-evidence".to_string(),
            saved_at: "0".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![code, table],
        };
        let search = focused_search(
            vec![FocusedCodeSearchMatch {
                qualified_name: "repo.loadOrders".to_string(),
                label: "Function".to_string(),
                file: "src/orders/repository.ts".to_string(),
                start_line: 10,
                end_line: 20,
                match_lines: vec![14],
            }],
            Vec::new(),
        );

        let applied = apply_focused_code_evidence(&mut snapshot, "db:table:orders", &search, false);

        assert_eq!(applied.matched_files, ["src/orders/repository.ts"]);
        assert_eq!(snapshot.items[0].location.as_ref().unwrap().line, Some(14));
        assert_eq!(snapshot.links.len(), 1);
        assert_eq!(snapshot.links[0].kind, "code_db_text_reference");
        assert_eq!(snapshot.links[0].truth_class, "candidate");
        let candidates = candidate_links(&snapshot);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, "high");
        assert!(candidates[0]
            .evidence
            .iter()
            .any(|entry| entry.kind == "code-search-exact-token"));
        assert!(!serde_json::to_string(&snapshot)
            .unwrap()
            .contains("SELECT * FROM orders"));
    }

    #[test]
    fn duplicate_qualified_names_require_the_exact_file_and_range() {
        let mut first = test_item(
            "code:first.load".to_string(),
            "repository",
            "load".to_string(),
            "code",
            Some("src/first.ts".to_string()),
        );
        first.qualified_name = Some("repo.load".to_string());
        first.engine_label = Some("Function".to_string());
        first.location = Some(super::SourceLocation {
            path: "src/first.ts".to_string(),
            line: Some(4),
            column: None,
            end_line: Some(8),
            end_column: None,
        });
        let mut second = first.clone();
        second.id = "code:second.load".to_string();
        second.path = Some("src/second.ts".to_string());
        second.location = Some(super::SourceLocation {
            path: "src/second.ts".to_string(),
            line: Some(20),
            column: None,
            end_line: Some(30),
            end_column: None,
        });
        let table = test_item(
            "db:table:orders".to_string(),
            "table",
            "orders".to_string(),
            "db",
            None,
        );
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "duplicate-qualified".to_string(),
            saved_at: "0".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![first, second, table],
        };
        let search = focused_search(
            vec![FocusedCodeSearchMatch {
                qualified_name: "repo.load".to_string(),
                label: "Function".to_string(),
                file: "src/second.ts".to_string(),
                start_line: 20,
                end_line: 30,
                match_lines: vec![24],
            }],
            Vec::new(),
        );

        apply_focused_code_evidence(&mut snapshot, "db:table:orders", &search, false);

        assert_eq!(snapshot.links.len(), 1);
        assert_eq!(snapshot.links[0].from, "code:second.load");
    }

    #[test]
    fn focused_column_evidence_stays_candidate_and_reaches_the_read_model() {
        let mut code = test_item(
            "code:repo.loadOrders".to_string(),
            "repository",
            "OrderRepository".to_string(),
            "code",
            Some("src/orders/repository.ts".to_string()),
        );
        code.qualified_name = Some("repo.loadOrders".to_string());
        code.engine_label = Some("Function".to_string());
        code.location = Some(super::SourceLocation {
            path: "src/orders/repository.ts".to_string(),
            line: Some(10),
            column: None,
            end_line: Some(20),
            end_column: None,
        });
        let mut column = test_item(
            "db:column:orders:status".to_string(),
            "column",
            "status".to_string(),
            "db",
            None,
        );
        column.parent_id = Some("db:table:orders".to_string());
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "focused-column-evidence".to_string(),
            saved_at: "0".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![code, column],
        };
        let search = focused_search(
            vec![FocusedCodeSearchMatch {
                qualified_name: "repo.loadOrders".to_string(),
                label: "Function".to_string(),
                file: "src/orders/repository.ts".to_string(),
                start_line: 10,
                end_line: 20,
                match_lines: vec![16],
            }],
            Vec::new(),
        );

        apply_focused_code_evidence(&mut snapshot, "db:column:orders:status", &search, false);

        assert_eq!(snapshot.links[0].truth_class, "candidate");
        assert_eq!(snapshot.links[0].kind, "code_db_column_text_reference");
        let candidates = candidate_links(&snapshot);
        assert!(candidates.iter().any(|link| {
            link.from == "code:repo.loadOrders"
                && link.to == "db:column:orders:status"
                && link.confidence == "high"
        }));
    }

    #[test]
    fn unicode_mapping_is_location_unique_and_schema_ambiguity_caps_confidence() {
        let mut code = test_item(
            "code:서비스.주문조회".to_string(),
            "repository",
            "OrderRepository".to_string(),
            "code",
            Some("src/주문.rs".to_string()),
        );
        code.qualified_name = Some("서비스.주문조회".to_string());
        code.engine_label = Some("Function".to_string());
        code.location = Some(super::SourceLocation {
            path: "src/주문.rs".to_string(),
            line: Some(10),
            column: None,
            end_line: Some(20),
            end_column: None,
        });
        let tables = ["public", "audit"].map(|schema| {
            test_item(
                format!("db:table:{schema}:orders"),
                "table",
                "orders".to_string(),
                "db",
                None,
            )
        });
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "unicode-evidence".to_string(),
            saved_at: "0".to_string(),
            metadata: SnapshotMetadata::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![code, tables[0].clone(), tables[1].clone()],
        };
        let search = focused_search(
            vec![FocusedCodeSearchMatch {
                qualified_name: "?????????.????????????".to_string(),
                label: "Function".to_string(),
                file: "src/??????.rs".to_string(),
                start_line: 10,
                end_line: 20,
                match_lines: vec![13],
            }],
            vec!["result-limit".to_string()],
        );

        apply_focused_code_evidence(&mut snapshot, "db:table:public:orders", &search, true);

        let candidate = candidate_links(&snapshot)
            .iter()
            .find(|link| link.to == "db:table:public:orders")
            .cloned()
            .unwrap();
        assert_eq!(candidate.confidence, "medium");
        assert!(candidate
            .evidence
            .iter()
            .any(|entry| entry.kind == "code-search-schema-ambiguous"));
        assert!(snapshot
            .metadata
            .gaps
            .iter()
            .any(|gap| gap.kind == "code-search-partial"));
    }

    fn focused_search(
        matches: Vec<FocusedCodeSearchMatch>,
        partial_reasons: Vec<String>,
    ) -> FocusedCodeSearch {
        let totals = FocusedCodeSearchTotals {
            returned: matches.len(),
            total_results: matches.len(),
            total_grep_matches: matches.len(),
            raw_match_count: 0,
        };
        FocusedCodeSearch {
            matches,
            totals,
            partial: !partial_reasons.is_empty(),
            partial_reasons,
        }
    }

    fn test_item(
        id: String,
        kind: &str,
        name: String,
        source: &str,
        path: Option<String>,
    ) -> InventoryItem {
        InventoryItem {
            id,
            kind: kind.to_string(),
            name,
            layer: source.to_string(),
            source: source.to_string(),
            parent_id: None,
            path,
            qualified_name: None,
            engine_label: None,
            language: None,
            role_basis: None,
            project_id: None,
            group_id: None,
            location: None,
            is_primary_key: false,
            is_foreign_key: false,
            nullable: None,
        }
    }
}
