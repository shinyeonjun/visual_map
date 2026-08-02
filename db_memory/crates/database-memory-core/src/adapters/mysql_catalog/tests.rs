#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use mysql::prelude::Queryable;
    use mysql::Conn;

    use super::*;
    use crate::analysis_outcome::{AnalysisFailureCode, AnalysisStatus};

    #[test]
    fn version_strategy_accepts_only_the_certified_matrix() {
        for (version, expected) in [
            ("8.0.46", MysqlFamilyVersion::Mysql80),
            ("8.4.10", MysqlFamilyVersion::Mysql84),
            ("9.7.1", MysqlFamilyVersion::Mysql97),
            ("10.11.18-MariaDB-ubu2204", MysqlFamilyVersion::MariaDb1011),
            ("11.4.12-MariaDB", MysqlFamilyVersion::MariaDb114),
            ("11.8.8-MariaDB", MysqlFamilyVersion::MariaDb118),
            ("12.3.2-MariaDB", MysqlFamilyVersion::MariaDb123),
        ] {
            assert_eq!(MysqlFamilyVersion::detect(version).unwrap(), expected);
        }
        assert!(MysqlFamilyVersion::detect("5.7.44").is_err());
        assert!(MysqlFamilyVersion::detect("10.6.23-MariaDB").is_err());
        assert!(MysqlFamilyVersion::detect("9.8.0").is_err());
    }

    #[test]
    fn loopback_policy_is_exact() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("::1"));
        assert!(!is_loopback_host("db.example.com"));
    }

    #[test]
    fn changed_catalog_signature_is_never_certified() {
        let before = CatalogSignature(vec!["table:a".to_owned()]);
        let after = CatalogSignature(vec!["table:b".to_owned()]);

        assert!(matches!(
            require_stable_signature(&before, &after),
            Err(CatalogError::ConcurrentDdl(_))
        ));
        assert!(require_stable_signature(&before, &before).is_ok());
    }

    #[test]
    fn server_generated_grants_are_scoped_without_substring_guessing() {
        let privileges = parse_schema_grant(
            "GRANT SELECT, EXECUTE, SHOW VIEW, EVENT, TRIGGER ON `app``data`.* TO `reader`@`%`",
            "app`data",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            privileges,
            BTreeSet::from([
                "SELECT".to_owned(),
                "EXECUTE".to_owned(),
                "SHOW VIEW".to_owned(),
                "EVENT".to_owned(),
                "TRIGGER".to_owned(),
            ])
        );
        assert!(parse_schema_grant(
            "GRANT SELECT ON `app``data`.`one_table` TO `reader`@`%`",
            "app`data"
        )
        .unwrap()
        .is_none());
        assert!(
            parse_schema_grant("GRANT `metadata_role` TO `reader`@`%`", "app`data")
                .unwrap()
                .is_none()
        );
        assert!(parse_schema_grant(
            "SET DEFAULT ROLE `metadata_role` FOR `reader`@`%`",
            "app`data"
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn transport_options_require_tls_for_remote_hosts_and_disable_cleartext_auth() {
        let request = IntrospectionRequest {
            connection_alias: "policy".to_owned(),
            requested_catalogs: Vec::new(),
            requested_schemas: Vec::new(),
            timeout_ms: 1_000,
        };
        let remote = secure_connection_options(
            &request,
            "mysql://reader:secret@db.example.com/app?prefer_socket=false",
        )
        .unwrap();
        assert!(remote.get_ssl_opts().is_some());
        assert!(!remote.get_enable_cleartext_plugin());

        let local = secure_connection_options(
            &request,
            "mysql://reader:secret@127.0.0.1/app?prefer_socket=false",
        )
        .unwrap();
        assert!(local.get_ssl_opts().is_none());

        let unsafe_auth = secure_connection_options(
            &request,
            "mysql://reader:secret@127.0.0.1/app?enable_cleartext_plugin=true",
        )
        .unwrap_err();
        assert_eq!(unsafe_auth.code, AnalysisFailureCode::UnsafeSource);
        assert!(!unsafe_auth.message.contains("secret"));
    }

    #[test]
    fn mariadb_view_ast_extracts_nested_relations_without_cte_aliases() {
        let relations = parse_mariadb_view_relations(
            "WITH recent AS (SELECT id FROM `dbmcp`.`orders`) \
             SELECT r.id FROM recent r JOIN customers c ON c.id = r.id \
             WHERE EXISTS (SELECT 1 FROM audit_log a WHERE a.id = r.id)",
            0,
        )
        .unwrap();

        assert_eq!(
            relations,
            BTreeSet::from([
                (Some("dbmcp".to_owned()), "orders".to_owned()),
                (None, "customers".to_owned()),
                (None, "audit_log".to_owned()),
            ])
        );
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_MYSQL*_URL or DATABASE_MEMORY_TEST_MARIADB*_URL"]
    fn mysql_family_live_matrix_is_env_gated() {
        let _live_test_guard = live_test_guard();
        let configured = required_live_cases();
        for (environment, source_kind, url) in configured {
            let outcome = analyze_mysql_family(&url, environment, Vec::new(), 30_000);
            assert_eq!(
                outcome.status(),
                AnalysisStatus::Complete,
                "{environment}: {:?}",
                outcome.failure()
            );
            let snapshot = outcome.certified_snapshot().unwrap();
            assert_eq!(snapshot.snapshot.schema.source_kind, source_kind);
            assert!(snapshot.snapshot.schema.capabilities.metadata_only);
        }
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_MYSQL*_URL or DATABASE_MEMORY_TEST_MARIADB*_URL"]
    fn rich_mysql_family_catalog_is_certified_across_the_live_matrix() {
        let _live_test_guard = live_test_guard();
        let configured = required_live_cases();
        for (environment, source_kind, url) in configured {
            let names = RichFixtureNames::new();
            let mut connection = Conn::new(url.as_str()).unwrap();
            create_rich_fixture(&mut connection, &names, source_kind == "mariadb");

            let outcome = analyze_mysql_family(&url, environment, Vec::new(), 30_000);
            let failure = outcome.failure().cloned();
            let certified = outcome.certified_snapshot().cloned();
            drop_rich_fixture(&mut connection, &names, source_kind == "mariadb");

            assert_eq!(
                outcome.status(),
                AnalysisStatus::Complete,
                "{environment}: {failure:?}"
            );
            let snapshot = &certified.unwrap().snapshot;
            for table in [&names.users, &names.orders, &names.events] {
                assert!(
                    snapshot
                        .schema
                        .tables
                        .iter()
                        .any(|item| &item.name == table),
                    "{environment}: missing table {table}"
                );
            }
            assert!(snapshot.schema.columns.iter().any(|column| {
                column.table_key.object_name == names.users
                    && column.name == "slug"
                    && column.is_generated
            }));
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
                    "{environment}: missing {kind:?}"
                );
            }
            assert!(snapshot
                .schema
                .indexes
                .iter()
                .any(|index| index.name == names.email_index));
            assert!(snapshot
                .schema
                .views
                .iter()
                .find(|view| view.name == names.order_view)
                .is_some_and(|view| view.depends_on.len() >= 2));
            assert!(snapshot.metadata.objects.iter().any(|object| {
                object.extension_kind.as_deref() == Some("mysql_partition")
                    && object.key.object_name == names.events
            }));
            if source_kind == "mariadb" {
                assert!(snapshot.metadata.objects.iter().any(|object| {
                    object.key.object_kind == ObjectKind::Sequence && object.name == names.sequence
                }));
            }
        }
    }

    #[test]
    #[ignore = "requires a DATABASE_MEMORY_TEST_MYSQL*_URL or DATABASE_MEMORY_TEST_MARIADB*_URL"]
    fn procedural_mysql_family_metadata_remains_structural() {
        let _live_test_guard = live_test_guard();
        let configured = required_live_cases();
        let mut exercised = BTreeSet::new();
        for (environment, source_kind, url) in configured {
            if !exercised.insert(source_kind) {
                continue;
            }
            let suffix = unique_suffix();
            let routine = format!("dm_routine_{suffix}");
            let mut connection = Conn::new(url.as_str()).unwrap();
            connection
                .query_drop(format!(
                    "CREATE PROCEDURE {}(IN value_in INT) SELECT value_in",
                    quote_identifier(&routine)
                ))
                .unwrap();

            let outcome = analyze_mysql_family(&url, environment, Vec::new(), 30_000);
            connection
                .query_drop(format!("DROP PROCEDURE {}", quote_identifier(&routine)))
                .unwrap();

            assert_eq!(
                outcome.status(),
                AnalysisStatus::Complete,
                "{:?}",
                outcome.failure()
            );
            let snapshot = &outcome.certified_snapshot().unwrap().snapshot;
            assert!(snapshot
                .schema
                .routines
                .iter()
                .any(|item| item.name == routine));
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_MYSQL_ADMIN_URL or DATABASE_MEMORY_TEST_MARIADB_ADMIN_URL"]
    fn trigger_and_event_bodies_remain_structural_for_both_products() {
        let _live_test_guard = live_test_guard();
        let configured = required_admin_cases();
        for (environment, _source_kind, url) in configured {
            let suffix = unique_suffix();
            let table = format!("dm_trigger_table_{suffix}");
            let trigger = format!("dm_trigger_{suffix}");
            let event = format!("dm_event_{suffix}");
            let mut connection = Conn::new(url.as_str()).unwrap();
            connection
                .query_drop(format!(
                    "CREATE TABLE {} (id INT NOT NULL PRIMARY KEY)",
                    quote_identifier(&table)
                ))
                .unwrap();
            connection
                .query_drop(format!(
                    "CREATE TRIGGER {} BEFORE INSERT ON {} FOR EACH ROW SET NEW.id = COALESCE(NEW.id, 0)",
                    quote_identifier(&trigger),
                    quote_identifier(&table)
                ))
                .unwrap();

            let trigger_outcome = analyze_mysql_family(&url, environment, Vec::new(), 30_000);
            connection
                .query_drop(format!("DROP TRIGGER {}", quote_identifier(&trigger)))
                .unwrap();
            connection
                .query_drop(format!("DROP TABLE {}", quote_identifier(&table)))
                .unwrap();

            assert_eq!(
                trigger_outcome.status(),
                AnalysisStatus::Complete,
                "{:?}",
                trigger_outcome.failure()
            );
            let trigger_snapshot = trigger_outcome.certified_snapshot().unwrap();
            assert!(trigger_snapshot
                .snapshot
                .schema
                .triggers
                .iter()
                .any(|item| item.name == trigger));

            connection
                .query_drop(format!(
                    "CREATE EVENT {} ON SCHEDULE EVERY 1 DAY DO SELECT 1",
                    quote_identifier(&event)
                ))
                .unwrap();
            let event_outcome = analyze_mysql_family(&url, environment, Vec::new(), 30_000);
            connection
                .query_drop(format!("DROP EVENT {}", quote_identifier(&event)))
                .unwrap();

            assert_eq!(
                event_outcome.status(),
                AnalysisStatus::Complete,
                "{:?}",
                event_outcome.failure()
            );
            let event_snapshot = event_outcome.certified_snapshot().unwrap();
            assert!(event_snapshot
                .snapshot
                .metadata
                .objects
                .iter()
                .any(|item| item.name == event));
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_MYSQL_ADMIN_URL or DATABASE_MEMORY_TEST_MARIADB_ADMIN_URL"]
    fn schema_wide_visibility_is_required_and_then_sufficient() {
        let _live_test_guard = live_test_guard();
        let configured = required_admin_cases();
        for (environment, source_kind, admin_url) in configured {
            let opts = Opts::from_url(&admin_url).unwrap();
            let database = opts.get_db_name().unwrap().to_owned();
            let host = opts.get_ip_or_hostname();
            let port = opts.get_tcp_port();
            let suffix = unique_suffix();
            let user = format!("dm_reader_{suffix}");
            let password = format!("DmRead{suffix}");
            let table = format!("dm_visible_{suffix}");
            let reader_url =
                format!("mysql://{user}:{password}@{host}:{port}/{database}?prefer_socket=false");
            let account = format!("'{}'@'%'", user.replace('\'', "''"));
            let mut admin = Conn::new(admin_url.as_str()).unwrap();
            admin
                .query_drop(format!("DROP USER IF EXISTS {account}"))
                .unwrap();
            admin
                .query_drop(format!(
                    "CREATE USER {account} IDENTIFIED BY '{}'",
                    password.replace('\'', "''")
                ))
                .unwrap();
            admin
                .query_drop(format!(
                    "CREATE TABLE {} (id INT NOT NULL PRIMARY KEY)",
                    quote_identifier(&table)
                ))
                .unwrap();
            admin
                .query_drop(format!(
                    "GRANT SELECT ON {}.{} TO {account}",
                    quote_identifier(&database),
                    quote_identifier(&table)
                ))
                .unwrap();

            let denied = analyze_mysql_family(&reader_url, environment, Vec::new(), 30_000);
            admin
                .query_drop(format!(
                    "GRANT SELECT, SHOW VIEW, EXECUTE, EVENT, TRIGGER ON {}.* TO {account}",
                    quote_identifier(&database)
                ))
                .unwrap();
            let allowed = analyze_mysql_family(&reader_url, environment, Vec::new(), 30_000);

            admin
                .query_drop(format!("DROP TABLE {}", quote_identifier(&table)))
                .unwrap();
            admin.query_drop(format!("DROP USER {account}")).unwrap();

            assert_eq!(denied.status(), AnalysisStatus::Failed);
            assert_eq!(
                denied.failure().map(|failure| failure.code),
                Some(AnalysisFailureCode::PermissionDenied)
            );
            assert_eq!(
                allowed.status(),
                AnalysisStatus::Complete,
                "{source_kind}: {:?}",
                allowed.failure()
            );
        }
    }

    #[test]
    #[ignore = "requires DATABASE_MEMORY_TEST_MYSQL_ADMIN_URL or DATABASE_MEMORY_TEST_MARIADB_ADMIN_URL"]
    fn active_role_privileges_are_part_of_the_visibility_proof() {
        let _live_test_guard = live_test_guard();
        let configured = required_admin_cases();
        for (environment, source_kind, admin_url) in configured {
            let opts = Opts::from_url(&admin_url).unwrap();
            let database = opts.get_db_name().unwrap().to_owned();
            let host = opts.get_ip_or_hostname();
            let port = opts.get_tcp_port();
            let suffix = unique_suffix();
            let user = format!("dm_role_user_{suffix}");
            let role = format!("dm_role_{suffix}");
            let password = format!("DmRole{suffix}");
            let table = format!("dm_role_table_{suffix}");
            let reader_url =
                format!("mysql://{user}:{password}@{host}:{port}/{database}?prefer_socket=false");
            let user_account = format!("'{user}'@'%'");
            let role_account = if source_kind == "mysql" {
                format!("'{role}'@'%'")
            } else {
                quote_identifier(&role)
            };
            let mut admin = Conn::new(admin_url.as_str()).unwrap();
            admin
                .query_drop(format!("DROP USER IF EXISTS {user_account}"))
                .unwrap();
            if source_kind == "mysql" {
                admin
                    .query_drop(format!("DROP ROLE IF EXISTS {role_account}"))
                    .unwrap();
            } else {
                admin
                    .query_drop(format!("DROP ROLE IF EXISTS {}", quote_identifier(&role)))
                    .unwrap();
            }
            admin
                .query_drop(format!(
                    "CREATE USER {user_account} IDENTIFIED BY '{password}'"
                ))
                .unwrap();
            admin
                .query_drop(format!("CREATE ROLE {role_account}"))
                .unwrap();
            admin
                .query_drop(format!(
                    "CREATE TABLE {} (id INT NOT NULL PRIMARY KEY)",
                    quote_identifier(&table)
                ))
                .unwrap();
            admin
                .query_drop(format!(
                    "GRANT SELECT, SHOW VIEW, EXECUTE, EVENT, TRIGGER ON {}.* TO {role_account}",
                    quote_identifier(&database)
                ))
                .unwrap();
            admin
                .query_drop(format!("GRANT {role_account} TO {user_account}"))
                .unwrap();
            if source_kind == "mysql" {
                admin
                    .query_drop(format!("SET DEFAULT ROLE {role_account} TO {user_account}"))
                    .unwrap();
            } else {
                admin
                    .query_drop(format!(
                        "SET DEFAULT ROLE {} FOR {user_account}",
                        quote_identifier(&role)
                    ))
                    .unwrap();
            }

            let outcome = analyze_mysql_family(&reader_url, environment, Vec::new(), 30_000);

            admin
                .query_drop(format!("DROP TABLE {}", quote_identifier(&table)))
                .unwrap();
            admin
                .query_drop(format!("DROP USER {user_account}"))
                .unwrap();
            admin
                .query_drop(format!("DROP ROLE {role_account}"))
                .unwrap();

            assert_eq!(
                outcome.status(),
                AnalysisStatus::Complete,
                "{source_kind}: {:?}",
                outcome.failure()
            );
            assert!(outcome
                .certified_snapshot()
                .unwrap()
                .snapshot
                .metadata
                .objects
                .iter()
                .any(|object| {
                    object.key.object_kind == ObjectKind::Principal
                        && object.properties.get("principal_kind")
                            == Some(&MetadataValue::String("active_role".to_owned()))
                }));
        }
    }

    fn required_live_cases() -> Vec<(&'static str, &'static str, String)> {
        let configured = [
            ("DATABASE_MEMORY_TEST_MYSQL80_URL", "mysql"),
            ("DATABASE_MEMORY_TEST_MYSQL84_URL", "mysql"),
            ("DATABASE_MEMORY_TEST_MYSQL97_URL", "mysql"),
            ("DATABASE_MEMORY_TEST_MARIADB1011_URL", "mariadb"),
            ("DATABASE_MEMORY_TEST_MARIADB114_URL", "mariadb"),
            ("DATABASE_MEMORY_TEST_MARIADB118_URL", "mariadb"),
            ("DATABASE_MEMORY_TEST_MARIADB123_URL", "mariadb"),
        ]
        .into_iter()
        .filter_map(|(environment, source_kind)| {
            std::env::var(environment)
                .ok()
                .map(|url| (environment, source_kind, url))
        })
        .collect::<Vec<_>>();
        assert!(
            !configured.is_empty(),
            "live MySQL-family test requires at least one DATABASE_MEMORY_TEST_MYSQL*_URL or DATABASE_MEMORY_TEST_MARIADB*_URL"
        );
        configured
    }

    fn required_admin_cases() -> Vec<(&'static str, &'static str, String)> {
        let configured = [
            ("DATABASE_MEMORY_TEST_MYSQL_ADMIN_URL", "mysql"),
            ("DATABASE_MEMORY_TEST_MARIADB_ADMIN_URL", "mariadb"),
        ]
        .into_iter()
        .filter_map(|(environment, source_kind)| {
            std::env::var(environment)
                .ok()
                .map(|url| (environment, source_kind, url))
        })
        .collect::<Vec<_>>();
        assert!(
            !configured.is_empty(),
            "live MySQL-family privilege test requires DATABASE_MEMORY_TEST_MYSQL_ADMIN_URL or DATABASE_MEMORY_TEST_MARIADB_ADMIN_URL"
        );
        configured
    }

    fn live_test_guard() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct RichFixtureNames {
        users: String,
        orders: String,
        events: String,
        active_view: String,
        order_view: String,
        email_index: String,
        user_unique: String,
        user_check: String,
        order_fk: String,
        order_check: String,
        sequence: String,
    }

    impl RichFixtureNames {
        fn new() -> Self {
            let suffix = unique_suffix();
            Self {
                users: format!("dm_users_{suffix}"),
                orders: format!("dm_orders_{suffix}"),
                events: format!("dm_events_{suffix}"),
                active_view: format!("dm_active_{suffix}"),
                order_view: format!("dm_order_view_{suffix}"),
                email_index: format!("dm_email_idx_{suffix}"),
                user_unique: format!("dm_user_uq_{suffix}"),
                user_check: format!("dm_user_ck_{suffix}"),
                order_fk: format!("dm_order_fk_{suffix}"),
                order_check: format!("dm_order_ck_{suffix}"),
                sequence: format!("dm_sequence_{suffix}"),
            }
        }
    }

    fn unique_suffix() -> String {
        format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                % 1_000_000_000
        )
    }

    fn create_rich_fixture(connection: &mut Conn, names: &RichFixtureNames, maria_db: bool) {
        connection
            .query_drop(format!(
                "CREATE TABLE {} (\
                    id BIGINT NOT NULL AUTO_INCREMENT,\
                    email VARCHAR(255) NOT NULL,\
                    status VARCHAR(20) NOT NULL DEFAULT 'active',\
                    slug VARCHAR(255) GENERATED ALWAYS AS (LOWER(email)) STORED,\
                    PRIMARY KEY (id),\
                    CONSTRAINT {} UNIQUE (email),\
                    CONSTRAINT {} CHECK (CHAR_LENGTH(email) > 3)\
                ) ENGINE=InnoDB COMMENT='database-memory rich fixture'",
                quote_identifier(&names.users),
                quote_identifier(&names.user_unique),
                quote_identifier(&names.user_check),
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "CREATE TABLE {} (\
                    id BIGINT NOT NULL PRIMARY KEY,\
                    user_id BIGINT NOT NULL,\
                    amount DECIMAL(12,2) NOT NULL DEFAULT 0,\
                    CONSTRAINT {} FOREIGN KEY (user_id) REFERENCES {}(id) ON DELETE CASCADE,\
                    CONSTRAINT {} CHECK (amount >= 0)\
                ) ENGINE=InnoDB",
                quote_identifier(&names.orders),
                quote_identifier(&names.order_fk),
                quote_identifier(&names.users),
                quote_identifier(&names.order_check),
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "CREATE TABLE {} (\
                    id BIGINT NOT NULL,\
                    created_at DATE NOT NULL,\
                    PRIMARY KEY (id, created_at)\
                ) ENGINE=InnoDB PARTITION BY RANGE (YEAR(created_at)) (\
                    PARTITION p2025 VALUES LESS THAN (2026),\
                    PARTITION pmax VALUES LESS THAN MAXVALUE\
                )",
                quote_identifier(&names.events),
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "CREATE INDEX {} ON {} (email(24))",
                quote_identifier(&names.email_index),
                quote_identifier(&names.users),
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "CREATE VIEW {} AS SELECT id, email, status FROM {} WHERE status = 'active'",
                quote_identifier(&names.active_view),
                quote_identifier(&names.users),
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "CREATE VIEW {} AS SELECT o.id, u.email FROM {} o JOIN {} u ON u.id = o.user_id",
                quote_identifier(&names.order_view),
                quote_identifier(&names.orders),
                quote_identifier(&names.active_view),
            ))
            .unwrap();
        if maria_db {
            connection
                .query_drop(format!(
                    "CREATE SEQUENCE {} START WITH 10 INCREMENT BY 2 MINVALUE 10 MAXVALUE 1000 CYCLE",
                    quote_identifier(&names.sequence)
                ))
                .unwrap();
        }
    }

    fn drop_rich_fixture(connection: &mut Conn, names: &RichFixtureNames, maria_db: bool) {
        connection
            .query_drop(format!(
                "DROP VIEW IF EXISTS {}",
                quote_identifier(&names.order_view)
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "DROP VIEW IF EXISTS {}",
                quote_identifier(&names.active_view)
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "DROP TABLE IF EXISTS {}",
                quote_identifier(&names.orders)
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "DROP TABLE IF EXISTS {}",
                quote_identifier(&names.events)
            ))
            .unwrap();
        connection
            .query_drop(format!(
                "DROP TABLE IF EXISTS {}",
                quote_identifier(&names.users)
            ))
            .unwrap();
        if maria_db {
            connection
                .query_drop(format!(
                    "DROP SEQUENCE IF EXISTS {}",
                    quote_identifier(&names.sequence)
                ))
                .unwrap();
        }
    }
}
