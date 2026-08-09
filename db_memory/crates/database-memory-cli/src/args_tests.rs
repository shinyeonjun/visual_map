#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<Command, String> {
        parse_args(values.iter().map(|value| (*value).to_owned()))
    }

    #[test]
    fn parses_contract_and_index_commands() {
        assert_eq!(
            parse(&["contract", "--format", "json"]).unwrap(),
            Command::Contract {
                format: OutputFormat::Json,
            }
        );
        assert_eq!(
            parse(&[
                "index",
                "--source",
                "sqlite",
                "--path",
                "sample.sqlite",
                "--alias",
                "sample",
                "--catalog",
                "app",
                "--schema",
                "main",
                "--timeout-ms",
                "12000",
                "--format",
                "json",
                "--cache-path",
                "cache.sqlite",
            ])
            .unwrap(),
            Command::Index {
                source: "sqlite".to_owned(),
                path: Some(PathBuf::from("sample.sqlite")),
                connection_string: None,
                alias: "sample".to_owned(),
                requested_catalogs: vec!["app".to_owned()],
                requested_schemas: vec!["main".to_owned()],
                timeout_ms: 12_000,
                format: OutputFormat::Json,
                cache_path: PathBuf::from("cache.sqlite"),
            }
        );
    }

    #[test]
    fn parses_snapshot_and_object_read_commands() {
        assert_eq!(
            parse(&["list-snapshots", "--json", "--cache-path", "cache.sqlite"]).unwrap(),
            Command::ListSnapshots {
                format: OutputFormat::Json,
                cache_path: PathBuf::from("cache.sqlite"),
            }
        );
        assert_eq!(
            parse(&[
                "describe-snapshot",
                "sample",
                "--cache-path",
                "cache.sqlite",
            ])
            .unwrap(),
            Command::DescribeSnapshot {
                selector: "sample".to_owned(),
                format: OutputFormat::Text,
                cache_path: PathBuf::from("cache.sqlite"),
            }
        );
        assert_eq!(
            parse(&[
                "list-objects",
                "sample",
                "--kind",
                "table",
                "--offset",
                "10",
                "--limit",
                "25",
                "--json",
            ])
            .unwrap(),
            Command::ListObjects {
                selector: "sample".to_owned(),
                kind: Some(ObjectKind::Table),
                offset: 10,
                limit: 25,
                format: OutputFormat::Json,
                cache_path: default_cache_path(),
            }
        );
        assert_eq!(
            parse(&[
                "find-objects",
                "sample",
                "orders",
                "--kind",
                "column",
                "--limit",
                "5",
            ])
            .unwrap(),
            Command::FindObjects {
                selector: "sample".to_owned(),
                query: "orders".to_owned(),
                kind: Some(ObjectKind::Column),
                offset: 0,
                limit: 5,
                format: OutputFormat::Text,
                cache_path: default_cache_path(),
            }
        );
        assert_eq!(
            parse(&[
                "describe-object",
                "sample",
                "sqlite:sample:main:main:table:orders",
                "--relationship-limit",
                "50",
                "--format",
                "json",
            ])
            .unwrap(),
            Command::DescribeObject {
                selector: "sample".to_owned(),
                object_key: "sqlite:sample:main:main:table:orders".to_owned(),
                relationship_limit: 50,
                format: OutputFormat::Json,
                cache_path: default_cache_path(),
            }
        );
    }

    #[test]
    fn rejects_zero_read_limits() {
        assert_eq!(
            parse(&["list-objects", "sample", "--limit", "0"]).unwrap_err(),
            "object limit must be at least 1"
        );
        assert_eq!(
            parse(&[
                "describe-object",
                "sample",
                "object-key",
                "--relationship-limit",
                "0",
            ])
            .unwrap_err(),
            "relationship limit must be at least 1"
        );
    }

    #[test]
    fn removed_convenience_commands_fail_closed() {
        for command in [
            "describe-table",
            "inventory",
            "find-table",
            "find-column",
            "impact-analysis",
            "trace-relationships",
        ] {
            assert_eq!(
                parse(&[command]).unwrap_err(),
                format!("unknown command '{command}'")
            );
        }
    }
}
