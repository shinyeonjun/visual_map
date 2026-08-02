#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::analysis_outcome::{AnalysisFailureCode, AnalysisStage, AnalysisStatus};

    use super::*;

    #[test]
    fn version_strategy_accepts_only_the_live_certified_engine() {
        for (major, expected) in [
            (14, SqlServerCatalogVersion::V2017),
            (15, SqlServerCatalogVersion::V2019),
            (16, SqlServerCatalogVersion::V2022),
            (17, SqlServerCatalogVersion::V2025),
        ] {
            assert_eq!(
                SqlServerCatalogVersion::detect(&server_facts(major, 3)).unwrap(),
                expected
            );
        }

        let unsupported_version = SqlServerCatalogVersion::detect(&server_facts(13, 3));
        assert!(matches!(
            unsupported_version,
            Err(CatalogError::UnsupportedVersion(13))
        ));

        let unsupported_engine = SqlServerCatalogVersion::detect(&server_facts(16, 5));
        assert!(matches!(
            unsupported_engine,
            Err(CatalogError::UnsupportedProduct(_))
        ));
    }

    #[test]
    fn changed_catalog_signature_is_never_accepted_as_stable() {
        assert_eq!(require_stable_catalog("same", &"same").unwrap(), "same");
        assert!(matches!(
            require_stable_catalog("first", &"second"),
            Err(CatalogError::CatalogChanged(_))
        ));
    }

    #[test]
    fn connection_policy_requires_a_database_and_secures_remote_transport() {
        let request = request("policy");
        let no_database = validate_connection_policy(
            &request,
            "Server=tcp:127.0.0.1,1433;User ID=reader;Password=do-not-echo",
        )
        .unwrap_err();
        assert_eq!(no_database.code, AnalysisFailureCode::InvalidConfiguration);

        let unsafe_remote = validate_connection_policy(
            &request,
            "Server=tcp:db.example.com,1433;Database=app;User ID=reader;Password=do-not-echo;Encrypt=false;TrustServerCertificate=true",
        )
        .unwrap_err();
        assert_eq!(unsafe_remote.code, AnalysisFailureCode::UnsafeSource);
        assert!(!unsafe_remote.message.contains("do-not-echo"));

        validate_connection_policy(
            &request,
            "Server=tcp:db.example.com,1433;Database=app;User ID=reader;Password=do-not-echo;Encrypt=true;TrustServerCertificate=false",
        )
        .unwrap();
        validate_connection_policy(
            &request,
            "Server=tcp:127.0.0.1,1433;Database=app;User ID=reader;Password=do-not-echo;Encrypt=false;TrustServerCertificate=true",
        )
        .unwrap();
    }

    #[test]
    fn dynamic_sql_is_rejected_without_blocking_static_execution_contexts() {
        reject_dynamic_sql(
            "routine",
            "dbo.static_proc",
            "CREATE PROCEDURE dbo.static_proc AS EXEC dbo.child_proc @id = 1",
        )
        .unwrap();
        reject_dynamic_sql(
            "routine",
            "dbo.execute_as_proc",
            "CREATE PROCEDURE dbo.execute_as_proc WITH EXECUTE AS OWNER AS SELECT 1",
        )
        .unwrap();

        for definition in [
            "CREATE PROCEDURE dbo.dynamic_var AS DECLARE @sql nvarchar(max); EXEC(@sql)",
            "CREATE PROCEDURE dbo.dynamic_text AS EXEC(N'SELECT 1')",
            "CREATE PROCEDURE dbo.dynamic_system AS EXEC sys.sp_executesql N'SELECT 1'",
        ] {
            assert!(
                matches!(
                    reject_dynamic_sql("routine", "dbo.dynamic", definition),
                    Err(CatalogError::UnsupportedMetadata(_))
                ),
                "accepted dynamic SQL: {definition}"
            );
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_SQLSERVER2022_URL"]
    fn sqlserver_2022_live_catalog_is_certified() {
        let _guard = live_test_guard();
        let connection_string = std::env::var("DATABASE_MEMORY_TEST_SQLSERVER2022_URL")
            .expect("live SQL Server test requires DATABASE_MEMORY_TEST_SQLSERVER2022_URL");

        let outcome = analyze_sqlserver(
            &connection_string,
            "sqlserver-2022-live",
            Vec::new(),
            Vec::new(),
            30_000,
        );

        assert_eq!(
            outcome.status(),
            AnalysisStatus::Complete,
            "{:?}",
            outcome.failure()
        );
        let snapshot = outcome.certified_snapshot().unwrap();
        assert_eq!(snapshot.snapshot.schema.source_kind, SQLSERVER_SOURCE);
        assert!(snapshot.snapshot.schema.capabilities.metadata_only);
        assert_eq!(snapshot.completeness.server.product, "Microsoft SQL Server");
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_SQLSERVER*_URL"]
    fn rich_sqlserver_catalog_is_certified_across_the_live_matrix() {
        let _guard = live_test_guard();
        let configured = required_sqlserver_matrix();
        for (strategy, connection_string) in configured {
            let schema = format!("dm_{}", unique_suffix());
            let fixture = SqlServerFixture::new(&schema);
            fixture.create(&connection_string);

            let outcome = analyze_sqlserver(
                &connection_string,
                &format!("{strategy}-rich"),
                vec!["master".to_owned()],
                vec![schema.clone()],
                60_000,
            );
            let failure = outcome.failure().cloned();
            let certified = outcome.certified_snapshot().cloned();
            fixture.drop(&connection_string);

            assert_eq!(outcome.status(), AnalysisStatus::Complete, "{failure:?}");
            let certified = certified.unwrap();
            assert!(certified
                .completeness
                .capability_checks
                .iter()
                .any(|check| {
                    check.name == "catalog_version_strategy" && check.evidence == strategy
                }));
            let snapshot = &certified.snapshot;
            assert_eq!(
                snapshot
                    .schema
                    .schemas
                    .iter()
                    .map(|item| item.name.as_str())
                    .collect::<Vec<_>>(),
                vec![schema.as_str()]
            );
            for table in [
                "users",
                "secured_accounts",
                "orders",
                "audit_log",
                "partitioned_events",
                "temporal_records",
                "temporal_records_history",
            ] {
                assert!(snapshot
                    .schema
                    .tables
                    .iter()
                    .any(|item| { item.key.schema == schema && item.name == table }));
            }
            for kind in [
                ConstraintKind::PrimaryKey,
                ConstraintKind::ForeignKey,
                ConstraintKind::Unique,
                ConstraintKind::Check,
            ] {
                assert!(
                    snapshot
                        .schema
                        .constraints
                        .iter()
                        .any(|constraint| constraint.kind == kind),
                    "missing {kind:?}"
                );
            }
            assert!(snapshot.schema.columns.iter().any(|column| {
                column.table_key.schema == schema
                    && column.name == "email_key"
                    && column.is_generated
            }));
            assert!(snapshot
                .schema
                .indexes
                .iter()
                .any(|index| index.name == "ix_orders_open"));
            assert!(snapshot
                .schema
                .views
                .iter()
                .any(|view| view.name == "order_summary" && view.depends_on.len() >= 2));
            assert!(snapshot
                .schema
                .routines
                .iter()
                .any(|routine| routine.name == "active_users" && !routine.depends_on.is_empty()));
            assert!(snapshot
                .schema
                .triggers
                .iter()
                .any(|trigger| trigger.name == "tr_orders_audit"));
            for (kind, name) in [
                (ObjectKind::UserDefinedType, "account_code"),
                (ObjectKind::Sequence, "order_numbers"),
                (ObjectKind::Synonym, "users_alias"),
            ] {
                assert!(snapshot
                    .metadata
                    .objects
                    .iter()
                    .any(|object| object.key.object_kind == kind && object.name == name));
            }
            assert!(snapshot.metadata.relationships.iter().any(|relationship| {
                relationship.kind == MetadataRelationshipKind::IncludesColumn
            }));
            assert!(snapshot
                .metadata
                .relationships
                .iter()
                .any(|relationship| { relationship.kind == MetadataRelationshipKind::SynonymFor }));
            assert!(snapshot.metadata.relationships.iter().any(|relationship| {
                relationship.kind == MetadataRelationshipKind::UsesSequence
            }));
            assert!(snapshot.metadata.objects.iter().any(|object| {
                object.key.object_kind == ObjectKind::MaterializedView
                    && object.name == "user_tenant_counts"
            }));
            assert!(snapshot.metadata.objects.iter().any(|object| {
                object.key.object_kind == ObjectKind::Policy && object.name == "tenant_policy"
            }));
            assert!(snapshot.metadata.objects.iter().any(|object| {
                object.key.object_kind == ObjectKind::Trigger
                    && object.name == fixture.database_trigger()
            }));
            assert!(snapshot.metadata.objects.iter().any(|object| {
                object.extension_kind.as_deref() == Some("sqlserver_partition_function")
                    && object.name == fixture.partition_function()
            }));
            for relationship in [
                MetadataRelationshipKind::Materializes,
                MetadataRelationshipKind::Extension("temporal_history_table".to_owned()),
                MetadataRelationshipKind::Extension("security_predicate_applies_to".to_owned()),
            ] {
                assert!(
                    snapshot
                        .metadata
                        .relationships
                        .iter()
                        .any(|item| item.kind == relationship),
                    "missing relationship {relationship:?}"
                );
            }
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_SQLSERVER2022_URL"]
    fn unprovable_sqlserver_metadata_fails_closed_on_the_live_server() {
        let _guard = live_test_guard();
        let connection_string = std::env::var("DATABASE_MEMORY_TEST_SQLSERVER2022_URL").expect(
            "SQL Server unsupported-metadata test requires DATABASE_MEMORY_TEST_SQLSERVER2022_URL",
        );
        let schema = format!("dm_{}", unique_suffix());
        execute_admin_batches(
            &connection_string,
            &[format!("CREATE SCHEMA [{schema}] AUTHORIZATION [dbo]")],
        )
        .unwrap();

        execute_admin_batches(
            &connection_string,
            &[format!(
                "CREATE PROCEDURE [{schema}].[dynamic_reader] AS DECLARE @sql nvarchar(max) = N'SELECT 1'; EXEC(@sql)"
            )],
        )
        .unwrap();
        let dynamic = analyze_sqlserver(
            &connection_string,
            "sqlserver-dynamic",
            Vec::new(),
            vec![schema.clone()],
            30_000,
        );
        execute_admin_batches(
            &connection_string,
            &[format!(
                "DROP PROCEDURE IF EXISTS [{schema}].[dynamic_reader]"
            )],
        )
        .unwrap();

        execute_admin_batches(
            &connection_string,
            &[format!(
                "CREATE PROCEDURE [{schema}].[encrypted_reader] WITH ENCRYPTION AS SELECT 1 AS [value]"
            )],
        )
        .unwrap();
        let encrypted = analyze_sqlserver(
            &connection_string,
            "sqlserver-encrypted",
            Vec::new(),
            vec![schema.clone()],
            30_000,
        );
        execute_admin_batches(
            &connection_string,
            &[
                format!("DROP PROCEDURE IF EXISTS [{schema}].[encrypted_reader]"),
                format!("DROP SCHEMA IF EXISTS [{schema}]"),
            ],
        )
        .unwrap();

        for (label, outcome) in [("dynamic", dynamic), ("encrypted", encrypted)] {
            assert_eq!(
                outcome.status(),
                AnalysisStatus::Failed,
                "{label}: {:?}",
                outcome.failure()
            );
            assert_eq!(
                outcome.failure().map(|failure| failure.code),
                Some(AnalysisFailureCode::UnsupportedMetadata),
                "{label}"
            );
            assert!(outcome.certified_snapshot().is_none(), "{label}");
        }
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_SQLSERVER*_URL"]
    fn table_types_are_fully_mapped_across_the_live_matrix() {
        let _guard = live_test_guard();
        let configured = required_sqlserver_matrix();
        for (strategy, connection_string) in configured {
            assert_table_type_catalog(strategy, &connection_string);
        }
    }

    fn assert_table_type_catalog(strategy: &str, connection_string: &str) {
        let schema = format!("dm_{}", unique_suffix());
        let creation = execute_admin_batches(
            connection_string,
            &[
                format!("CREATE SCHEMA [{schema}] AUTHORIZATION [dbo]"),
                format!("CREATE TYPE [{schema}].[code] FROM nvarchar(20) NOT NULL"),
                format!(
                    "CREATE TYPE [{schema}].[payload] AS TABLE (\
                     [id] int IDENTITY(1,1) NOT NULL PRIMARY KEY, \
                     [code] [{schema}].[code] NOT NULL UNIQUE, \
                     [amount] decimal(10,2) NULL CHECK ([amount] >= 0), \
                     [doubled] AS ([amount] * 2), \
                     [created_at] datetime2 NOT NULL DEFAULT SYSUTCDATETIME(), \
                     INDEX [ix_payload_amount] NONCLUSTERED ([amount] DESC))"
                ),
                format!(
                    "CREATE PROCEDURE [{schema}].[consume_payload] \
                     @items [{schema}].[payload] READONLY AS \
                     SELECT [id] FROM @items"
                ),
            ],
        );
        if let Err(error) = creation {
            let cleanup = drop_table_type_fixture(connection_string, &schema);
            panic!("{strategy}: failed to create table-type fixture: {error}; cleanup={cleanup:?}");
        }

        let outcome = analyze_sqlserver(
            connection_string,
            &format!("{strategy}-table-type"),
            Vec::new(),
            vec![schema.clone()],
            30_000,
        );
        let failure = outcome.failure().cloned();
        let certified = outcome.certified_snapshot().cloned();
        drop_table_type_fixture(connection_string, &schema).unwrap();

        assert_eq!(
            outcome.status(),
            AnalysisStatus::Complete,
            "{strategy}: {failure:?}"
        );
        let certified = certified.unwrap();
        let extension_reconciliation = certified
            .completeness
            .object_counts
            .iter()
            .find(|count| count.category == ObjectCategory::Extension)
            .unwrap();
        assert_eq!(extension_reconciliation.discovered, 11, "{strategy}");
        assert_eq!(
            extension_reconciliation.discovered, extension_reconciliation.emitted,
            "{strategy}"
        );
        let snapshot = &certified.snapshot;
        let payload = snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::UserDefinedType
                    && object.key.schema == schema
                    && object.name == "payload"
            })
            .unwrap();
        assert_eq!(
            payload.properties.get("table_type"),
            Some(&MetadataValue::Boolean(true))
        );
        assert!(!snapshot
            .schema
            .tables
            .iter()
            .any(|table| table.key.schema == schema));

        let extension_count = |kind: &str| {
            snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| object.extension_kind.as_deref() == Some(kind))
                .count()
        };
        assert_eq!(extension_count("sqlserver_table_type_column"), 5);
        assert_eq!(extension_count("sqlserver_table_type_constraint"), 3);
        assert_eq!(extension_count("sqlserver_table_type_index"), 3);
        for relationship_kind in ["table_type_constraint_column", "table_type_index_column"] {
            assert!(snapshot.metadata.relationships.iter().any(|relationship| {
                relationship.kind
                    == MetadataRelationshipKind::Extension(relationship_kind.to_owned())
            }));
        }
        let parameter = snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::RoutineParameter && object.name == "@items"
            })
            .unwrap();
        assert!(snapshot.metadata.relationships.iter().any(|relationship| {
            relationship.kind == MetadataRelationshipKind::UsesType
                && relationship.from_key == parameter.key
                && relationship.to_key == payload.key
        }));
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_SQLSERVER*_URL"]
    fn xml_schema_and_extended_properties_are_certified_across_the_live_matrix() {
        let _guard = live_test_guard();
        let configured = required_sqlserver_matrix();
        for (strategy, connection_string) in configured {
            assert_xml_metadata_catalog(strategy, &connection_string);
        }
    }

    fn assert_xml_metadata_catalog(strategy: &str, connection_string: &str) {
        let schema = format!("dm_{}", unique_suffix());
        let creation = execute_admin_batches(
            connection_string,
            &[
                format!("CREATE SCHEMA [{schema}] AUTHORIZATION [dbo]"),
                format!(
                    "CREATE XML SCHEMA COLLECTION [{schema}].[payload_xsd] AS \
                     N'<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" \
                     targetNamespace=\"urn:dbmcp:test\" elementFormDefault=\"qualified\">\
                     <xs:element name=\"payload\" type=\"xs:string\"/>\
                     </xs:schema>'"
                ),
                format!(
                    "CREATE TABLE [{schema}].[typed_documents] (\
                     [id] int NOT NULL PRIMARY KEY, \
                     [payload] xml(CONTENT [{schema}].[payload_xsd]) NULL); \
                     CREATE INDEX [ix_typed_documents_payload] \
                     ON [{schema}].[typed_documents] ([id]) INCLUDE ([payload])"
                ),
                format!(
                    "CREATE PROCEDURE [{schema}].[read_payload] \
                     @payload xml(CONTENT [{schema}].[payload_xsd]) AS \
                     SELECT @payload AS [payload]"
                ),
                format!(
                    "CREATE TYPE [{schema}].[payload_type] AS TABLE (\
                     [id] int NOT NULL PRIMARY KEY, [label] nvarchar(20) NULL)"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'MS_Description', \
                     @value=N'XML metadata fixture schema', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'MS_Description', \
                     @value=N'Typed document table', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TABLE', @level1name=N'typed_documents'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'DisplayLabel', \
                     @value=N'Validated XML payload', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TABLE', @level1name=N'typed_documents', \
                     @level2type=N'COLUMN', @level2name=N'payload'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'LookupIndex', @value=7, \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TABLE', @level1name=N'typed_documents', \
                     @level2type=N'INDEX', @level2name=N'ix_typed_documents_payload'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'InputContract', \
                     @value=N'Validated payload input', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'PROCEDURE', @level1name=N'read_payload', \
                     @level2type=N'PARAMETER', @level2name=N'@payload'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'ContractKind', \
                     @value=N'table-valued payload', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TYPE', @level1name=N'payload_type'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'FieldHint', \
                     @value=N'payload label', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TYPE', @level1name=N'payload_type', \
                     @level2type=N'COLUMN', @level2name=N'label'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'NamespaceOwner', \
                     @value=N'backend-map', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'XML SCHEMA COLLECTION', @level1name=N'payload_xsd'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'BinaryMarker', \
                     @value=0x01020304, \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TABLE', @level1name=N'typed_documents'"
                ),
                format!(
                    "EXEC sys.sp_addextendedproperty @name=N'NullMarker', \
                     @level0type=N'SCHEMA', @level0name=N'{schema}', \
                     @level1type=N'TABLE', @level1name=N'typed_documents'"
                ),
            ],
        );
        if let Err(error) = creation {
            let cleanup = drop_xml_metadata_fixture(connection_string, &schema);
            panic!(
                "{strategy}: failed to create XML metadata fixture: {error}; cleanup={cleanup:?}"
            );
        }

        let outcome = analyze_sqlserver(
            connection_string,
            &format!("{strategy}-xml-metadata"),
            Vec::new(),
            vec![schema.clone()],
            30_000,
        );
        let failure = outcome.failure().cloned();
        let certified = outcome.certified_snapshot().cloned();
        drop_xml_metadata_fixture(connection_string, &schema).unwrap();

        assert_eq!(
            outcome.status(),
            AnalysisStatus::Complete,
            "{strategy}: {failure:?}"
        );
        let certified = certified.unwrap();
        let snapshot = &certified.snapshot;
        let collection = snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.extension_kind.as_deref() == Some("sqlserver_xml_schema_collection")
                    && object.key.schema == schema
                    && object.name == "payload_xsd"
            })
            .unwrap();
        assert!(snapshot.metadata.objects.iter().any(|object| {
            object.extension_kind.as_deref() == Some("sqlserver_xml_schema_namespace")
                && object.properties.get("namespace")
                    == Some(&MetadataValue::String("urn:dbmcp:test".to_owned()))
        }));
        assert_eq!(
            snapshot
                .metadata
                .relationships
                .iter()
                .filter(|relationship| {
                    relationship.kind
                        == MetadataRelationshipKind::Extension(
                            "uses_xml_schema_collection".to_owned(),
                        )
                        && relationship.to_key == collection.key
                })
                .count(),
            2,
            "{strategy}"
        );

        let extended_properties = snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| {
                object.extension_kind.as_deref() == Some("sqlserver_extended_property")
            })
            .collect::<Vec<_>>();
        assert_eq!(extended_properties.len(), 10, "{strategy}");
        let binary = extended_properties
            .iter()
            .find(|property| property.name == "BinaryMarker")
            .unwrap();
        assert_eq!(
            binary.properties.get("value_hex"),
            Some(&MetadataValue::String("01020304".to_owned())),
            "{strategy}"
        );
        let null_value = extended_properties
            .iter()
            .find(|property| property.name == "NullMarker")
            .unwrap();
        assert_eq!(
            null_value.properties.get("value_is_null"),
            Some(&MetadataValue::Boolean(true)),
            "{strategy}"
        );
        let extension_reconciliation = certified
            .completeness
            .object_counts
            .iter()
            .find(|count| count.category == ObjectCategory::Extension)
            .unwrap();
        assert_eq!(
            extension_reconciliation.discovered, extension_reconciliation.emitted,
            "{strategy}"
        );
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_SQLSERVER*_URL"]
    fn cross_schema_foreign_keys_require_a_complete_scope_across_the_live_matrix() {
        let _guard = live_test_guard();
        let configured = required_sqlserver_matrix();
        for (strategy, connection_string) in configured {
            let suffix = unique_suffix();
            let child_schema = format!("dm_child_{suffix}");
            let parent_schema = format!("dm_parent_{suffix}");
            let creation = execute_admin_batches(
                &connection_string,
                &[
                    format!("CREATE SCHEMA [{parent_schema}] AUTHORIZATION [dbo]"),
                    format!("CREATE SCHEMA [{child_schema}] AUTHORIZATION [dbo]"),
                    format!(
                        "CREATE TABLE [{parent_schema}].[accounts] (\
                         [id] int NOT NULL PRIMARY KEY)"
                    ),
                    format!(
                        "CREATE TABLE [{child_schema}].[orders] (\
                         [id] int NOT NULL PRIMARY KEY, [account_id] int NOT NULL, \
                         CONSTRAINT [fk_orders_accounts] FOREIGN KEY ([account_id]) \
                         REFERENCES [{parent_schema}].[accounts] ([id]))"
                    ),
                ],
            );
            if let Err(error) = creation {
                let cleanup =
                    drop_cross_schema_fixture(&connection_string, &child_schema, &parent_schema);
                panic!("{strategy}: failed to create scope fixture: {error}; cleanup={cleanup:?}");
            }

            let incomplete = analyze_sqlserver(
                &connection_string,
                &format!("{strategy}-incomplete-scope"),
                Vec::new(),
                vec![child_schema.clone()],
                30_000,
            );
            assert_eq!(incomplete.status(), AnalysisStatus::Failed, "{strategy}");
            assert_eq!(
                incomplete.failure().map(|failure| failure.code),
                Some(AnalysisFailureCode::InvalidConfiguration),
                "{strategy}: {:?}",
                incomplete.failure()
            );
            assert!(
                incomplete
                    .failure()
                    .is_some_and(|failure| failure.message.contains(&parent_schema)),
                "{strategy}: {:?}",
                incomplete.failure()
            );
            assert!(incomplete.certified_snapshot().is_none(), "{strategy}");

            let complete = analyze_sqlserver(
                &connection_string,
                &format!("{strategy}-complete-scope"),
                Vec::new(),
                vec![child_schema.clone(), parent_schema.clone()],
                30_000,
            );
            let failure = complete.failure().cloned();
            let certified = complete.certified_snapshot().cloned();
            drop_cross_schema_fixture(&connection_string, &child_schema, &parent_schema).unwrap();

            assert_eq!(
                complete.status(),
                AnalysisStatus::Complete,
                "{strategy}: {failure:?}"
            );
            let snapshot = &certified.unwrap().snapshot;
            let foreign_key = snapshot
                .schema
                .constraints
                .iter()
                .find(|constraint| constraint.name == "fk_orders_accounts")
                .unwrap();
            assert_eq!(
                foreign_key
                    .referenced_table_key
                    .as_ref()
                    .map(|key| key.schema.as_str()),
                Some(parent_schema.as_str()),
                "{strategy}"
            );
        }
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_SQLSERVER*_URL"]
    fn timeout_never_emits_a_partial_snapshot_across_the_live_matrix() {
        let _guard = live_test_guard();
        let configured = required_sqlserver_matrix();
        for (strategy, connection_string) in configured {
            let outcome = analyze_sqlserver(
                &connection_string,
                &format!("{strategy}-timeout"),
                Vec::new(),
                Vec::new(),
                1,
            );
            assert_eq!(outcome.status(), AnalysisStatus::Failed, "{strategy}");
            let failure = outcome.failure().unwrap();
            assert_eq!(failure.code, AnalysisFailureCode::Timeout, "{strategy}");
            assert_eq!(failure.stage, AnalysisStage::Discovery, "{strategy}");
            assert!(failure.retryable, "{strategy}");
            assert!(outcome.certified_snapshot().is_none(), "{strategy}");
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_SQLSERVER2022_URL"]
    fn metadata_visibility_is_required_and_sufficient_on_the_live_server() {
        let _guard = live_test_guard();
        let admin_connection = std::env::var("DATABASE_MEMORY_TEST_SQLSERVER2022_URL")
            .expect("SQL Server privilege test requires DATABASE_MEMORY_TEST_SQLSERVER2022_URL");
        let suffix = unique_suffix();
        let schema = format!("dm_{suffix}");
        let principal = format!("dm_reader_{suffix}");
        let password = format!("DmRead1!{suffix}");
        let reader_connection =
            connection_with_credentials(&admin_connection, &principal, &password);

        execute_admin_batches(
            &admin_connection,
            &[
                format!("CREATE SCHEMA [{schema}] AUTHORIZATION [dbo]"),
                format!(
                    "CREATE TABLE [{schema}].[visible_table] ([id] int NOT NULL PRIMARY KEY)"
                ),
                format!(
                    "CREATE LOGIN [{principal}] WITH PASSWORD = N'{}', CHECK_POLICY = OFF, CHECK_EXPIRATION = OFF",
                    password.replace('\'', "''")
                ),
                format!(
                    "CREATE USER [{principal}] FOR LOGIN [{principal}] WITH DEFAULT_SCHEMA = [{schema}]"
                ),
            ],
        )
        .unwrap();

        let denied = analyze_sqlserver(
            &reader_connection,
            "sqlserver-low-privilege",
            Vec::new(),
            vec![schema.clone()],
            30_000,
        );
        execute_admin_batches(
            &admin_connection,
            &[
                format!("GRANT VIEW DEFINITION TO [{principal}]"),
                format!(
                    "GRANT SELECT ON OBJECT::[sys].[sql_expression_dependencies] TO [{principal}]"
                ),
            ],
        )
        .unwrap();
        let allowed = analyze_sqlserver(
            &reader_connection,
            "sqlserver-metadata-reader",
            Vec::new(),
            vec![schema.clone()],
            30_000,
        );

        execute_admin_batches(
            &admin_connection,
            &[
                format!("DROP TABLE IF EXISTS [{schema}].[visible_table]"),
                format!("DROP SCHEMA IF EXISTS [{schema}]"),
                format!("DROP USER IF EXISTS [{principal}]"),
                format!(
                    "IF EXISTS (SELECT 1 FROM sys.server_principals WHERE name = N'{principal}') DROP LOGIN [{principal}]"
                ),
            ],
        )
        .unwrap();

        assert_eq!(denied.status(), AnalysisStatus::Failed);
        assert_eq!(
            denied.failure().map(|failure| failure.code),
            Some(AnalysisFailureCode::PermissionDenied),
            "{:?}",
            denied.failure()
        );
        assert_eq!(
            allowed.status(),
            AnalysisStatus::Complete,
            "{:?}",
            allowed.failure()
        );
        assert!(allowed
            .certified_snapshot()
            .unwrap()
            .snapshot
            .schema
            .tables
            .iter()
            .any(|table| table.name == "visible_table"));
    }

    #[test]
    fn async_runtime_cancellation_preempts_connection_work() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let failure = run_catalog_discovery_on_runtime(
            "not-a-sqlserver-connection-string",
            &request("sqlserver-cancelled-runtime"),
            &cancellation,
        )
        .expect_err("cancelled runtime must fail");

        assert_eq!(failure.code, AnalysisFailureCode::Cancelled);
        assert_eq!(failure.stage, AnalysisStage::Discovery);
    }

    fn request(alias: &str) -> IntrospectionRequest {
        IntrospectionRequest {
            connection_alias: alias.to_owned(),
            requested_catalogs: Vec::new(),
            requested_schemas: Vec::new(),
            timeout_ms: 1_000,
        }
    }

    fn server_facts(major: i32, engine_edition: i32) -> ServerFacts {
        ServerFacts {
            database: "app".to_owned(),
            version: format!("{major}.0.0.0"),
            major,
            engine_edition,
            edition: "Developer Edition".to_owned(),
            current_user: "dbo".to_owned(),
            login: "sa".to_owned(),
            original_login: "sa".to_owned(),
            collation: "SQL_Latin1_General_CP1_CI_AS".to_owned(),
            compatibility_level: 160,
            database_read_only: false,
            containment: "NONE".to_owned(),
            encrypted_transport: true,
        }
    }

    fn live_test_guard() -> MutexGuard<'static, ()> {
        static LIVE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LIVE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn required_sqlserver_matrix() -> Vec<(&'static str, String)> {
        let configured = [
            ("DATABASE_MEMORY_TEST_SQLSERVER2017_URL", "sqlserver-2017"),
            ("DATABASE_MEMORY_TEST_SQLSERVER2019_URL", "sqlserver-2019"),
            ("DATABASE_MEMORY_TEST_SQLSERVER2022_URL", "sqlserver-2022"),
            ("DATABASE_MEMORY_TEST_SQLSERVER2025_URL", "sqlserver-2025"),
        ]
        .into_iter()
        .filter_map(|(environment, strategy)| {
            std::env::var(environment)
                .ok()
                .map(|connection_string| (strategy, connection_string))
        })
        .collect::<Vec<_>>();
        assert!(
            !configured.is_empty(),
            "live SQL Server matrix test requires at least one DATABASE_MEMORY_TEST_SQLSERVER*_URL"
        );
        configured
    }

    fn unique_suffix() -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!(
            "{:x}_{:x}_{:x}",
            std::process::id(),
            nanos,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    struct SqlServerFixture {
        schema: String,
    }

    impl SqlServerFixture {
        fn new(schema: &str) -> Self {
            Self {
                schema: schema.to_owned(),
            }
        }

        fn create(&self, connection_string: &str) {
            let schema = &self.schema;
            let partition_function = self.partition_function();
            let partition_scheme = self.partition_scheme();
            let database_trigger = self.database_trigger();
            let creation = execute_admin_batches(
                connection_string,
                &[
                    format!("CREATE SCHEMA [{schema}] AUTHORIZATION [dbo]"),
                    format!(
                        "CREATE TYPE [{schema}].[account_code] FROM nvarchar(32) NOT NULL"
                    ),
                    format!(
                        "CREATE SEQUENCE [{schema}].[order_numbers] AS bigint START WITH 1000 INCREMENT BY 5 MINVALUE 1000 MAXVALUE 999999 CYCLE CACHE 20"
                    ),
                    format!(
                        "CREATE TABLE [{schema}].[users] (\
                         [id] bigint IDENTITY(10, 2) NOT NULL CONSTRAINT [pk_users] PRIMARY KEY,\
                         [tenant_id] int NOT NULL,\
                         [email] nvarchar(320) NOT NULL,\
                         [code] [{schema}].[account_code] NOT NULL,\
                         [email_key] AS LOWER(CONVERT(nvarchar(320), [email])) PERSISTED,\
                         [status] varchar(16) NOT NULL CONSTRAINT [df_users_status] DEFAULT ('active'),\
                         [created_at] datetime2(3) NOT NULL CONSTRAINT [df_users_created] DEFAULT (SYSUTCDATETIME()),\
                         CONSTRAINT [uq_users_email] UNIQUE ([email]),\
                         CONSTRAINT [ck_users_status] CHECK ([status] IN ('active', 'disabled')))"
                    ),
                    format!(
                        "CREATE TABLE [{schema}].[secured_accounts] (\
                         [id] bigint NOT NULL CONSTRAINT [pk_secured_accounts] PRIMARY KEY,\
                         [tenant_id] int NOT NULL,\
                         [display_name] nvarchar(200) NOT NULL)"
                    ),
                    format!(
                        "CREATE TABLE [{schema}].[orders] (\
                         [id] bigint NOT NULL CONSTRAINT [df_orders_id] DEFAULT (NEXT VALUE FOR [{schema}].[order_numbers]) CONSTRAINT [pk_orders] PRIMARY KEY,\
                         [user_id] bigint NOT NULL,\
                         [amount] decimal(18, 2) NOT NULL,\
                         [state] varchar(16) NOT NULL CONSTRAINT [df_orders_state] DEFAULT ('open'),\
                         [note] nvarchar(500) NULL,\
                         CONSTRAINT [fk_orders_users] FOREIGN KEY ([user_id]) REFERENCES [{schema}].[users]([id]) ON DELETE CASCADE,\
                         CONSTRAINT [ck_orders_amount] CHECK ([amount] >= 0))"
                    ),
                    format!(
                        "CREATE TABLE [{schema}].[audit_log] (\
                         [id] bigint IDENTITY(1, 1) NOT NULL CONSTRAINT [pk_audit_log] PRIMARY KEY,\
                         [order_id] bigint NOT NULL,\
                         [event_name] varchar(16) NOT NULL,\
                         [recorded_at] datetime2(3) NOT NULL CONSTRAINT [df_audit_recorded] DEFAULT (SYSUTCDATETIME()))"
                    ),
                    format!(
                        "CREATE PARTITION FUNCTION [{partition_function}](int) AS RANGE RIGHT FOR VALUES (100, 1000)"
                    ),
                    format!(
                        "CREATE PARTITION SCHEME [{partition_scheme}] AS PARTITION [{partition_function}] ALL TO ([PRIMARY])"
                    ),
                    format!(
                        "CREATE TABLE [{schema}].[partitioned_events] (\
                         [bucket] int NOT NULL,\
                         [payload] nvarchar(200) NULL) ON [{partition_scheme}]([bucket])"
                    ),
                    format!(
                        "CREATE INDEX [ix_partitioned_events] ON [{schema}].[partitioned_events]([bucket]) ON [{partition_scheme}]([bucket])"
                    ),
                    format!(
                        "CREATE TABLE [{schema}].[temporal_records] (\
                         [id] int NOT NULL CONSTRAINT [pk_temporal_records] PRIMARY KEY,\
                         [payload] nvarchar(200) NULL,\
                         [valid_from] datetime2 GENERATED ALWAYS AS ROW START HIDDEN NOT NULL,\
                         [valid_to] datetime2 GENERATED ALWAYS AS ROW END HIDDEN NOT NULL,\
                         PERIOD FOR SYSTEM_TIME ([valid_from], [valid_to]))\
                         WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = [{schema}].[temporal_records_history], DATA_CONSISTENCY_CHECK = ON))"
                    ),
                    format!(
                        "CREATE INDEX [ix_orders_open] ON [{schema}].[orders] ([user_id] ASC, [amount] DESC) INCLUDE ([note]) WHERE [state] = 'open' WITH (FILLFACTOR = 90)"
                    ),
                    format!(
                        "CREATE VIEW [{schema}].[active_users_view] AS SELECT [id], [email], [tenant_id] FROM [{schema}].[users] WHERE [status] = 'active'"
                    ),
                    format!(
                        "CREATE VIEW [{schema}].[order_summary] AS SELECT o.[id], u.[email], o.[amount] FROM [{schema}].[orders] AS o JOIN [{schema}].[active_users_view] AS u ON u.[id] = o.[user_id]"
                    ),
                    "SET ANSI_NULLS ON; SET QUOTED_IDENTIFIER ON; SET ANSI_PADDING ON; SET ANSI_WARNINGS ON; SET ARITHABORT ON; SET CONCAT_NULL_YIELDS_NULL ON; SET NUMERIC_ROUNDABORT OFF".to_owned(),
                    format!(
                        "CREATE VIEW [{schema}].[user_tenant_counts] WITH SCHEMABINDING AS SELECT [tenant_id], COUNT_BIG(*) AS [user_count] FROM [{schema}].[users] GROUP BY [tenant_id]"
                    ),
                    format!(
                        "CREATE UNIQUE CLUSTERED INDEX [cix_user_tenant_counts] ON [{schema}].[user_tenant_counts]([tenant_id])"
                    ),
                    format!(
                        "CREATE FUNCTION [{schema}].[active_users](@minimum_id bigint) RETURNS TABLE WITH SCHEMABINDING AS RETURN (SELECT [id], [email] FROM [{schema}].[users] WHERE [id] >= @minimum_id)"
                    ),
                    format!(
                        "CREATE PROCEDURE [{schema}].[read_orders] @minimum_amount decimal(18, 2) AS SELECT [id], [user_id], [amount] FROM [{schema}].[orders] WHERE [amount] >= @minimum_amount"
                    ),
                    format!(
                        "CREATE FUNCTION [{schema}].[tenant_filter](@tenant_id int) RETURNS TABLE WITH SCHEMABINDING AS RETURN SELECT 1 AS [allowed] WHERE @tenant_id = CONVERT(int, SESSION_CONTEXT(N'tenant_id'))"
                    ),
                    format!(
                        "CREATE SECURITY POLICY [{schema}].[tenant_policy] ADD FILTER PREDICATE [{schema}].[tenant_filter]([tenant_id]) ON [{schema}].[secured_accounts] WITH (STATE = ON, SCHEMABINDING = ON)"
                    ),
                    format!(
                        "CREATE TRIGGER [{schema}].[tr_orders_audit] ON [{schema}].[orders] AFTER INSERT, UPDATE AS INSERT INTO [{schema}].[audit_log]([order_id], [event_name]) SELECT [id], 'changed' FROM inserted"
                    ),
                    format!(
                        "CREATE SYNONYM [{schema}].[users_alias] FOR [{schema}].[users]"
                    ),
                    format!(
                        "CREATE TRIGGER [{database_trigger}] ON DATABASE FOR CREATE_TABLE AS RETURN"
                    ),
                ],
            );
            if let Err(error) = creation {
                let cleanup = self.try_drop(connection_string);
                panic!("failed to create SQL Server fixture: {error}; cleanup: {cleanup:?}");
            }
        }

        fn drop(&self, connection_string: &str) {
            self.try_drop(connection_string).unwrap();
        }

        fn try_drop(&self, connection_string: &str) -> Result<(), String> {
            let schema = &self.schema;
            let partition_function = self.partition_function();
            let partition_scheme = self.partition_scheme();
            let database_trigger = self.database_trigger();
            execute_admin_batches(
                connection_string,
                &[
                    format!(
                        "IF EXISTS (SELECT 1 FROM sys.triggers WHERE parent_class = 0 AND name = N'{database_trigger}') DROP TRIGGER [{database_trigger}] ON DATABASE"
                    ),
                    format!("DROP SECURITY POLICY IF EXISTS [{schema}].[tenant_policy]"),
                    format!("DROP SYNONYM IF EXISTS [{schema}].[users_alias]"),
                    format!("DROP TRIGGER IF EXISTS [{schema}].[tr_orders_audit]"),
                    format!("DROP PROCEDURE IF EXISTS [{schema}].[read_orders]"),
                    format!("DROP FUNCTION IF EXISTS [{schema}].[tenant_filter]"),
                    format!("DROP FUNCTION IF EXISTS [{schema}].[active_users]"),
                    format!("DROP VIEW IF EXISTS [{schema}].[user_tenant_counts]"),
                    format!("DROP VIEW IF EXISTS [{schema}].[order_summary]"),
                    format!("DROP VIEW IF EXISTS [{schema}].[active_users_view]"),
                    format!(
                        "IF OBJECT_ID(N'{schema}.temporal_records', N'U') IS NOT NULL ALTER TABLE [{schema}].[temporal_records] SET (SYSTEM_VERSIONING = OFF)"
                    ),
                    format!("DROP TABLE IF EXISTS [{schema}].[temporal_records_history]"),
                    format!("DROP TABLE IF EXISTS [{schema}].[temporal_records]"),
                    format!("DROP TABLE IF EXISTS [{schema}].[partitioned_events]"),
                    format!("DROP TABLE IF EXISTS [{schema}].[orders]"),
                    format!("DROP TABLE IF EXISTS [{schema}].[audit_log]"),
                    format!("DROP TABLE IF EXISTS [{schema}].[secured_accounts]"),
                    format!("DROP TABLE IF EXISTS [{schema}].[users]"),
                    format!(
                        "IF EXISTS (SELECT 1 FROM sys.partition_schemes WHERE name = N'{partition_scheme}') DROP PARTITION SCHEME [{partition_scheme}]"
                    ),
                    format!(
                        "IF EXISTS (SELECT 1 FROM sys.partition_functions WHERE name = N'{partition_function}') DROP PARTITION FUNCTION [{partition_function}]"
                    ),
                    format!("DROP SEQUENCE IF EXISTS [{schema}].[order_numbers]"),
                    format!("DROP TYPE IF EXISTS [{schema}].[account_code]"),
                    format!("DROP SCHEMA IF EXISTS [{schema}]"),
                ],
            )
        }

        fn partition_function(&self) -> String {
            format!("pf_{}", self.schema)
        }

        fn partition_scheme(&self) -> String {
            format!("ps_{}", self.schema)
        }

        fn database_trigger(&self) -> String {
            format!("tr_database_{}", self.schema)
        }
    }

    fn drop_table_type_fixture(connection_string: &str, schema: &str) -> Result<(), String> {
        execute_admin_batches(
            connection_string,
            &[
                format!("DROP PROCEDURE IF EXISTS [{schema}].[consume_payload]"),
                format!("DROP TYPE IF EXISTS [{schema}].[payload]"),
                format!("DROP TYPE IF EXISTS [{schema}].[code]"),
                format!("DROP SCHEMA IF EXISTS [{schema}]"),
            ],
        )
    }

    fn drop_xml_metadata_fixture(connection_string: &str, schema: &str) -> Result<(), String> {
        execute_admin_batches(
            connection_string,
            &[
                format!("DROP PROCEDURE IF EXISTS [{schema}].[read_payload]"),
                format!("DROP TABLE IF EXISTS [{schema}].[typed_documents]"),
                format!("DROP TYPE IF EXISTS [{schema}].[payload_type]"),
                format!(
                    "IF EXISTS (SELECT 1 FROM sys.xml_schema_collections xsc \
                     JOIN sys.schemas s ON s.schema_id = xsc.schema_id \
                     WHERE s.name = N'{schema}' AND xsc.name = N'payload_xsd') \
                     DROP XML SCHEMA COLLECTION [{schema}].[payload_xsd]"
                ),
                format!("DROP SCHEMA IF EXISTS [{schema}]"),
            ],
        )
    }

    fn drop_cross_schema_fixture(
        connection_string: &str,
        child_schema: &str,
        parent_schema: &str,
    ) -> Result<(), String> {
        execute_admin_batches(
            connection_string,
            &[
                format!("DROP TABLE IF EXISTS [{child_schema}].[orders]"),
                format!("DROP TABLE IF EXISTS [{parent_schema}].[accounts]"),
                format!("DROP SCHEMA IF EXISTS [{child_schema}]"),
                format!("DROP SCHEMA IF EXISTS [{parent_schema}]"),
            ],
        )
    }

    fn execute_admin_batches(connection_string: &str, batches: &[String]) -> Result<(), String> {
        let connection_string = connection_string.to_owned();
        let batches = batches.to_vec();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(async move {
            let config =
                Config::from_ado_string(&connection_string).map_err(|error| error.to_string())?;
            let tcp = TcpStream::connect(config.get_addr())
                .await
                .map_err(|error| error.to_string())?;
            tcp.set_nodelay(true).map_err(|error| error.to_string())?;
            let mut client = Client::connect(config, tcp.compat_write())
                .await
                .map_err(|error| error.to_string())?;
            for (batch_index, batch) in batches.into_iter().enumerate() {
                client
                    .simple_query(batch)
                    .await
                    .map_err(|error| format!("batch #{} failed: {error}", batch_index + 1))?
                    .into_results()
                    .await
                    .map_err(|error| {
                        format!("batch #{} result failed: {error}", batch_index + 1)
                    })?;
            }
            Ok(())
        })
    }

    fn connection_with_credentials(connection_string: &str, user: &str, password: &str) -> String {
        let mut values = connection_string.parse::<AdoNetString>().unwrap();
        for key in [
            "user",
            "uid",
            "user id",
            "username",
            "password",
            "pwd",
            "integrated security",
            "trusted_connection",
        ] {
            values.remove(key);
        }
        values.insert("user id".to_owned(), user.to_owned());
        values.insert("password".to_owned(), password.to_owned());
        values.to_string()
    }
}
