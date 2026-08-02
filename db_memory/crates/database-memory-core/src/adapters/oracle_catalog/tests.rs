#[cfg(test)]
mod tests {
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::analysis_outcome::{AnalysisFailureCode, AnalysisStatus};

    use super::*;

    #[test]
    fn connection_parser_preserves_password_delimiters() {
        let parsed =
            parse_oracle_connection_string("backend/pa/ss@word@127.0.0.1:1521/FREEPDB1").unwrap();

        assert_eq!(parsed.username, "backend");
        assert_eq!(parsed.password, "pa/ss@word");
        assert_eq!(parsed.connect_string, "127.0.0.1:1521/FREEPDB1");
        assert!(parse_oracle_connection_string("missing-delimiters").is_err());
        assert!(parse_oracle_connection_string("/@host").is_err());
    }

    #[test]
    fn connection_policy_requires_tcps_away_from_loopback() {
        let request = request();

        assert!(
            validate_connection_policy(&request, "backend/secret@127.0.0.1:1521/FREEPDB1").is_ok()
        );
        assert!(validate_connection_policy(&request, "backend/secret@[::1]:1521/FREEPDB1").is_ok());
        assert!(validate_connection_policy(
            &request,
            "backend/secret@tcps://oracle.example.com:1522/FREEPDB1"
        )
        .is_ok());

        let failure =
            validate_connection_policy(&request, "backend/secret@oracle.example.com:1521/FREEPDB1")
                .unwrap_err();
        assert_eq!(failure.code, AnalysisFailureCode::UnsafeSource);
        assert!(!failure.message.contains("secret"));
    }

    #[test]
    fn version_strategy_accepts_only_the_live_certified_release() {
        assert_eq!(
            OracleCatalogVersion::detect(
                &Version::new(23, 26, 2, 0, 0),
                "Oracle AI Database 26ai Free Release 23.26.2.0.0"
            )
            .unwrap(),
            OracleCatalogVersion::Oracle26Ai
        );
        assert!(
            OracleCatalogVersion::detect(&Version::new(19, 0, 0, 0, 0), "Oracle Database 19c")
                .is_err()
        );
        assert!(
            OracleCatalogVersion::detect(&Version::new(23, 0, 0, 0, 0), "Oracle Database 23c")
                .is_err()
        );
    }

    #[test]
    fn stability_gate_rejects_catalog_changes() {
        assert_eq!(
            require_stable_catalog(vec![1, 2], &vec![1, 2]).unwrap(),
            vec![1, 2]
        );
        assert!(matches!(
            require_stable_catalog(vec![1, 2], &vec![1, 3]),
            Err(CatalogError::CatalogChanged(_))
        ));
    }

    #[test]
    fn dynamic_plsql_detection_ignores_literals_and_comments() {
        reject_dynamic_plsql(
            "trigger",
            "STATIC_TRIGGER",
            "BEGIN -- EXECUTE IMMEDIATE ignored\n :NEW.note := q'[DBMS_SQL.PARSE]'; END;",
        )
        .unwrap();
        for body in [
            "BEGIN EXECUTE IMMEDIATE statement_text; END;",
            "BEGIN DBMS_SQL.PARSE(cursor_id, statement_text, DBMS_SQL.NATIVE); END;",
            "BEGIN OPEN result_set FOR statement_text; END;",
            "BEGIN DBMS_UTILITY.EXEC_DDL_STATEMENT(statement_text); END;",
        ] {
            assert!(matches!(
                reject_dynamic_plsql("trigger", "DYNAMIC_TRIGGER", body),
                Err(CatalogError::UnsupportedMetadata(_))
            ));
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_ORACLE_URL"]
    fn oracle_catalog_live_contract_is_env_gated() {
        let admin_url = env::var("DATABASE_MEMORY_TEST_ORACLE_URL")
            .expect("live Oracle test requires DATABASE_MEMORY_TEST_ORACLE_URL");
        let parsed = parse_oracle_connection_string(&admin_url).unwrap();
        let connect_string = parsed.connect_string.to_owned();
        let admin = Connection::connect(parsed.username, parsed.password, parsed.connect_string)
            .expect("connect to Oracle certification database");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let username = format!("DBMCP_T{}_{}", std::process::id(), suffix);
        let password = "DbmcpTest1!";
        admin
            .execute(
                &format!(
                    "CREATE USER {username} IDENTIFIED BY \"{password}\" DEFAULT TABLESPACE USERS QUOTA UNLIMITED ON USERS"
                ),
                &[],
            )
            .expect("create isolated Oracle test user");
        let cleanup = TestUserGuard { admin, username };
        cleanup
            .admin
            .execute(
                &format!(
                    "GRANT CREATE SESSION, CREATE TABLE, CREATE SEQUENCE, CREATE VIEW, CREATE MATERIALIZED VIEW, CREATE TRIGGER, CREATE PROCEDURE, CREATE SYNONYM, CREATE TYPE, CREATE DATABASE LINK, ADMINISTER DATABASE TRIGGER TO {}",
                    cleanup.username
                ),
                &[],
            )
            .expect("grant metadata fixture privileges");

        let user_url = format!("{}/{password}@{connect_string}", cleanup.username);
        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("connect as isolated Oracle test user");
        setup
            .execute(
                "CREATE TABLE PARENT_ENTITY (ID NUMBER GENERATED BY DEFAULT AS IDENTITY, CODE VARCHAR2(32 CHAR), CONSTRAINT PK_PARENT_ENTITY PRIMARY KEY (ID), CONSTRAINT UQ_PARENT_ENTITY_CODE UNIQUE (CODE), CONSTRAINT CK_PARENT_ENTITY_ID CHECK (ID > 0))",
                &[],
            )
            .expect("create parent table");
        setup
            .execute(
                "CREATE TABLE CHILD_ENTITY (ID NUMBER(10), PARENT_ID NUMBER(10), LABEL VARCHAR2(64 CHAR) DEFAULT 'new', CONSTRAINT PK_CHILD_ENTITY PRIMARY KEY (ID), CONSTRAINT FK_CHILD_PARENT FOREIGN KEY (PARENT_ID) REFERENCES PARENT_ENTITY (ID) ON DELETE CASCADE)",
                &[],
            )
            .expect("create child table");
        setup
            .execute(
                "CREATE INDEX IX_CHILD_PARENT ON CHILD_ENTITY (PARENT_ID)",
                &[],
            )
            .expect("create secondary index");
        setup
            .execute(
                "CREATE INDEX IX_PARENT_CODE_SEARCH ON PARENT_ENTITY (UPPER(CODE), ID)",
                &[],
            )
            .expect("create function-based index");
        setup
            .execute(
                "CREATE BITMAP INDEX IX_CHILD_LABEL_BITMAP ON CHILD_ENTITY (LABEL)",
                &[],
            )
            .expect("create bitmap index");
        setup
            .execute(
                "CREATE BITMAP INDEX IX_CHILD_LABEL_BITMAP_FN ON CHILD_ENTITY (UPPER(LABEL))",
                &[],
            )
            .expect("create function-based bitmap index");
        setup
            .execute(
                "CREATE INDEX IX_CHILD_LABEL_DESC ON CHILD_ENTITY (LABEL DESC)",
                &[],
            )
            .expect("create descending function-based index");
        setup
            .execute("CREATE SEQUENCE AUDIT_SEQUENCE START WITH 10", &[])
            .expect("create explicit sequence");
        setup
            .execute(
                "CREATE TYPE ADDRESS_T AS OBJECT (STREET VARCHAR2(100), ZIP_CODE VARCHAR2(12))",
                &[],
            )
            .expect("create object type");
        setup
            .execute("CREATE TYPE ADDRESS_LIST_T AS TABLE OF ADDRESS_T", &[])
            .expect("create nested-table type");
        setup
            .execute("CREATE TYPE TAG_LIST_T AS VARRAY(5) OF VARCHAR2(30)", &[])
            .expect("create varray type");
        setup
            .execute(
                "CREATE TYPE PERSON_T AS OBJECT (NAME VARCHAR2(100), ADDRESS ADDRESS_T, MEMBER FUNCTION DISPLAY_NAME(P_PREFIX VARCHAR2) RETURN VARCHAR2) NOT FINAL",
                &[],
            )
            .expect("create object type with method");
        setup
            .execute(
                "CREATE TYPE BODY PERSON_T AS MEMBER FUNCTION DISPLAY_NAME(P_PREFIX VARCHAR2) RETURN VARCHAR2 IS BEGIN RETURN P_PREFIX || NAME; END; END;",
                &[],
            )
            .expect("create object type body");
        setup
            .execute(
                "CREATE TYPE EMPLOYEE_T UNDER PERSON_T (EMPLOYEE_NO NUMBER)",
                &[],
            )
            .expect("create object subtype");
        setup
            .execute(
                "CREATE TABLE TYPE_USAGE (ID NUMBER, ADDRESS ADDRESS_T, TAGS TAG_LIST_T)",
                &[],
            )
            .expect("create typed-column table");
        setup
            .execute(
                "CREATE TABLE LOB_DOCUMENTS (ID NUMBER, CONTENT CLOB, BINARY_CONTENT BLOB)",
                &[],
            )
            .expect("create unpartitioned LOB table");
        setup
            .execute(
                "CREATE TABLE PARTITIONED_EVENTS (ID NUMBER, EVENT_DATE DATE, REGION VARCHAR2(10), PAYLOAD CLOB) LOB (PAYLOAD) STORE AS SECUREFILE PARTITION BY RANGE (EVENT_DATE) SUBPARTITION BY HASH (REGION) SUBPARTITIONS 2 (PARTITION P_2025 VALUES LESS THAN (DATE '2026-01-01'), PARTITION P_MAX VALUES LESS THAN (MAXVALUE))",
                &[],
            )
            .expect("create composite-partitioned table");
        setup
            .execute(
                "CREATE INDEX IX_PART_EVENTS_LOCAL ON PARTITIONED_EVENTS (EVENT_DATE) LOCAL",
                &[],
            )
            .expect("create local composite-partitioned index");
        setup
            .execute(
                "CREATE INDEX IX_PART_EVENTS_GLOBAL ON PARTITIONED_EVENTS (ID) GLOBAL PARTITION BY RANGE (ID) (PARTITION IP_LOW VALUES LESS THAN (1000), PARTITION IP_MAX VALUES LESS THAN (MAXVALUE))",
                &[],
            )
            .expect("create global partitioned index");
        setup
            .execute(
                "CREATE VIEW ACTIVE_PARENT AS SELECT ID, CODE FROM PARENT_ENTITY WHERE ID > 0",
                &[],
            )
            .expect("create view");
        setup
            .execute(
                "CREATE OR REPLACE FUNCTION NORMALIZE_LABEL(P_LABEL IN VARCHAR2) RETURN VARCHAR2 DETERMINISTIC AUTHID CURRENT_USER AS BEGIN RETURN UPPER(P_LABEL); END;",
                &[],
            )
            .expect("create standalone function");
        setup
            .execute(
                "CREATE OR REPLACE FUNCTION ECHO_ADDRESS(P_ADDRESS IN ADDRESS_T) RETURN ADDRESS_T AUTHID DEFINER AS BEGIN RETURN P_ADDRESS; END;",
                &[],
            )
            .expect("create standalone function using an object type");
        setup
            .execute(
                "CREATE OR REPLACE PROCEDURE UPDATE_CHILD_LABEL(P_ID IN NUMBER, P_LABEL IN VARCHAR2 DEFAULT 'new', P_ROWS OUT NUMBER) AUTHID DEFINER AS BEGIN UPDATE CHILD_ENTITY SET LABEL = P_LABEL WHERE ID = P_ID; P_ROWS := SQL%ROWCOUNT; END;",
                &[],
            )
            .expect("create standalone procedure");
        setup
            .execute(
                "CREATE OR REPLACE PACKAGE ITEM_API AUTHID DEFINER AS PROCEDURE TOUCH(P_ID IN NUMBER); PROCEDURE TOUCH(P_LABEL IN VARCHAR2); FUNCTION LABEL_FOR(P_ID IN NUMBER) RETURN VARCHAR2; END ITEM_API;",
                &[],
            )
            .expect("create package specification");
        setup
            .execute(
                "CREATE OR REPLACE PACKAGE BODY ITEM_API AS PROCEDURE PRIVATE_HELPER(P_TEXT IN VARCHAR2) AS BEGIN NULL; END; PROCEDURE TOUCH(P_ID IN NUMBER) AS BEGIN UPDATE CHILD_ENTITY SET LABEL = LABEL WHERE ID = P_ID; END; PROCEDURE TOUCH(P_LABEL IN VARCHAR2) AS BEGIN UPDATE CHILD_ENTITY SET LABEL = P_LABEL; END; FUNCTION LABEL_FOR(P_ID IN NUMBER) RETURN VARCHAR2 AS V_LABEL VARCHAR2(64); BEGIN SELECT LABEL INTO V_LABEL FROM CHILD_ENTITY WHERE ID = P_ID; PRIVATE_HELPER(V_LABEL); RETURN V_LABEL; END; END ITEM_API;",
                &[],
            )
            .expect("create package body");
        for statement in [
            "CREATE SYNONYM CHILD_ALIAS FOR CHILD_ENTITY",
            "CREATE SYNONYM ACTIVE_PARENT_ALIAS FOR ACTIVE_PARENT",
            "CREATE SYNONYM AUDIT_SEQUENCE_ALIAS FOR AUDIT_SEQUENCE",
            "CREATE SYNONYM NORMALIZE_LABEL_ALIAS FOR NORMALIZE_LABEL",
            "CREATE SYNONYM ITEM_API_ALIAS FOR ITEM_API",
            "CREATE SYNONYM CHILD_ALIAS_CHAIN FOR CHILD_ALIAS",
        ] {
            setup.execute(statement, &[]).expect("create local synonym");
        }
        drop(setup);

        let timed_out = analyze_oracle(&user_url, "oracle-timeout-live", Vec::new(), Vec::new(), 1);
        assert_eq!(timed_out.status(), AnalysisStatus::Failed);
        let timed_out_failure = timed_out.failure().expect("bounded deadline must fail");
        assert_eq!(
            timed_out_failure.code,
            AnalysisFailureCode::Timeout,
            "unexpected Oracle timeout failure: {timed_out_failure:?}"
        );
        assert!(timed_out.certified_snapshot().is_none());

        let reader = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("connect Oracle read-only stability reader");
        reader
            .set_call_timeout(Some(Duration::from_secs(30)))
            .expect("set Oracle stability reader timeout");
        reader
            .execute("SET TRANSACTION READ ONLY", &[])
            .expect("start Oracle read-only stability transaction");
        let deadline = Instant::now() + Duration::from_secs(30);
        let facts = ServerFacts::read(&reader, deadline).expect("read Oracle stability facts");
        let scope = DictionaryScope::select(&reader, &request(), &facts, deadline)
            .expect("select Oracle stability dictionary scope");
        let before = RawOracleCatalog::read(&reader, &scope, deadline)
            .expect("read Oracle catalog before concurrent DDL");

        let mutator = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("connect Oracle catalog mutator");
        mutator
            .execute("CREATE TABLE CATALOG_MUTATION_PROBE (ID NUMBER)", &[])
            .expect("create concurrent Oracle catalog mutation");
        let during = RawOracleCatalog::read(&reader, &scope, deadline)
            .expect("read Oracle catalog after concurrent DDL in the same snapshot");
        assert_eq!(before, during);
        reader
            .rollback()
            .expect("finish Oracle read-only stability transaction");

        reader
            .execute("SET TRANSACTION READ ONLY", &[])
            .expect("start fresh Oracle read-only transaction");
        let deadline = Instant::now() + Duration::from_secs(30);
        let facts =
            ServerFacts::read(&reader, deadline).expect("read fresh Oracle stability facts");
        let scope = DictionaryScope::select(&reader, &request(), &facts, deadline)
            .expect("select fresh Oracle stability dictionary scope");
        let after = RawOracleCatalog::read(&reader, &scope, deadline)
            .expect("read Oracle catalog in a fresh snapshot");
        assert_ne!(before, after);
        reader
            .rollback()
            .expect("finish fresh Oracle read-only transaction");
        mutator
            .execute("DROP TABLE CATALOG_MUTATION_PROBE PURGE", &[])
            .expect("drop concurrent Oracle catalog mutation");

        let complete = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(
            complete.status(),
            AnalysisStatus::Complete,
            "Oracle live analysis failed: {:?}",
            complete.failure()
        );
        let certified = complete
            .certified_snapshot()
            .expect("simple Oracle schema must be certified");
        let dba_complete = analyze_oracle(
            &admin_url,
            "oracle-dba-live",
            Vec::new(),
            vec![cleanup.username.clone()],
            30_000,
        );
        assert_eq!(
            dba_complete.status(),
            AnalysisStatus::Complete,
            "Oracle DBA-scope analysis failed: {:?}",
            dba_complete.failure()
        );
        let dba_certified = dba_complete
            .certified_snapshot()
            .expect("DBA-scoped Oracle schema must be certified");
        assert_eq!(dba_certified.snapshot.schema.tables.len(), 5);
        assert_eq!(
            dba_certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| { object.extension_kind.as_deref() == Some("oracle_lob_storage") })
                .count(),
            3
        );
        assert!(dba_certified.snapshot.schema.indexes.iter().any(|index| {
            index.name == "IX_PARENT_CODE_SEARCH"
                && index
                    .expression
                    .as_deref()
                    .is_some_and(|expression| expression.contains("UPPER"))
        }));
        assert_eq!(certified.snapshot.schema.tables.len(), 5);
        assert!(certified
            .snapshot
            .schema
            .constraints
            .iter()
            .any(|constraint| constraint.kind == ConstraintKind::ForeignKey));
        assert!(certified.snapshot.schema.indexes.len() >= 7);
        let function_index = certified
            .snapshot
            .schema
            .indexes
            .iter()
            .find(|index| index.name == "IX_PARENT_CODE_SEARCH")
            .expect("function-based index is mapped");
        assert_eq!(function_index.columns.len(), 1);
        assert_eq!(function_index.columns[0].sub_object.as_deref(), Some("ID"));
        assert!(function_index
            .expression
            .as_deref()
            .is_some_and(|expression| expression.contains("UPPER") && expression.contains("CODE")));
        let function_index_annotation = certified
            .snapshot
            .metadata
            .annotations
            .iter()
            .find(|annotation| annotation.object_key == function_index.key)
            .expect("function-based index evidence is mapped");
        assert!(matches!(
            function_index_annotation.properties.get("index_type"),
            Some(MetadataValue::String(value)) if value == "FUNCTION-BASED NORMAL"
        ));
        assert!(matches!(
            function_index_annotation.properties.get("function_status"),
            Some(MetadataValue::String(value)) if value == "ENABLED"
        ));
        assert!(matches!(
            function_index_annotation.properties.get("key_parts"),
            Some(MetadataValue::StringList(parts))
                if parts.len() == 2 && parts[0].contains("UPPER") && parts[1] == "ID"
        ));
        let bitmap_index = certified
            .snapshot
            .schema
            .indexes
            .iter()
            .find(|index| index.name == "IX_CHILD_LABEL_BITMAP")
            .expect("bitmap index is mapped");
        assert_eq!(bitmap_index.columns.len(), 1);
        assert_eq!(bitmap_index.columns[0].sub_object.as_deref(), Some("LABEL"));
        assert!(bitmap_index.expression.is_none());
        let function_bitmap_index = certified
            .snapshot
            .schema
            .indexes
            .iter()
            .find(|index| index.name == "IX_CHILD_LABEL_BITMAP_FN")
            .expect("function-based bitmap index is mapped");
        assert!(function_bitmap_index.columns.is_empty());
        assert!(function_bitmap_index
            .expression
            .as_deref()
            .is_some_and(|expression| expression.contains("UPPER")));
        let descending_index = certified
            .snapshot
            .schema
            .indexes
            .iter()
            .find(|index| index.name == "IX_CHILD_LABEL_DESC")
            .expect("descending index is mapped");
        assert!(descending_index.columns.is_empty());
        assert!(
            descending_index
                .expression
                .as_deref()
                .is_some_and(
                    |expression| expression.contains("LABEL") && expression.ends_with("DESC")
                )
        );
        assert!(
            certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| object.key.object_kind == ObjectKind::Sequence)
                .count()
                >= 2
        );
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| relationship.kind == MetadataRelationshipKind::UsesSequence));
        let view = certified
            .snapshot
            .schema
            .views
            .iter()
            .find(|view| view.name == "ACTIVE_PARENT")
            .expect("view is mapped");
        assert!(view
            .depends_on
            .iter()
            .any(|key| key.object_kind == ObjectKind::Table && key.object_name == "PARENT_ENTITY"));
        assert_eq!(
            certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| object.key.object_kind == ObjectKind::ViewColumn)
                .count(),
            2
        );
        assert_eq!(certified.snapshot.schema.routines.len(), 3);
        let procedure = certified
            .snapshot
            .schema
            .routines
            .iter()
            .find(|routine| routine.name == "UPDATE_CHILD_LABEL")
            .expect("standalone procedure is mapped");
        assert!(procedure
            .depends_on
            .iter()
            .any(|key| key.object_kind == ObjectKind::Table && key.object_name == "CHILD_ENTITY"));
        assert_eq!(
            certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| object.key.object_kind == ObjectKind::RoutineParameter)
                .count(),
            17
        );
        let user_types = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| object.key.object_kind == ObjectKind::UserDefinedType)
            .collect::<Vec<_>>();
        assert_eq!(user_types.len(), 5);
        let person_type = user_types
            .iter()
            .find(|object| object.name == "PERSON_T")
            .expect("object type is mapped");
        assert!(person_type
            .definition
            .as_deref()
            .is_some_and(|definition| definition.contains("TYPE BODY PERSON_T")));
        assert!(matches!(
            person_type.properties.get("has_body"),
            Some(MetadataValue::Boolean(true))
        ));
        let employee_type = user_types
            .iter()
            .find(|object| object.name == "EMPLOYEE_T")
            .expect("object subtype is mapped");
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::InheritsFrom
                    && relationship.from_key == employee_type.key
                    && relationship.to_key == person_type.key
            }));
        let address_type = user_types
            .iter()
            .find(|object| object.name == "ADDRESS_T")
            .expect("referenced object type is mapped");
        let address_list_type = user_types
            .iter()
            .find(|object| object.name == "ADDRESS_LIST_T")
            .expect("nested-table type is mapped");
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::UsesType
                    && relationship.from_key == address_list_type.key
                    && relationship.to_key == address_type.key
            }));
        let person_address_attribute = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::Extension
                    && object.extension_kind.as_deref() == Some("oracle_type_attribute")
                    && object.parent_key.as_ref() == Some(&person_type.key)
                    && object.name == "ADDRESS"
            })
            .expect("object type attribute is mapped");
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::UsesType
                    && relationship.from_key == person_address_attribute.key
                    && relationship.to_key == address_type.key
            }));
        let person_method = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::Routine
                    && object.parent_key.as_ref() == Some(&person_type.key)
                    && object.name == "DISPLAY_NAME"
            })
            .expect("object type method is mapped");
        assert_eq!(
            certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| {
                    object.key.object_kind == ObjectKind::RoutineParameter
                        && object.parent_key.as_ref() == Some(&person_method.key)
                })
                .count(),
            3
        );
        let type_usage = certified
            .snapshot
            .schema
            .tables
            .iter()
            .find(|table| table.name == "TYPE_USAGE")
            .expect("typed-column table is mapped");
        for (column_name, target_name) in [("ADDRESS", "ADDRESS_T"), ("TAGS", "TAG_LIST_T")] {
            let column = certified
                .snapshot
                .schema
                .columns
                .iter()
                .find(|column| column.table_key == type_usage.key && column.name == column_name)
                .expect("typed column is mapped");
            assert!(certified
                .snapshot
                .metadata
                .relationships
                .iter()
                .any(|relationship| {
                    relationship.kind == MetadataRelationshipKind::UsesType
                        && relationship.from_key == column.key
                        && relationship.to_key.object_kind == ObjectKind::UserDefinedType
                        && relationship.to_key.object_name == target_name
                }));
        }
        let echo_address = certified
            .snapshot
            .schema
            .routines
            .iter()
            .find(|routine| routine.name == "ECHO_ADDRESS")
            .expect("routine using an object type is mapped");
        assert_eq!(
            certified
                .snapshot
                .metadata
                .relationships
                .iter()
                .filter(|relationship| {
                    relationship.kind == MetadataRelationshipKind::UsesType
                        && relationship.to_key == address_type.key
                        && certified.snapshot.metadata.objects.iter().any(|object| {
                            object.key == relationship.from_key
                                && object.parent_key.as_ref() == Some(&echo_address.key)
                        })
                })
                .count(),
            2
        );
        let partitioned_table = certified
            .snapshot
            .schema
            .tables
            .iter()
            .find(|table| table.name == "PARTITIONED_EVENTS")
            .expect("partitioned table is mapped");
        assert_eq!(partitioned_table.kind, TableKind::Partitioned);
        let partitioned_table_annotation = certified
            .snapshot
            .metadata
            .annotations
            .iter()
            .find(|annotation| annotation.object_key == partitioned_table.key)
            .expect("partitioned table annotation is mapped");
        assert!(matches!(
            partitioned_table_annotation
                .properties
                .get("partition_key_columns"),
            Some(MetadataValue::StringList(columns)) if columns == &["EVENT_DATE"]
        ));
        assert!(matches!(
            partitioned_table_annotation
                .properties
                .get("subpartition_key_columns"),
            Some(MetadataValue::StringList(columns)) if columns == &["REGION"]
        ));
        let table_partitions = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| {
                object.extension_kind.as_deref() == Some("oracle_table_partition")
                    && object.parent_key.as_ref() == Some(&partitioned_table.key)
            })
            .collect::<Vec<_>>();
        assert_eq!(table_partitions.len(), 2);
        assert!(table_partitions.iter().any(|partition| {
            partition.name == "P_MAX" && partition.definition.as_deref() == Some("MAXVALUE")
        }));
        let table_subpartitions = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| {
                object.extension_kind.as_deref() == Some("oracle_table_subpartition")
                    && object.parent_key.as_ref().is_some_and(|parent| {
                        table_partitions
                            .iter()
                            .any(|partition| partition.key == *parent)
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(table_subpartitions.len(), 4);
        let lob_storage = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| object.extension_kind.as_deref() == Some("oracle_lob_storage"))
            .collect::<Vec<_>>();
        assert_eq!(lob_storage.len(), 3);
        let payload_column = certified
            .snapshot
            .schema
            .columns
            .iter()
            .find(|column| column.table_key == partitioned_table.key && column.name == "PAYLOAD")
            .expect("partitioned LOB column is mapped");
        let payload_lob = lob_storage
            .iter()
            .find(|object| object.parent_key.as_ref() == Some(&payload_column.key))
            .expect("partitioned LOB storage is mapped");
        assert!(matches!(
            payload_lob.properties.get("partitioned"),
            Some(MetadataValue::Boolean(true))
        ));
        assert!(matches!(
            payload_lob.properties.get("securefile"),
            Some(MetadataValue::Boolean(true))
        ));
        let lob_partitions = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| {
                object.extension_kind.as_deref() == Some("oracle_lob_partition")
                    && object.parent_key.as_ref() == Some(&payload_lob.key)
            })
            .collect::<Vec<_>>();
        assert_eq!(lob_partitions.len(), 2);
        for partition in &lob_partitions {
            assert!(certified
                .snapshot
                .metadata
                .relationships
                .iter()
                .any(|relationship| {
                    matches!(
                        &relationship.kind,
                        MetadataRelationshipKind::Extension(kind)
                            if kind == "oracle_lob_partition_storage"
                    ) && relationship.from_key == partition.key
                        && table_partitions
                            .iter()
                            .any(|table_partition| table_partition.key == relationship.to_key)
                }));
        }
        let lob_subpartitions = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| {
                object.extension_kind.as_deref() == Some("oracle_lob_subpartition")
                    && object.parent_key.as_ref().is_some_and(|parent| {
                        lob_partitions
                            .iter()
                            .any(|partition| partition.key == *parent)
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(lob_subpartitions.len(), 4);
        for subpartition in &lob_subpartitions {
            assert!(certified
                .snapshot
                .metadata
                .relationships
                .iter()
                .any(|relationship| {
                    matches!(
                        &relationship.kind,
                        MetadataRelationshipKind::Extension(kind)
                            if kind == "oracle_lob_subpartition_storage"
                    ) && relationship.from_key == subpartition.key
                        && table_subpartitions
                            .iter()
                            .any(|table_subpartition| table_subpartition.key == relationship.to_key)
                }));
        }
        for storage in &lob_storage {
            let Some(MetadataValue::String(index_name)) = storage.properties.get("index_name")
            else {
                panic!("LOB storage must expose its generated index name");
            };
            assert!(certified
                .snapshot
                .schema
                .indexes
                .iter()
                .all(|index| index.name != *index_name));
        }
        assert_eq!(
            lob_storage
                .iter()
                .filter(|storage| matches!(
                    storage.properties.get("partitioned"),
                    Some(MetadataValue::Boolean(false))
                ))
                .count(),
            2
        );
        for (index_name, locality, partition_count, subpartition_count) in [
            ("IX_PART_EVENTS_LOCAL", "LOCAL", 2, 4),
            ("IX_PART_EVENTS_GLOBAL", "GLOBAL", 2, 0),
        ] {
            let index = certified
                .snapshot
                .schema
                .indexes
                .iter()
                .find(|index| index.name == index_name)
                .expect("partitioned index is mapped");
            let annotation = certified
                .snapshot
                .metadata
                .annotations
                .iter()
                .find(|annotation| annotation.object_key == index.key)
                .expect("partitioned index annotation is mapped");
            assert!(matches!(
                annotation.properties.get("locality"),
                Some(MetadataValue::String(value)) if value == locality
            ));
            let partitions = certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| {
                    object.extension_kind.as_deref() == Some("oracle_index_partition")
                        && object.parent_key.as_ref() == Some(&index.key)
                })
                .collect::<Vec<_>>();
            assert_eq!(partitions.len(), partition_count);
            assert_eq!(
                certified
                    .snapshot
                    .metadata
                    .objects
                    .iter()
                    .filter(|object| {
                        object.extension_kind.as_deref() == Some("oracle_index_subpartition")
                            && object.parent_key.as_ref().is_some_and(|parent| {
                                partitions.iter().any(|partition| partition.key == *parent)
                            })
                    })
                    .count(),
                subpartition_count
            );
        }
        let package = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::Package && object.name == "ITEM_API"
            })
            .expect("package is mapped");
        assert!(package
            .definition
            .as_deref()
            .is_some_and(|definition| definition.contains("PRIVATE_HELPER")));
        let packaged_routines = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .filter(|object| {
                object.key.object_kind == ObjectKind::Routine
                    && object.parent_key.as_ref() == Some(&package.key)
            })
            .collect::<Vec<_>>();
        assert_eq!(packaged_routines.len(), 3);
        assert_eq!(
            packaged_routines
                .iter()
                .filter(|routine| routine.name == "TOUCH")
                .map(|routine| routine.key.to_string())
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::DependsOn
                    && relationship.from_key == package.key
                    && relationship.to_key.object_kind == ObjectKind::Table
                    && relationship.to_key.object_name == "CHILD_ENTITY"
            }));
        for (synonym_name, target_kind, target_name) in [
            ("CHILD_ALIAS", ObjectKind::Table, "CHILD_ENTITY"),
            ("ACTIVE_PARENT_ALIAS", ObjectKind::View, "ACTIVE_PARENT"),
            (
                "AUDIT_SEQUENCE_ALIAS",
                ObjectKind::Sequence,
                "AUDIT_SEQUENCE",
            ),
            (
                "NORMALIZE_LABEL_ALIAS",
                ObjectKind::Routine,
                "NORMALIZE_LABEL",
            ),
            ("ITEM_API_ALIAS", ObjectKind::Package, "ITEM_API"),
            ("CHILD_ALIAS_CHAIN", ObjectKind::Synonym, "CHILD_ALIAS"),
        ] {
            let synonym = certified
                .snapshot
                .metadata
                .objects
                .iter()
                .find(|object| {
                    object.key.object_kind == ObjectKind::Synonym && object.name == synonym_name
                })
                .expect("synonym is mapped");
            assert!(certified
                .snapshot
                .metadata
                .relationships
                .iter()
                .any(|relationship| {
                    relationship.kind == MetadataRelationshipKind::SynonymFor
                        && relationship.from_key == synonym.key
                        && relationship.to_key.object_kind == target_kind
                        && relationship.to_key.object_name == target_name
                }));
        }

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute(
                "CREATE MATERIALIZED VIEW PARENT_SUMMARY_MV BUILD IMMEDIATE REFRESH COMPLETE ON DEMAND AS SELECT ID, CODE FROM PARENT_ENTITY",
                &[],
            )
            .expect("create materialized view");
        drop(setup);

        let complete = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(
            complete.status(),
            AnalysisStatus::Complete,
            "Oracle materialized-view analysis failed: {:?}",
            complete.failure()
        );
        let certified = complete
            .certified_snapshot()
            .expect("Oracle materialized-view schema must be certified");
        assert_eq!(certified.snapshot.schema.tables.len(), 5);
        assert!(certified
            .snapshot
            .schema
            .tables
            .iter()
            .all(|table| table.name != "PARENT_SUMMARY_MV"));
        let materialized_view = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::MaterializedView
                    && object.name == "PARENT_SUMMARY_MV"
            })
            .expect("materialized view is mapped");
        assert_eq!(
            certified
                .snapshot
                .metadata
                .objects
                .iter()
                .filter(|object| {
                    object.key.object_kind == ObjectKind::ViewColumn
                        && object.parent_key.as_ref() == Some(&materialized_view.key)
                })
                .count(),
            2
        );
        assert!(certified.snapshot.metadata.objects.iter().any(|object| {
            object.key.object_kind == ObjectKind::Index
                && object.parent_key.as_ref() == Some(&materialized_view.key)
        }));
        assert!(certified.snapshot.metadata.objects.iter().any(|object| {
            object.key.object_kind == ObjectKind::PrimaryKey
                && object.parent_key.as_ref() == Some(&materialized_view.key)
        }));
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::Materializes
                    && relationship.from_key == materialized_view.key
                    && relationship.to_key.object_kind == ObjectKind::Table
                    && relationship.to_key.object_name == "PARENT_ENTITY"
            }));

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute(
                "CREATE OR REPLACE PROCEDURE LOG_CHILD(P_ID IN NUMBER) AS BEGIN NULL; END;",
                &[],
            )
            .expect("create CALL trigger routine");
        setup
            .execute(
                "CREATE OR REPLACE TRIGGER CHILD_LABEL_BIU BEFORE INSERT OR UPDATE ON CHILD_ENTITY FOR EACH ROW BEGIN :NEW.LABEL := NORMALIZE_LABEL(:NEW.LABEL); END;",
                &[],
            )
            .expect("create static trigger");
        setup
            .execute(
                "CREATE OR REPLACE TRIGGER ACTIVE_PARENT_IO INSTEAD OF INSERT ON ACTIVE_PARENT FOR EACH ROW BEGIN INSERT INTO PARENT_ENTITY (ID, CODE) VALUES (:NEW.ID, :NEW.CODE); END;",
                &[],
            )
            .expect("create view trigger");
        setup
            .execute(
                "CREATE OR REPLACE TRIGGER CHILD_LOG_CALL AFTER INSERT ON CHILD_ENTITY FOR EACH ROW CALL LOG_CHILD(:NEW.ID)",
                &[],
            )
            .expect("create CALL trigger");
        setup
            .execute(
                "CREATE OR REPLACE TRIGGER SCHEMA_DDL_AUDIT AFTER CREATE ON SCHEMA BEGIN NULL; END;",
                &[],
            )
            .expect("create schema trigger");
        setup
            .execute(
                "CREATE OR REPLACE TRIGGER DATABASE_ERROR_AUDIT AFTER SERVERERROR ON DATABASE BEGIN NULL; END;",
                &[],
            )
            .expect("create database trigger");
        setup
            .execute("ALTER TRIGGER DATABASE_ERROR_AUDIT DISABLE", &[])
            .expect("disable database trigger fixture");
        drop(setup);

        let complete = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(
            complete.status(),
            AnalysisStatus::Complete,
            "Oracle trigger analysis failed: {:?}",
            complete.failure()
        );
        let certified = complete
            .certified_snapshot()
            .expect("Oracle trigger schema must be certified");
        let trigger = certified
            .snapshot
            .schema
            .triggers
            .iter()
            .find(|trigger| trigger.name == "CHILD_LABEL_BIU")
            .expect("static trigger is mapped");
        assert_eq!(trigger.timing.as_deref(), Some("BEFORE"));
        assert_eq!(trigger.events, ["INSERT", "UPDATE"]);
        assert_eq!(trigger.table_key.object_name, "CHILD_ENTITY");
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::Invokes
                    && relationship.from_key == trigger.key
                    && relationship.to_key.object_kind == ObjectKind::Routine
                    && relationship.to_key.object_name == "NORMALIZE_LABEL"
            }));
        let view_trigger = certified
            .snapshot
            .schema
            .triggers
            .iter()
            .find(|trigger| trigger.name == "ACTIVE_PARENT_IO")
            .expect("view trigger is mapped");
        assert_eq!(view_trigger.table_key.object_kind, ObjectKind::View);
        assert_eq!(view_trigger.table_key.object_name, "ACTIVE_PARENT");
        let call_trigger = certified
            .snapshot
            .schema
            .triggers
            .iter()
            .find(|trigger| trigger.name == "CHILD_LOG_CALL")
            .expect("CALL trigger is mapped");
        assert!(certified
            .snapshot
            .metadata
            .annotations
            .iter()
            .any(|annotation| {
                annotation.object_key == call_trigger.key
                    && matches!(
                        annotation.properties.get("action_type"),
                        Some(MetadataValue::String(value)) if value == "CALL"
                    )
            }));
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::Invokes
                    && relationship.from_key == call_trigger.key
                    && relationship.to_key.object_kind == ObjectKind::Routine
                    && relationship.to_key.object_name == "LOG_CHILD"
            }));
        let schema_trigger = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::Trigger && object.name == "SCHEMA_DDL_AUDIT"
            })
            .expect("schema trigger is mapped");
        assert_eq!(
            schema_trigger
                .parent_key
                .as_ref()
                .expect("schema trigger parent")
                .object_kind,
            ObjectKind::Schema
        );
        let database_trigger = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::Trigger
                    && object.name == "DATABASE_ERROR_AUDIT"
            })
            .expect("database trigger is mapped");
        assert_eq!(
            database_trigger
                .parent_key
                .as_ref()
                .expect("database trigger parent"),
            &certified.snapshot.schema.database.key
        );
        assert!(matches!(
            database_trigger.properties.get("status"),
            Some(MetadataValue::String(value)) if value == "DISABLED"
        ));

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute(
                &format!(
                    "CREATE DATABASE LINK REMOTE_LOOPBACK CONNECT TO {} IDENTIFIED BY \"{password}\" USING '(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=127.0.0.1)(PORT=1521))(CONNECT_DATA=(SERVICE_NAME=FREEPDB1)))'",
                    cleanup.username
                ),
                &[],
            )
            .expect("create remote database-link fixture");
        setup
            .execute(
                "CREATE SYNONYM REMOTE_CHILD_ALIAS FOR CHILD_ENTITY@REMOTE_LOOPBACK",
                &[],
            )
            .expect("create remote synonym fixture");
        drop(setup);

        let failed = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(failed.status(), AnalysisStatus::Failed);
        let remote_failure = failed.failure().expect("remote link must fail");
        assert_eq!(
            remote_failure.code,
            AnalysisFailureCode::UnsupportedMetadata,
            "unexpected Oracle remote-link failure: {remote_failure:?}"
        );
        assert!(failed.certified_snapshot().is_none());

        let dba_failed = analyze_oracle(
            &admin_url,
            "oracle-dba-remote-link",
            Vec::new(),
            vec![cleanup.username.clone()],
            30_000,
        );
        assert_eq!(dba_failed.status(), AnalysisStatus::Failed);
        assert_eq!(
            dba_failed
                .failure()
                .expect("DBA-scoped remote link must fail")
                .code,
            AnalysisFailureCode::UnsupportedMetadata
        );
        assert!(dba_failed.certified_snapshot().is_none());

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute("DROP SYNONYM REMOTE_CHILD_ALIAS", &[])
            .expect("drop remote synonym fixture");
        setup
            .execute("DROP DATABASE LINK REMOTE_LOOPBACK", &[])
            .expect("drop remote database-link fixture");
        setup
            .execute(
                "CREATE FORCE VIEW INVALID_PARENT AS SELECT ID FROM MISSING_PARENT",
                &[],
            )
            .expect("create invalid Oracle object fixture");
        drop(setup);

        let failed = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(failed.status(), AnalysisStatus::Failed);
        assert_eq!(
            failed.failure().expect("failed outcome").code,
            AnalysisFailureCode::UnsupportedMetadata
        );
        assert!(failed.certified_snapshot().is_none());

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute("DROP VIEW INVALID_PARENT", &[])
            .expect("drop invalid Oracle object fixture");
        setup
            .execute("CREATE SYNONYM MISSING_ALIAS FOR MISSING_TARGET", &[])
            .expect("create unresolved synonym fixture");
        drop(setup);

        let failed = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(failed.status(), AnalysisStatus::Failed);
        assert!(failed.certified_snapshot().is_none());

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute("DROP SYNONYM MISSING_ALIAS", &[])
            .expect("drop unresolved synonym fixture");
        setup
            .execute(
                "CREATE TYPE DYNAMIC_TYPE_T AS OBJECT (PAYLOAD VARCHAR2(20), MEMBER PROCEDURE RUN_STATEMENT)",
                &[],
            )
            .expect("create dynamic type fixture specification");
        setup
            .execute(
                "CREATE TYPE BODY DYNAMIC_TYPE_T AS MEMBER PROCEDURE RUN_STATEMENT IS BEGIN EXECUTE IMMEDIATE 'BEGIN NULL; END;'; END; END;",
                &[],
            )
            .expect("create dynamic type fixture body");
        drop(setup);

        let failed = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(failed.status(), AnalysisStatus::Failed);
        assert_eq!(
            failed.failure().expect("failed outcome").code,
            AnalysisFailureCode::UnsupportedMetadata
        );
        assert!(failed.certified_snapshot().is_none());

        let setup = Connection::connect(&cleanup.username, password, &connect_string)
            .expect("reconnect as isolated Oracle test user");
        setup
            .execute("DROP TYPE DYNAMIC_TYPE_T FORCE", &[])
            .expect("drop dynamic type fixture");
        setup
            .execute(
                "CREATE OR REPLACE TRIGGER DYNAMIC_TRIGGER BEFORE UPDATE ON CHILD_ENTITY FOR EACH ROW BEGIN EXECUTE IMMEDIATE 'BEGIN NULL; END;'; END;",
                &[],
            )
            .expect("create fail-closed dynamic trigger fixture");
        drop(setup);

        let failed = analyze_oracle(&user_url, "oracle-live", Vec::new(), Vec::new(), 30_000);
        assert_eq!(failed.status(), AnalysisStatus::Failed);
        assert_eq!(
            failed.failure().expect("failed outcome").code,
            AnalysisFailureCode::UnsupportedMetadata
        );
        assert!(failed.certified_snapshot().is_none());
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_ORACLE_URL"]
    fn oracle_multi_schema_contract_is_env_gated() {
        let admin_url = env::var("DATABASE_MEMORY_TEST_ORACLE_URL")
            .expect("Oracle multi-schema test requires DATABASE_MEMORY_TEST_ORACLE_URL");
        let parsed = parse_oracle_connection_string(&admin_url).unwrap();
        let connect_string = parsed.connect_string.to_owned();
        let password = "DbmcpTest1!";
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let parent_name = format!("DBMCP_P{}_{}", std::process::id(), suffix);
        let child_name = format!("DBMCP_C{}_{}", std::process::id(), suffix);

        let parent_admin =
            Connection::connect(parsed.username, parsed.password, parsed.connect_string)
                .expect("connect to Oracle certification database for parent schema");
        parent_admin
            .execute(
                &format!(
                    "CREATE USER {parent_name} IDENTIFIED BY \"{password}\" DEFAULT TABLESPACE USERS QUOTA UNLIMITED ON USERS"
                ),
                &[],
            )
            .expect("create Oracle parent test user");
        let parent_cleanup = TestUserGuard {
            admin: parent_admin,
            username: parent_name,
        };

        let child_admin =
            Connection::connect(parsed.username, parsed.password, parsed.connect_string)
                .expect("connect to Oracle certification database for child schema");
        child_admin
            .execute(
                &format!(
                    "CREATE USER {child_name} IDENTIFIED BY \"{password}\" DEFAULT TABLESPACE USERS QUOTA UNLIMITED ON USERS"
                ),
                &[],
            )
            .expect("create Oracle child test user");
        let child_cleanup = TestUserGuard {
            admin: child_admin,
            username: child_name,
        };

        for cleanup in [&parent_cleanup, &child_cleanup] {
            cleanup
                .admin
                .execute(
                    &format!(
                        "GRANT CREATE SESSION, CREATE TABLE, CREATE VIEW, CREATE PROCEDURE, CREATE SYNONYM, CREATE TYPE TO {}",
                        cleanup.username
                    ),
                    &[],
                )
                .expect("grant Oracle multi-schema fixture privileges");
        }

        let parent = Connection::connect(&parent_cleanup.username, password, &connect_string)
            .expect("connect as Oracle parent test user");
        parent
            .execute(
                "CREATE TABLE SHARED_PARENT (ID NUMBER, CODE VARCHAR2(32), CONSTRAINT PK_SHARED_PARENT PRIMARY KEY (ID))",
                &[],
            )
            .expect("create shared parent table");
        parent
            .execute(
                "CREATE TYPE SHARED_PAYLOAD_T AS OBJECT (VALUE_TEXT VARCHAR2(64))",
                &[],
            )
            .expect("create shared object type");
        parent
            .execute(
                "CREATE FUNCTION SHARED_LABEL(P_ID IN NUMBER) RETURN VARCHAR2 AUTHID DEFINER AS V_CODE VARCHAR2(32); BEGIN SELECT CODE INTO V_CODE FROM SHARED_PARENT WHERE ID = P_ID; RETURN V_CODE; END;",
                &[],
            )
            .expect("create shared function");
        for statement in [
            format!(
                "GRANT SELECT, REFERENCES ON SHARED_PARENT TO {}",
                child_cleanup.username
            ),
            format!(
                "GRANT EXECUTE ON SHARED_PAYLOAD_T TO {}",
                child_cleanup.username
            ),
            format!(
                "GRANT EXECUTE ON SHARED_LABEL TO {}",
                child_cleanup.username
            ),
        ] {
            parent
                .execute(&statement, &[])
                .expect("grant cross-schema object privilege");
        }
        drop(parent);

        let child = Connection::connect(&child_cleanup.username, password, &connect_string)
            .expect("connect as Oracle child test user");
        child
            .execute(
                &format!(
                    "CREATE TABLE CHILD_RECORD (ID NUMBER, PARENT_ID NUMBER, PAYLOAD {}.SHARED_PAYLOAD_T, CONSTRAINT PK_CHILD_RECORD PRIMARY KEY (ID), CONSTRAINT FK_CHILD_SHARED FOREIGN KEY (PARENT_ID) REFERENCES {}.SHARED_PARENT (ID))",
                    parent_cleanup.username, parent_cleanup.username
                ),
                &[],
            )
            .expect("create cross-schema foreign key and typed column");
        child
            .execute(
                &format!(
                    "CREATE VIEW SHARED_PARENT_VIEW AS SELECT ID, CODE FROM {}.SHARED_PARENT",
                    parent_cleanup.username
                ),
                &[],
            )
            .expect("create cross-schema view");
        child
            .execute(
                &format!(
                    "CREATE SYNONYM SHARED_PARENT_ALIAS FOR {}.SHARED_PARENT",
                    parent_cleanup.username
                ),
                &[],
            )
            .expect("create cross-schema synonym");
        child
            .execute(
                &format!(
                    "CREATE PROCEDURE READ_SHARED(P_ID IN NUMBER, P_VALUE OUT VARCHAR2) AUTHID DEFINER AS BEGIN P_VALUE := {}.SHARED_LABEL(P_ID); END;",
                    parent_cleanup.username
                ),
                &[],
            )
            .expect("create cross-schema routine call");
        drop(child);

        let child_url = format!("{}/{password}@{connect_string}", child_cleanup.username);
        let denied = analyze_oracle(
            &child_url,
            "oracle-multi-denied",
            Vec::new(),
            vec![
                parent_cleanup.username.clone(),
                child_cleanup.username.clone(),
            ],
            30_000,
        );
        assert_eq!(denied.status(), AnalysisStatus::Failed);
        assert_eq!(
            denied.failure().expect("denied scope must fail").code,
            AnalysisFailureCode::PermissionDenied
        );
        assert!(denied.certified_snapshot().is_none());

        let incomplete = analyze_oracle(
            &admin_url,
            "oracle-multi-incomplete",
            Vec::new(),
            vec![child_cleanup.username.clone()],
            30_000,
        );
        assert_eq!(incomplete.status(), AnalysisStatus::Failed);
        assert!(incomplete.certified_snapshot().is_none());
        let incomplete_failure = incomplete.failure().expect("incomplete scope must fail");
        assert_eq!(
            incomplete_failure.code,
            AnalysisFailureCode::InvalidConfiguration,
            "unexpected incomplete-scope failure: {incomplete_failure:?}"
        );
        assert!(incomplete_failure.message.contains("relationship-closed"));
        assert!(incomplete_failure
            .message
            .contains(&parent_cleanup.username));

        let complete = analyze_oracle(
            &admin_url,
            "oracle-multi-live",
            Vec::new(),
            vec![
                parent_cleanup.username.clone(),
                child_cleanup.username.clone(),
            ],
            30_000,
        );
        assert_eq!(
            complete.status(),
            AnalysisStatus::Complete,
            "Oracle multi-schema analysis failed: {:?}",
            complete.failure()
        );
        let certified = complete
            .certified_snapshot()
            .expect("Oracle multi-schema snapshot must be certified");
        assert_eq!(certified.snapshot.schema.schemas.len(), 2);
        assert_eq!(certified.snapshot.schema.tables.len(), 2);

        let parent_table = certified
            .snapshot
            .schema
            .tables
            .iter()
            .find(|table| {
                table.key.schema == parent_cleanup.username && table.name == "SHARED_PARENT"
            })
            .expect("cross-schema parent table is mapped");
        let child_table = certified
            .snapshot
            .schema
            .tables
            .iter()
            .find(|table| {
                table.key.schema == child_cleanup.username && table.name == "CHILD_RECORD"
            })
            .expect("cross-schema child table is mapped");
        let foreign_key = certified
            .snapshot
            .schema
            .constraints
            .iter()
            .find(|constraint| {
                constraint.table_key == child_table.key && constraint.name == "FK_CHILD_SHARED"
            })
            .expect("cross-schema foreign key is mapped");
        assert_eq!(
            foreign_key.referenced_table_key.as_ref(),
            Some(&parent_table.key)
        );

        let view = certified
            .snapshot
            .schema
            .views
            .iter()
            .find(|view| {
                view.key.schema == child_cleanup.username && view.name == "SHARED_PARENT_VIEW"
            })
            .expect("cross-schema view is mapped");
        assert!(view.depends_on.contains(&parent_table.key));

        let shared_function = certified
            .snapshot
            .schema
            .routines
            .iter()
            .find(|routine| {
                routine.key.schema == parent_cleanup.username && routine.name == "SHARED_LABEL"
            })
            .expect("shared function is mapped");
        let child_procedure = certified
            .snapshot
            .schema
            .routines
            .iter()
            .find(|routine| {
                routine.key.schema == child_cleanup.username && routine.name == "READ_SHARED"
            })
            .expect("cross-schema procedure is mapped");
        assert!(child_procedure.depends_on.contains(&shared_function.key));

        let shared_type = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::UserDefinedType
                    && object.key.schema == parent_cleanup.username
                    && object.name == "SHARED_PAYLOAD_T"
            })
            .expect("shared object type is mapped");
        let payload_column = certified
            .snapshot
            .schema
            .columns
            .iter()
            .find(|column| column.table_key == child_table.key && column.name == "PAYLOAD")
            .expect("cross-schema typed column is mapped");
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::UsesType
                    && relationship.from_key == payload_column.key
                    && relationship.to_key == shared_type.key
            }));

        let synonym = certified
            .snapshot
            .metadata
            .objects
            .iter()
            .find(|object| {
                object.key.object_kind == ObjectKind::Synonym
                    && object.key.schema == child_cleanup.username
                    && object.name == "SHARED_PARENT_ALIAS"
            })
            .expect("cross-schema synonym is mapped");
        assert!(certified
            .snapshot
            .metadata
            .relationships
            .iter()
            .any(|relationship| {
                relationship.kind == MetadataRelationshipKind::SynonymFor
                    && relationship.from_key == synonym.key
                    && relationship.to_key == parent_table.key
            }));
    }

    fn request() -> IntrospectionRequest {
        IntrospectionRequest {
            connection_alias: "oracle-test".to_owned(),
            requested_catalogs: Vec::new(),
            requested_schemas: Vec::new(),
            timeout_ms: 30_000,
        }
    }

    struct TestUserGuard {
        admin: Connection,
        username: String,
    }

    impl Drop for TestUserGuard {
        fn drop(&mut self) {
            let _ = self
                .admin
                .execute(&format!("DROP USER {} CASCADE", self.username), &[]);
        }
    }
}
