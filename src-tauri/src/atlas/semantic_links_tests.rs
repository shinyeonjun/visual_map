#[cfg(test)]
mod tests {
    use super::sql_parser::QueryOperation;
    use super::*;

    #[test]
    fn discovers_static_sql_from_selected_code_without_name_candidates() {
        let root =
            std::env::temp_dir().join(format!("backend-map-semantic-links-{}", std::process::id()));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("repository.ts"),
            "function load() {\n  return db.query(\"SELECT id, status FROM public.orders\");\n}\n",
        )
        .unwrap();
        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![
                inventory_item(
                    "code:load",
                    "function",
                    "load",
                    "code",
                    None,
                    Some("src/repository.ts"),
                    None,
                ),
                inventory_item(
                    "db:table:public.orders",
                    "table",
                    "orders",
                    "db",
                    None,
                    None,
                    Some("public"),
                ),
                inventory_item(
                    "db:column:public.orders:id",
                    "column",
                    "id",
                    "db",
                    Some("db:table:public.orders"),
                    None,
                    Some("public"),
                ),
                inventory_item(
                    "db:column:public.orders:status",
                    "column",
                    "status",
                    "db",
                    Some("db:table:public.orders"),
                    None,
                    Some("public"),
                ),
            ],
        };
        snapshot.items[0].location = Some(super::super::model::SourceLocation {
            path: "src/repository.ts".to_string(),
            line: Some(1),
            column: None,
            end_line: Some(3),
            end_column: None,
        });

        let count = apply_explicit_query_evidence_for_code(
            &mut snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );

        assert_eq!(count, 1);
        let table_link = snapshot
            .links
            .iter()
            .find(|link| {
                link.from == "code:load"
                    && link.to == "db:table:public.orders"
                    && link.kind == "code_db_read"
                    && link.is_confirmed()
            })
            .unwrap();
        assert!(table_link.evidence[0].text.contains("repository.ts:L2"));
        assert_eq!(
            snapshot
                .links
                .iter()
                .filter(|link| link.kind == "code_db_uses_column")
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_static_sql_when_snapshot_has_no_matching_table() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-semantic-empty-table-{}",
            std::process::id()
        ));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("repository.ts"),
            "function load() { return db.query(\"SELECT id FROM missing_orders\"); }\n",
        )
        .unwrap();

        let mut snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace-empty-table".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![inventory_item(
                "code:load",
                "function",
                "load",
                "code",
                None,
                Some("src/repository.ts"),
                None,
            )],
        };
        snapshot.items[0].location = Some(super::super::model::SourceLocation {
            path: "src/repository.ts".to_string(),
            line: Some(1),
            column: None,
            end_line: Some(1),
            end_column: None,
        });

        let count = apply_explicit_query_evidence_for_code(
            &mut snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );

        assert_eq!(count, 0);
        assert!(snapshot.links.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirms_static_select_and_exact_columns() {
        let result = analyze_source(
            r#"const sql = "SELECT id, status FROM public.orders WHERE id = ?";
               return connection.query(sql, params);"#,
            "orders",
            Some("public"),
            &["id", "status", "created_at"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].operation, QueryOperation::Select);
        assert_eq!(
            result[0].columns,
            BTreeSet::from(["id".to_string(), "status".to_string()])
        );
    }

    #[test]
    fn confirms_static_update_as_write() {
        let result = analyze_source(
            r#"jdbcTemplate.execute("UPDATE orders SET status = ? WHERE id = ?");"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result[0].operation, QueryOperation::Update);
        assert_eq!(result[0].operation.edge_type(), "WRITES");
    }

    #[test]
    fn confirms_inline_generic_execution_call() {
        let result = analyze_source(
            r#"return connection.QueryAsync<Order>("SELECT id FROM orders");"#,
            "orders",
            None,
            &["id"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].operation, QueryOperation::Select);
    }

    #[test]
    fn confirms_static_sql_across_common_framework_execution_apis() {
        for (source, operation) in [
            (
                r#"return connection.QuerySingleAsync<Order>("SELECT id, status FROM orders WHERE id = @id");"#,
                QueryOperation::Select,
            ),
            (
                r#"return connection.QuerySingleAsync<Order>(@"SELECT id, status FROM orders WHERE id = @id");"#,
                QueryOperation::Select,
            ),
            (
                r#"context.Database.ExecuteSqlRaw("UPDATE orders SET status = ? WHERE id = ?");"#,
                QueryOperation::Update,
            ),
            (
                r#"jdbcTemplate.queryForObject("SELECT id FROM orders WHERE id = ?", mapper, id);"#,
                QueryOperation::Select,
            ),
            (
                r#"session.execute(text("SELECT id FROM orders WHERE id = :id"), params)"#,
                QueryOperation::Select,
            ),
            (
                r##"sqlx::query!(r#"SELECT "id", status FROM orders WHERE status = 'ready'"#);"##,
                QueryOperation::Select,
            ),
        ] {
            let result = analyze_source(
                source,
                "orders",
                None,
                &["id", "status", "created_at"],
                false,
            );
            assert_eq!(
                result.len(),
                1,
                "explicit static SQL should be confirmed: {source}"
            );
            assert_eq!(result[0].operation, operation);
        }

        for source in [
            r#"reporter.QuerySingle("SELECT id FROM orders")"#,
            r#"session.execute(text(prefix + "SELECT id FROM orders"))"#,
            r#"session.execute(text("SELECT id FROM orders" + suffix))"#,
            r#"session.execute(render("SELECT id FROM orders"))"#,
            r#"const query = sql`SELECT id FROM orders`; db.query(query);"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id"], false).is_empty(),
                "non-evidence execution form must stay unconfirmed: {source}"
            );
        }
    }

    #[test]
    fn sql_parser_baseline_corpus_stays_measurable() {
        let accepted = [
            r#"db.query("SELECT id FROM orders")"#,
            r#"jdbcTemplate.execute("UPDATE orders SET status = ?")"#,
            r#"session.execute(text("DELETE FROM orders WHERE id = :id"))"#,
            r#"connection.QuerySingleAsync<Order>("SELECT id FROM orders")"#,
            r##"sqlx::query!(r#"SELECT id FROM orders"#);"##,
            r#"sqlite3_exec(db, "SELECT id FROM orders", callback, 0, error);"#,
            r#"pdo->query("SELECT id FROM orders");"#,
        ];
        let rejected = [
            r#"db.query("SELECT id FROM orders " + suffix)"#,
            r#"cursor.execute(f"SELECT id FROM {table_name}")"#,
            r#"db.query("WITH selected AS (SELECT id FROM orders) SELECT id FROM selected")"#,
            r#"db.query("SELECT id FROM orders; DELETE FROM orders")"#,
            r#"db.query("SELECT id FROM orders, users")"#,
            r#"logger.raw("SELECT id FROM orders")"#,
        ];
        let accepted_count = accepted
            .iter()
            .filter(|source| !super::sql_parser::parse_source(source).is_empty())
            .count();
        let rejected_count = rejected
            .iter()
            .filter(|source| !super::sql_parser::parse_source(source).is_empty())
            .count();
        assert_eq!(accepted_count, accepted.len());
        assert_eq!(rejected_count, 0);
        assert_eq!(accepted.len() as f32 / (accepted.len() + rejected.len()) as f32, 0.53846157);
    }

    #[test]
    fn confirms_static_sql_for_every_active_language_shape() {
        let cases = [
            ("typescript", r#"db.query("SELECT id FROM orders")"#),
            ("javascript", r#"db.query("SELECT id FROM orders")"#),
            (
                "python",
                r#"session.execute(text("SELECT id FROM orders"))"#,
            ),
            (
                "java",
                r#"jdbcTemplate.queryForObject("SELECT id FROM orders", mapper);"#,
            ),
            (
                "csharp",
                r#"connection.QuerySingleAsync<Order>("SELECT id FROM orders");"#,
            ),
            (
                "c",
                r#"sqlite3_exec(db, "SELECT id FROM orders", callback, 0, error);"#,
            ),
            (
                "cpp",
                r#"sqlite3_exec(db, "SELECT id FROM orders", callback, 0, error);"#,
            ),
            ("go", r#"db.Query("SELECT id FROM orders")"#),
            ("rust", r##"sqlx::query!(r#"SELECT id FROM orders"#);"##),
            ("php", r#"$pdo->query("SELECT id FROM orders");"#),
            ("ruby", r#"connection.query("SELECT id FROM orders")"#),
            ("dart", r#"db.query("SELECT id FROM orders");"#),
        ];

        let root = std::env::temp_dir().join(format!(
            "backend-map-language-db-shapes-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();

        for (language, source) in cases {
            let result = analyze_source(source, "orders", None, &["id"], false);
            assert_eq!(
                result.len(),
                1,
                "static SQL should be confirmed for {language}: {source}"
            );
            assert_eq!(result[0].operation, QueryOperation::Select);
            assert_eq!(result[0].columns, BTreeSet::from(["id".to_string()]));

            let path = format!("src/{language}.source");
            std::fs::write(root.join(&path), source).unwrap();
            let code_id = format!("code:{language}:load");
            let table_id = format!("db:table:{language}:orders");
            let column_id = format!("db:column:{language}:orders:id");
            let mut snapshot = InventorySnapshot {
                schema_version: 2,
                workspace_id: format!("workspace-{language}"),
                saved_at: "1".to_string(),
                metadata: Default::default(),
                stale_reasons: Vec::new(),
                links: Vec::new(),
                items: vec![
                    inventory_item(
                        &code_id,
                        "function",
                        "load",
                        "code",
                        None,
                        Some(&path),
                        None,
                    ),
                    inventory_item(&table_id, "table", "orders", "db", None, None, None),
                    inventory_item(
                        &column_id,
                        "column",
                        "id",
                        "db",
                        Some(&table_id),
                        None,
                        None,
                    ),
                ],
            };
            snapshot.items[0].location = Some(super::super::model::SourceLocation {
                path: path.clone(),
                line: Some(1),
                column: None,
                end_line: Some(1),
                end_column: None,
            });
            let count = apply_explicit_query_evidence_for_code(
                &mut snapshot,
                root.to_str().unwrap(),
                std::slice::from_ref(&code_id),
            );
            assert_eq!(
                count, 1,
                "exact table join should be confirmed for {language}"
            );
            assert!(snapshot.links.iter().any(|link| {
                link.from == code_id
                    && link.to == table_id
                    && link.kind == "code_db_read"
                    && link.is_confirmed()
            }));
            assert!(snapshot.links.iter().any(|link| {
                link.from == code_id
                    && link.to == column_id
                    && link.kind == "code_db_uses_column"
                    && link.is_confirmed()
            }));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_dynamic_or_commented_sql() {
        assert!(analyze_source(
            r#"cursor.execute(f"SELECT id FROM {table_name}")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        for source in [
            r#"db.query("SELECT id FROM orders " + whereClause)"#,
            r#"const sql = "SELECT id FROM orders " + whereClause; db.query(sql);"#,
            "const sql = \"SELECT id FROM orders \"\n  + whereClause;\ndb.query(sql);",
            r#"db.query(prefix + "SELECT id FROM orders")"#,
            r#"db.query("SELECT ${column} FROM orders")"#,
            r##"db.query("SELECT #{column} FROM orders")"##,
            r#"db.query("SELECT $column FROM orders")"#,
            r#"const sql = "SELECT id FROM orders"; db.query(sql + whereClause)"#,
            r#"const sql = "SELECT id FROM orders"; db.query(prefix + sql)"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id"], false).is_empty(),
                "dynamic SQL must not become confirmed: {source}"
            );
        }
        assert!(analyze_source(
            r#"// connection.query("SELECT id FROM orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn rejects_unrelated_sql_literal_near_an_execution_call() {
        assert!(analyze_source(
            r#"const help = "SELECT id FROM orders";
               logger.info(help);
               return connection.query(otherSql);"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"connection.query(otherSql);
               const help = "SELECT id FROM orders";"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn ignores_sql_string_values_that_match_column_names() {
        let result = analyze_source(
            r#"db.query("SELECT id FROM orders WHERE name = 'status'")"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result[0].columns, BTreeSet::from(["id".to_string()]));
    }

    #[test]
    fn ignores_projection_aliases_that_match_real_columns() {
        let result = analyze_source(
            r#"db.query("SELECT count(*) AS id, status AS state, 'fixed' name FROM orders")"#,
            "orders",
            None,
            &["id", "status", "state", "name"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].columns, BTreeSet::from(["status".to_string()]));
    }

    #[test]
    fn fails_closed_for_dialect_projection_clauses_outside_the_bounded_grammar() {
        for source in [
            r#"db.query("SELECT TOP (10) id FROM orders")"#,
            r#"db.query("SELECT TOP @limit id FROM orders")"#,
            r#"db.query("SELECT DISTINCT ON (tenant_id) status FROM orders")"#,
        ] {
            assert!(
                analyze_source(source, "orders", None, &["id", "status"], false).is_empty(),
                "unsupported projection syntax must stay unconfirmed: {source}"
            );
        }
    }

    #[test]
    fn keeps_top_as_a_column_outside_the_sql_server_projection_clause() {
        let selected = analyze_source(
            r#"db.query("SELECT top FROM orders")"#,
            "orders",
            None,
            &["top"],
            false,
        );
        let updated = analyze_source(
            r#"db.execute("UPDATE orders SET top = ? WHERE id = ?")"#,
            "orders",
            None,
            &["id", "top"],
            false,
        );

        assert_eq!(selected[0].columns, BTreeSet::from(["top".to_string()]));
        assert_eq!(
            updated[0].columns,
            BTreeSet::from(["id".to_string(), "top".to_string()])
        );
    }

    #[test]
    fn ignores_named_parameters_that_match_column_names() {
        let result = analyze_source(
            r#"db.query("SELECT id FROM orders WHERE name = :status AND role = @status")"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result[0].columns, BTreeSet::from(["id".to_string()]));
    }

    #[test]
    fn rejects_generic_receivers_and_reassigned_query_variables() {
        assert!(analyze_source(
            r#"logger.raw("SELECT id FROM orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"logger->query("SELECT id FROM orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"let sql = "SELECT id FROM orders";
               sql = buildSql();
               db.query(sql);"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query(format("SELECT id FROM orders"))"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"const sql = "SELECT id FROM orders";
               db.query(transform(sql));"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"const sql = "SELECT id FROM orders";
               db.query("sql");"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query({ text: "SELECT id FROM orders" })"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn assigns_qualified_join_columns_only_to_their_owner() {
        let source = r#"db.query("SELECT users.id, orders.status FROM orders JOIN users ON users.id = orders.user_id")"#;
        let orders = analyze_source(source, "orders", None, &["id", "status", "user_id"], false);
        let users = analyze_source(source, "users", None, &["id"], false);

        assert_eq!(
            orders[0].columns,
            BTreeSet::from(["status".to_string(), "user_id".to_string()])
        );
        assert_eq!(users[0].columns, BTreeSet::from(["id".to_string()]));

        let ambiguous =
            r#"db.query("SELECT id FROM orders JOIN users ON orders.user_id = users.owner_id")"#;
        assert!(
            !analyze_source(ambiguous, "orders", None, &["id"], false)[0]
                .columns
                .contains("id")
        );
    }

    #[test]
    fn accepts_qualified_table_when_duplicate_schemas_exist() {
        assert_eq!(
            analyze_source(
                r#"db.query("SELECT id FROM public.orders")"#,
                "orders",
                Some("public"),
                &["id"],
                true,
            )
            .len(),
            1
        );
        assert!(analyze_source(
            r#"db.query("SELECT id FROM orders")"#,
            "orders",
            Some("public"),
            &["id"],
            true,
        )
        .is_empty());
        assert_eq!(
            analyze_source(
                r#"db.query('SELECT id FROM "public"."orders"')"#,
                "orders",
                Some("public"),
                &["id"],
                true,
            )
            .len(),
            1
        );
    }

    #[test]
    fn separates_read_and_write_targets_in_composite_dml() {
        let source = r#"db.execute("INSERT INTO archived_orders (id) SELECT id FROM orders")"#;
        let target = analyze_source(source, "archived_orders", None, &["id"], false);
        let source_table = analyze_source(source, "orders", None, &["id"], false);

        assert_eq!(target[0].operation, QueryOperation::Insert);
        assert_eq!(source_table[0].operation, QueryOperation::Select);

        let merge = r#"db.execute("MERGE INTO orders AS o USING staged_orders AS s ON o.id = s.id WHEN MATCHED THEN UPDATE SET status = s.status")"#;
        assert_eq!(
            analyze_source(merge, "orders", None, &["id", "status"], false)[0].operation,
            QueryOperation::Merge
        );
        assert_eq!(
            analyze_source(merge, "staged_orders", None, &["id", "status"], false)[0].operation,
            QueryOperation::Select
        );
    }

    #[test]
    fn keeps_insert_column_lists_out_of_alias_detection() {
        let result = analyze_source(
            r#"db.execute("INSERT INTO orders (id, status) VALUES (?, ?)")"#,
            "orders",
            None,
            &["id", "status"],
            false,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].columns,
            BTreeSet::from(["id".to_string(), "status".to_string()])
        );
    }

    #[test]
    fn fails_closed_for_ctes_and_unresolved_join_column_owners() {
        assert!(analyze_source(
            r#"db.query("WITH recent AS (SELECT id FROM orders) SELECT id FROM recent")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());

        let result = analyze_source(
            r#"db.query("SELECT status FROM orders JOIN audit_feed ON audit_feed.order_id = audit_feed.id")"#,
            "orders",
            None,
            &["status"],
            false,
        );
        assert_eq!(result.len(), 1);
        assert!(result[0].columns.is_empty());
    }

    #[test]
    fn fails_closed_for_multi_statement_comma_join_and_table_function_sql() {
        for source in [
            r#"db.query("SELECT id FROM orders; DELETE FROM audit")"#,
            r#"db.query("SELECT id FROM orders, users")"#,
            r#"db.query("SELECT id FROM orders(?)")"#,
        ] {
            assert!(analyze_source(source, "orders", None, &["id"], false).is_empty());
        }
    }

    #[test]
    fn ignores_sql_comments_and_does_not_treat_temp_tables_as_real_tables() {
        assert!(analyze_source(
            "db.query(\"SELECT 1 -- FROM orders\\n\")",
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query("SELECT id FROM #orders")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
        assert!(analyze_source(
            r#"db.query("SELECT id FROM orders # JOIN audit")"#,
            "orders",
            None,
            &["id"],
            false,
        )
        .is_empty());
    }

    #[test]
    fn semantic_cache_signature_changes_with_the_source_file() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-semantic-signature-{}",
            std::process::id()
        ));
        let source_dir = root.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        let path = source_dir.join("repository.ts");
        std::fs::write(&path, "db.query('SELECT id FROM orders')").unwrap();
        let snapshot = InventorySnapshot {
            schema_version: 2,
            workspace_id: "workspace".to_string(),
            saved_at: "1".to_string(),
            metadata: Default::default(),
            stale_reasons: Vec::new(),
            links: Vec::new(),
            items: vec![inventory_item(
                "code:load",
                "function",
                "load",
                "code",
                None,
                Some("src/repository.ts"),
                None,
            )],
        };
        let first = semantic_source_signature(
            &snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );
        std::fs::write(&path, "db.query('SELECT id, status FROM orders')").unwrap();
        let second = semantic_source_signature(
            &snapshot,
            root.to_str().unwrap(),
            &["code:load".to_string()],
        );

        assert_ne!(first, second);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn inventory_item(
        id: &str,
        kind: &str,
        name: &str,
        source: &str,
        parent_id: Option<&str>,
        path: Option<&str>,
        group_id: Option<&str>,
    ) -> InventoryItem {
        InventoryItem {
            id: id.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            layer: if source == "db" { "db" } else { "code" }.to_string(),
            source: source.to_string(),
            parent_id: parent_id.map(str::to_string),
            path: path.map(str::to_string),
            qualified_name: None,
            engine_label: None,
            language: None,
            role_basis: None,
            project_id: None,
            group_id: group_id.map(str::to_string),
            location: None,
            is_primary_key: false,
            is_foreign_key: false,
            nullable: None,
        }
    }
}
