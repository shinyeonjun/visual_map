#[cfg(test)]
mod tests {
    use super::*;
    use database_memory_core::graph_builder::insert_schema_snapshot_graph;
    use database_memory_core::graph_store::GraphSnapshotRecord;
    use database_memory_core::{
        AdapterCapabilities, CapabilitySupport, DatabaseObject, ObjectKind, RoutineKind,
        RoutineObject, SchemaObject, SchemaSnapshot, TableKind, TableObject, TriggerObject,
        ViewObject,
    };

    const SNAPSHOT: &str = "sqlite:sample";

    #[test]
    fn describes_and_finds_cached_graph_metadata() {
        let store = GraphStore::in_memory().unwrap();
        insert_schema_snapshot_graph(&store, SNAPSHOT, 0, &snapshot()).unwrap();

        let description = describe_table(&store, SNAPSHOT, None, Some("orders")).unwrap();
        let text = render_table_description(&description, OutputFormat::Text);
        let json = render_table_description(&description, OutputFormat::Json);

        assert!(text.contains("user_id INTEGER nullable: no"));
        assert!(text.contains("fk_orders_user: orders(user_id) -> users(id)"));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["contract_version"], PRODUCT_CONTRACT_VERSION);
        assert_eq!(value["snapshot_key"], SNAPSHOT);
        assert_eq!(value["table"], "orders");
        assert!(value["table_key"]
            .as_str()
            .is_some_and(|key| key.contains(":table:orders")));
        assert!(value["constraints"].as_array().is_some_and(|constraints| {
            constraints
                .iter()
                .any(|constraint| constraint["kind"] == "foreign_key")
        }));
        assert_eq!(value["columns"][1]["name"], "user_id");
        assert!(value["columns"][1]["key"]
            .as_str()
            .is_some_and(|key| { key == "sqlite:sample:main:main:column:orders:user_id" }));
        assert_eq!(
            value["columns"][1]["table_key"],
            "sqlite:sample:main:main:table:orders"
        );
        assert_eq!(value["columns"][1]["schema"], "main");
        assert_eq!(value["columns"][1]["database"], "main");
        let foreign_key = value["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|constraint| constraint["kind"] == "foreign_key")
            .unwrap();
        assert_eq!(foreign_key["columns"][0], "user_id");
        assert_eq!(
            foreign_key["column_keys"][0],
            "sqlite:sample:main:main:column:orders:user_id"
        );
        assert_eq!(
            foreign_key["referenced_column_keys"][0],
            "sqlite:sample:main:main:column:users:id"
        );
        assert!(value["indexes"][0].get("predicate").is_some());
        assert!(value["indexes"][0].get("expression").is_some());
        assert!(value["indexes"][0]["key"]
            .as_str()
            .is_some_and(|key| key.contains(":index:orders:")));
        assert!(value["foreign_keys"]["outbound"][0]["key"]
            .as_str()
            .is_some_and(|key| key.contains(":foreign_key:orders:")));
        assert!(value["foreign_keys"]["outbound"][0]["table_key"]
            .as_str()
            .is_some_and(|key| key.contains(":table:orders")));
        assert!(value["foreign_keys"]["outbound"][0]["referenced_table_key"]
            .as_str()
            .is_some_and(|key| key.contains(":table:users")));
        assert_eq!(
            value["foreign_keys"]["outbound"][0]["column_keys"][0],
            "sqlite:sample:main:main:column:orders:user_id"
        );
        assert_eq!(
            value["foreign_keys"]["outbound"][0]["referenced_column_keys"][0],
            "sqlite:sample:main:main:column:users:id"
        );
        assert_eq!(value["indexes"][0]["columns"][0], "user_id");
        assert_eq!(
            value["indexes"][0]["column_keys"][0],
            "sqlite:sample:main:main:column:orders:user_id"
        );
        assert_eq!(
            render_find_table(&store, SNAPSHOT, "ord", OutputFormat::Text).unwrap(),
            "orders\n"
        );
        assert_eq!(
            render_find_column(&store, SNAPSHOT, "USER", OutputFormat::Text).unwrap(),
            "orders.user_id\n"
        );

        let found_columns: serde_json::Value = serde_json::from_str(
            &render_find_column(&store, SNAPSHOT, "USER", OutputFormat::Json).unwrap(),
        )
        .unwrap();
        assert_eq!(found_columns["columns"][0]["table"], "orders");
        assert_eq!(found_columns["columns"][0]["column"], "user_id");
        assert_eq!(
            found_columns["columns"][0]["key"],
            "sqlite:sample:main:main:column:orders:user_id"
        );
        assert_eq!(
            found_columns["columns"][0]["column_key"],
            "sqlite:sample:main:main:column:orders:user_id"
        );
        assert_eq!(
            found_columns["columns"][0]["table_key"],
            "sqlite:sample:main:main:table:orders"
        );
        assert_eq!(found_columns["columns"][0]["schema"], "main");
        assert_eq!(found_columns["columns"][0]["database"], "main");
        assert_eq!(found_columns["columns"][0]["type"], "INTEGER");
        assert_eq!(found_columns["columns"][0]["nullable"], false);
        assert_eq!(found_columns["columns"][0]["generated"], false);
    }

    #[test]
    fn inventory_and_describe_include_direct_view_trigger_and_routine_dependents() {
        let mut source = snapshot();
        let orders = key(ObjectKind::Table, "orders", None);
        let user_id = key(ObjectKind::Column, "orders", Some("user_id"));
        source.views.push(ViewObject {
            key: key(ObjectKind::View, "order_users", None),
            schema_key: key(ObjectKind::Schema, "main", None),
            name: "order_users".to_owned(),
            definition: None,
            depends_on: vec![orders.clone(), user_id.clone()],
        });
        source.triggers.push(TriggerObject {
            key: key(ObjectKind::Trigger, "orders", Some("orders_touch")),
            table_key: orders.clone(),
            name: "orders_touch".to_owned(),
            timing: Some("AFTER".to_owned()),
            events: vec!["UPDATE".to_owned()],
            definition: None,
            executes_routine_key: None,
        });
        source.routines.push(RoutineObject {
            key: key(ObjectKind::Routine, "refresh_orders", None),
            schema_key: key(ObjectKind::Schema, "main", None),
            name: "refresh_orders".to_owned(),
            kind: RoutineKind::Function,
            definition: None,
            depends_on: vec![orders.clone(), user_id.clone()],
        });
        let store = GraphStore::in_memory().unwrap();
        insert_schema_snapshot_graph(&store, SNAPSHOT, 0, &source).unwrap();

        let inventory: serde_json::Value =
            serde_json::from_str(&render_inventory(&store, SNAPSHOT, 0, 10).unwrap()).unwrap();
        let table = inventory["tables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|table| table["table_key"] == orders.to_string())
            .unwrap();
        let dependents = table["dependents"].as_array().unwrap();

        assert_eq!(dependents.len(), 3);
        assert!(dependents.iter().any(|dependent| {
            dependent["kind"] == "view"
                && dependent["name"] == "order_users"
                && dependent["column_keys"] == json!([user_id.to_string()])
        }));
        assert!(dependents.iter().any(|dependent| {
            dependent["kind"] == "trigger"
                && dependent["relation"] == "table_has_trigger"
                && dependent["column_keys"] == json!([])
        }));

        let described = describe_table(&store, SNAPSHOT, Some(&orders.to_string()), None).unwrap();
        assert_eq!(table, &table_description_json_value(&described));
    }

    #[test]
    fn snapshot_selector_supports_non_sqlite_aliases_and_rejects_ambiguity() {
        let store = GraphStore::in_memory().unwrap();
        for snapshot_key in ["postgres:shared", "mysql:shared"] {
            store
                .insert_snapshot(&GraphSnapshotRecord {
                    snapshot_key: snapshot_key.to_owned(),
                    source: Some(snapshot_key.to_owned()),
                    captured_at_unix_ms: 0,
                    payload_json: "{}".to_owned(),
                })
                .unwrap();
        }

        assert_eq!(
            resolve_snapshot_key(&store, "postgres:shared").unwrap(),
            "postgres:shared"
        );
        let error = resolve_snapshot_key(&store, "shared").unwrap_err();
        assert!(error.contains("ambiguous"));
        assert!(error.contains("mysql:shared"));
        assert!(error.contains("postgres:shared"));
    }

    #[test]
    fn duplicate_table_names_require_a_stable_key_and_find_keeps_legacy_names() {
        let store = GraphStore::in_memory().unwrap();
        insert_schema_snapshot_graph(&store, SNAPSHOT, 0, &multi_schema_snapshot()).unwrap();

        let main_key = key(ObjectKind::Table, "orders", None).to_string();
        let audit_key = key_in_schema("audit", ObjectKind::Table, "orders", None).to_string();
        let ambiguity = describe_table(&store, SNAPSHOT, None, Some("orders"))
            .err()
            .unwrap();
        assert!(ambiguity.contains("ambiguous"));
        assert!(ambiguity.contains(&main_key));
        assert!(ambiguity.contains(&audit_key));

        let selected = describe_table(&store, SNAPSHOT, Some(&audit_key), None).unwrap();
        assert_eq!(selected.table_key, audit_key);
        assert_eq!(selected.table_name, "orders");

        let found: serde_json::Value = serde_json::from_str(
            &render_find_table(&store, SNAPSHOT, "orders", OutputFormat::Json).unwrap(),
        )
        .unwrap();
        assert_eq!(found["tables"], json!(["orders", "orders"]));
        assert_eq!(found["table_matches"].as_array().unwrap().len(), 2);
        let match_keys = found["table_matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["table_key"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(match_keys, vec![audit_key.as_str(), main_key.as_str()]);
        assert_eq!(found["table_matches"][0]["name"], "orders");
        assert_eq!(found["table_matches"][0]["schema"], "audit");
        assert_eq!(found["table_matches"][0]["database"], "main");

        let impact_error = render_impact_analysis(
            &store,
            SNAPSHOT,
            None,
            Some("orders"),
            None,
            Direction::Both,
            1,
            10,
        )
        .unwrap_err();
        assert_eq!(impact_error, ambiguity);
    }

    #[test]
    fn inventory_json_is_bounded_sorted_and_matches_describe_table_shape() {
        let store = GraphStore::in_memory().unwrap();
        insert_schema_snapshot_graph(&store, SNAPSHOT, 0, &multi_schema_snapshot()).unwrap();

        let warnings = json!([
            "view dependency metadata is not tracked by the sqlite adapter.",
            "trigger dependency metadata is not tracked by the sqlite adapter.",
            "routine dependency metadata is not tracked by the sqlite adapter.",
            "cross-object dependency metadata is not tracked by the sqlite adapter."
        ]);
        let inventory: serde_json::Value =
            serde_json::from_str(&render_inventory(&store, SNAPSHOT, 0, 1).unwrap()).unwrap();
        assert_eq!(
            inventory,
            json!({
                "contract_version": PRODUCT_CONTRACT_VERSION,
                "snapshot_key": SNAPSHOT,
                "offset": 0,
                "limit_requested": 1,
                "limit_applied": 1,
                "limit_clamped": false,
                "result_count": 1,
                "total_tables": 3,
                "has_more": true,
                "next_offset": 1,
                "truncated": true,
                "capability_warnings": warnings,
                "tables": [{
                    "contract_version": PRODUCT_CONTRACT_VERSION,
                    "snapshot_key": SNAPSHOT,
                    "table_key": "sqlite:sample:main:audit:table:orders",
                    "table": "orders",
                    "columns": [{
                        "key": "sqlite:sample:main:audit:column:orders:id",
                        "table_key": "sqlite:sample:main:audit:table:orders",
                        "schema": "audit",
                        "database": "main",
                        "name": "id",
                        "type": "INTEGER",
                        "nullable": false
                    }],
                    "primary_key": [],
                    "constraints": [],
                    "foreign_keys": { "outbound": [], "inbound": [] },
                    "indexes": [],
                    "dependents": [],
                    "capability_warnings": warnings
                }]
            })
        );

        let all: serde_json::Value =
            serde_json::from_str(&render_inventory(&store, SNAPSHOT, 0, 10).unwrap()).unwrap();
        let table_keys = all["tables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|table| table["table_key"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(table_keys.windows(2).all(|keys| keys[0] < keys[1]));
        assert_eq!(
            all["tables"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|table| table["table"] == "orders")
                .count(),
            2
        );

        let main_orders = describe_table(
            &store,
            SNAPSHOT,
            Some("sqlite:sample:main:main:table:orders"),
            None,
        )
        .unwrap();
        assert_eq!(
            all["tables"]
                .as_array()
                .unwrap()
                .iter()
                .find(|table| table["table_key"] == main_orders.table_key)
                .unwrap(),
            &table_description_json_value(&main_orders)
        );

        assert_eq!(
            inventory_bounds(MAX_INVENTORY_TABLES + 1),
            (MAX_INVENTORY_TABLES, true)
        );

        let second_page: serde_json::Value =
            serde_json::from_str(&render_inventory(&store, SNAPSHOT, 1, 1).unwrap()).unwrap();
        assert_eq!(second_page["offset"], 1);
        assert_eq!(second_page["result_count"], 1);
        assert_eq!(second_page["next_offset"], 2);
        assert_ne!(
            second_page["tables"][0]["table_key"],
            inventory["tables"][0]["table_key"]
        );

        let exhausted: serde_json::Value =
            serde_json::from_str(&render_inventory(&store, SNAPSHOT, 3, 1).unwrap()).unwrap();
        assert_eq!(exhausted["tables"], json!([]));
        assert_eq!(exhausted["has_more"], false);
        assert_eq!(exhausted["next_offset"], serde_json::Value::Null);
        assert_eq!(exhausted["truncated"], false);
    }

    #[test]
    fn renders_bounded_impact_and_trace_contracts() {
        let store = GraphStore::in_memory().unwrap();
        insert_schema_snapshot_graph(&store, SNAPSHOT, 0, &snapshot()).unwrap();

        let impact: serde_json::Value = serde_json::from_str(
            &render_impact_analysis(
                &store,
                SNAPSHOT,
                None,
                Some("orders"),
                Some("user_id"),
                Direction::Outbound,
                99,
                1,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(impact["contract_version"], PRODUCT_CONTRACT_VERSION);
        assert_eq!(impact["snapshot_key"], SNAPSHOT);
        assert_eq!(impact["max_depth_applied"], MAX_TRAVERSAL_DEPTH);
        assert_eq!(impact["max_depth_clamped"], true);
        assert_eq!(impact["result_count"], 1);
        assert_eq!(impact["truncated"], true);
        assert!(impact["groups"][0]["nodes"][0]["edge_type"].is_string());
        assert!(impact["groups"][0]["nodes"][0]["edge_from"].is_string());
        assert!(impact["groups"][0]["nodes"][0]["edge_to"].is_string());
        assert!(impact["capability_warnings"].is_array());

        let start = key(ObjectKind::Column, "orders", Some("user_id")).to_string();
        let trace: serde_json::Value = serde_json::from_str(
            &render_relationship_trace(&store, SNAPSHOT, &start, Direction::Outbound, 2, 1)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(trace["start_object_key"], start);
        assert_eq!(trace["direction"], "outbound");
        assert_eq!(trace["result_count"], 1);
        assert_eq!(trace["truncated"], true);
        assert_eq!(trace["paths"][0]["hops"][1]["depth"], 1);
        assert!(trace["paths"][0]["hops"][1]["edge_type"].is_string());
        assert!(trace["paths"][0]["hops"][1]["edge_from"].is_string());
        assert!(trace["paths"][0]["hops"][1]["edge_to"].is_string());
    }

    fn snapshot() -> SchemaSnapshot {
        let database = key(ObjectKind::Database, "main", None);
        let schema = key(ObjectKind::Schema, "main", None);
        let users = key(ObjectKind::Table, "users", None);
        let orders = key(ObjectKind::Table, "orders", None);
        let users_id = key(ObjectKind::Column, "users", Some("id"));
        let orders_id = key(ObjectKind::Column, "orders", Some("id"));
        let orders_user_id = key(ObjectKind::Column, "orders", Some("user_id"));

        SchemaSnapshot {
            source_kind: "sqlite".to_owned(),
            connection_alias: "sample".to_owned(),
            database: DatabaseObject {
                key: database.clone(),
                name: "main".to_owned(),
            },
            schemas: vec![SchemaObject {
                key: schema.clone(),
                database_key: database,
                name: "main".to_owned(),
            }],
            tables: vec![
                TableObject {
                    key: users.clone(),
                    schema_key: schema.clone(),
                    name: "users".to_owned(),
                    kind: TableKind::BaseTable,
                },
                TableObject {
                    key: orders.clone(),
                    schema_key: schema,
                    name: "orders".to_owned(),
                    kind: TableKind::BaseTable,
                },
            ],
            columns: vec![
                column(users_id.clone(), users.clone(), "id", 1),
                column(orders_id.clone(), orders.clone(), "id", 1),
                column(orders_user_id.clone(), orders.clone(), "user_id", 2),
            ],
            constraints: vec![
                ConstraintObject {
                    key: key(ObjectKind::PrimaryKey, "orders", Some("pk_orders")),
                    table_key: orders.clone(),
                    name: "pk_orders".to_owned(),
                    kind: ConstraintKind::PrimaryKey,
                    columns: vec![orders_id],
                    referenced_table_key: None,
                    referenced_columns: vec![],
                    expression: None,
                },
                ConstraintObject {
                    key: key(ObjectKind::ForeignKey, "orders", Some("fk_orders_user")),
                    table_key: orders.clone(),
                    name: "fk_orders_user".to_owned(),
                    kind: ConstraintKind::ForeignKey,
                    columns: vec![orders_user_id.clone()],
                    referenced_table_key: Some(users.clone()),
                    referenced_columns: vec![users_id.clone()],
                    expression: None,
                },
                ConstraintObject {
                    key: key(ObjectKind::PrimaryKey, "users", Some("pk_users")),
                    table_key: users,
                    name: "pk_users".to_owned(),
                    kind: ConstraintKind::PrimaryKey,
                    columns: vec![users_id],
                    referenced_table_key: None,
                    referenced_columns: vec![],
                    expression: None,
                },
            ],
            indexes: vec![IndexObject {
                key: key(ObjectKind::Index, "orders", Some("idx_orders_user_id")),
                table_key: orders,
                name: "idx_orders_user_id".to_owned(),
                columns: vec![orders_user_id],
                is_unique: false,
                is_primary: false,
                predicate: None,
                expression: None,
            }],
            views: vec![],
            triggers: vec![],
            routines: vec![],
            capabilities: AdapterCapabilities {
                source_kind: "sqlite".to_owned(),
                metadata_only: true,
                schemas: true,
                tables: true,
                columns: true,
                constraints: true,
                indexes: true,
                views: CapabilitySupport::Unsupported,
                triggers: CapabilitySupport::Unsupported,
                routines: CapabilitySupport::Unsupported,
                dependencies: CapabilitySupport::Unsupported,
                limitations: vec![],
                notes: vec![],
            },
        }
    }

    fn multi_schema_snapshot() -> SchemaSnapshot {
        let mut snapshot = snapshot();
        let audit_schema = key_in_schema("audit", ObjectKind::Schema, "audit", None);
        let audit_orders = key_in_schema("audit", ObjectKind::Table, "orders", None);
        let audit_orders_id = key_in_schema("audit", ObjectKind::Column, "orders", Some("id"));
        snapshot.schemas.push(SchemaObject {
            key: audit_schema.clone(),
            database_key: snapshot.database.key.clone(),
            name: "audit".to_owned(),
        });
        snapshot.tables.push(TableObject {
            key: audit_orders.clone(),
            schema_key: audit_schema,
            name: "orders".to_owned(),
            kind: TableKind::BaseTable,
        });
        snapshot
            .columns
            .push(column(audit_orders_id, audit_orders, "id", 1));
        snapshot
    }

    fn column(
        key: ObjectKey,
        table_key: ObjectKey,
        name: &str,
        ordinal_position: u32,
    ) -> ColumnObject {
        ColumnObject {
            key,
            table_key,
            name: name.to_owned(),
            ordinal_position,
            data_type: "INTEGER".to_owned(),
            is_nullable: false,
            default_value: None,
            is_generated: false,
        }
    }

    fn key(kind: ObjectKind, object_name: &str, sub_object: Option<&str>) -> ObjectKey {
        key_in_schema("main", kind, object_name, sub_object)
    }

    fn key_in_schema(
        schema: &str,
        kind: ObjectKind,
        object_name: &str,
        sub_object: Option<&str>,
    ) -> ObjectKey {
        ObjectKey::new(
            "sqlite",
            "sample",
            "main",
            schema,
            kind,
            object_name,
            sub_object.map(str::to_owned),
        )
    }
}
