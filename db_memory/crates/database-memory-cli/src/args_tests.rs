#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_index_and_describe_commands() {
        assert_eq!(
            parse_args(
                ["contract", "--format", "json"]
                    .into_iter()
                    .map(str::to_owned)
            )
            .unwrap(),
            Command::Contract {
                format: OutputFormat::Json,
            }
        );

        assert_eq!(
            parse_args(
                [
                    "index",
                    "--source",
                    "sqlite",
                    "--path",
                    "sample.sqlite",
                    "--alias",
                    "sample"
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::Index {
                source: "sqlite".to_owned(),
                path: Some(PathBuf::from("sample.sqlite")),
                connection_string: None,
                alias: "sample".to_owned(),
                requested_catalogs: vec![],
                requested_schemas: vec![],
                timeout_ms: DEFAULT_TIMEOUT_MS,
                format: OutputFormat::Text,
                cache_path: PathBuf::from(".database-memory").join("graph.sqlite"),
            }
        );

        assert_eq!(
            parse_args(
                [
                    "index",
                    "--source",
                    "yugabytedb",
                    "--connection-string",
                    "postgresql://yugabyte@localhost:5433/yugabyte",
                    "--alias",
                    "yb-local",
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::Index {
                source: "yugabytedb".to_owned(),
                path: None,
                connection_string: Some("postgresql://yugabyte@localhost:5433/yugabyte".to_owned()),
                alias: "yb-local".to_owned(),
                requested_catalogs: vec![],
                requested_schemas: vec![],
                timeout_ms: DEFAULT_TIMEOUT_MS,
                format: OutputFormat::Text,
                cache_path: PathBuf::from(".database-memory").join("graph.sqlite"),
            }
        );

        assert_eq!(
            parse_args(
                [
                    "describe-table",
                    "sample",
                    "orders",
                    "--format",
                    "json",
                    "--cache-path",
                    "cache.sqlite"
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::DescribeTable {
                alias: "sample".to_owned(),
                object_key: None,
                table_name: Some("orders".to_owned()),
                format: OutputFormat::Json,
                cache_path: PathBuf::from("cache.sqlite"),
            }
        );

        assert_eq!(
            parse_args(
                [
                    "describe-table",
                    "sample",
                    "--object-key",
                    "sqlite:sample:main:audit:table:orders"
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::DescribeTable {
                alias: "sample".to_owned(),
                object_key: Some("sqlite:sample:main:audit:table:orders".to_owned()),
                table_name: None,
                format: OutputFormat::Text,
                cache_path: PathBuf::from(".database-memory").join("graph.sqlite"),
            }
        );

        assert!(parse_args(
            [
                "describe-table",
                "sample",
                "orders",
                "--object-key",
                "sqlite:sample:main:main:table:orders"
            ]
            .into_iter()
            .map(str::to_owned)
        )
        .unwrap_err()
        .contains("pass one table selector"));
    }

    #[test]
    fn parses_complete_scope_and_generic_object_commands() {
        assert_eq!(
            parse_args(
                [
                    "index",
                    "--source",
                    "sqlserver",
                    "--connection-string",
                    "Driver=SQL Server;Server=localhost",
                    "--alias",
                    "prod",
                    "--catalog",
                    "app",
                    "--schema",
                    "dbo",
                    "--timeout-ms",
                    "45000",
                ]
                .into_iter()
                .map(str::to_owned),
            )
            .unwrap(),
            Command::Index {
                source: "sqlserver".to_owned(),
                path: None,
                connection_string: Some("Driver=SQL Server;Server=localhost".to_owned()),
                alias: "prod".to_owned(),
                requested_catalogs: vec!["app".to_owned()],
                requested_schemas: vec!["dbo".to_owned()],
                timeout_ms: 45_000,
                format: OutputFormat::Text,
                cache_path: default_cache_path(),
            }
        );

        assert_eq!(
            parse_args(
                [
                    "find-objects",
                    "prod",
                    "account",
                    "--kind",
                    "table",
                    "--offset",
                    "10",
                    "--limit",
                    "25",
                    "--json",
                ]
                .into_iter()
                .map(str::to_owned),
            )
            .unwrap(),
            Command::FindObjects {
                selector: "prod".to_owned(),
                query: "account".to_owned(),
                kind: Some(ObjectKind::Table),
                offset: 10,
                limit: 25,
                format: OutputFormat::Json,
                cache_path: default_cache_path(),
            }
        );

        assert!(parse_args(
            ["list-objects", "prod", "--kind", "not-a-kind"]
                .into_iter()
                .map(str::to_owned)
        )
        .unwrap_err()
        .contains("unknown object kind"));
    }

    #[test]
    fn parses_bounded_json_traversal_commands() {
        assert_eq!(
            parse_args(
                [
                    "impact-analysis",
                    "ddl-sqlite:sample",
                    "--table",
                    "orders",
                    "--column",
                    "user_id",
                    "--direction",
                    "outbound",
                    "--max-depth",
                    "99",
                    "--limit",
                    "999",
                    "--format",
                    "json",
                    "--cache-path",
                    "cache.sqlite",
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::ImpactAnalysis {
                alias: "ddl-sqlite:sample".to_owned(),
                object_key: None,
                table_name: Some("orders".to_owned()),
                column_name: Some("user_id".to_owned()),
                direction: Direction::Outbound,
                max_depth: 99,
                limit: 999,
                cache_path: PathBuf::from("cache.sqlite"),
            }
        );

        assert_eq!(
            parse_args(
                [
                    "trace-relationships",
                    "sample",
                    "sqlite:sample:main:main:table:orders",
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::TraceRelationships {
                alias: "sample".to_owned(),
                object_key: "sqlite:sample:main:main:table:orders".to_owned(),
                direction: Direction::Both,
                max_depth: DEFAULT_TRAVERSAL_DEPTH,
                limit: DEFAULT_RESULT_LIMIT,
                cache_path: PathBuf::from(".database-memory").join("graph.sqlite"),
            }
        );

        assert!(parse_args(
            ["impact-analysis", "sample", "orders", "--format", "text"]
                .into_iter()
                .map(str::to_owned)
        )
        .unwrap_err()
        .contains("JSON output only"));
    }

    #[test]
    fn parses_json_inventory_and_rejects_invalid_limits() {
        assert_eq!(
            parse_args(
                [
                    "inventory",
                    "postgres:sample",
                    "--offset",
                    "1000",
                    "--limit",
                    "6000",
                    "--format",
                    "json",
                    "--cache-path",
                    "cache.sqlite",
                ]
                .into_iter()
                .map(str::to_owned)
            )
            .unwrap(),
            Command::Inventory {
                alias: "postgres:sample".to_owned(),
                offset: 1_000,
                limit: 6_000,
                cache_path: PathBuf::from("cache.sqlite"),
            }
        );

        assert_eq!(
            parse_args(["inventory", "sample"].into_iter().map(str::to_owned)).unwrap(),
            Command::Inventory {
                alias: "sample".to_owned(),
                offset: 0,
                limit: DEFAULT_INVENTORY_LIMIT,
                cache_path: PathBuf::from(".database-memory").join("graph.sqlite"),
            }
        );
        assert!(parse_args(
            ["inventory", "sample", "--limit", "0"]
                .into_iter()
                .map(str::to_owned)
        )
        .unwrap_err()
        .contains("at least 1"));
        assert!(parse_args(
            ["inventory", "sample", "--format", "text"]
                .into_iter()
                .map(str::to_owned)
        )
        .unwrap_err()
        .contains("JSON output only"));
    }
}
