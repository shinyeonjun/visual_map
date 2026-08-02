#[cfg(test)]
mod version_strategy_tests {
    use super::*;

    #[test]
    fn only_explicitly_certified_postgres_majors_select_a_strategy() {
        for major in MIN_SUPPORTED_MAJOR..=MAX_SUPPORTED_MAJOR {
            let strategy = PostgresCatalogVersion::detect(major * 10_000).unwrap();
            assert_eq!(strategy.major(), major);
        }
        assert!(matches!(
            PostgresCatalogVersion::detect(130_000),
            Err(CatalogError::UnsupportedVersion(13))
        ));
        assert!(matches!(
            PostgresCatalogVersion::detect(190_000),
            Err(CatalogError::UnsupportedVersion(19))
        ));
    }

    #[test]
    fn product_strategies_reject_cross_product_and_uncertified_releases() {
        let postgres = server_facts(
            "16.10 (Debian 16.10-1.pgdg13+1)",
            "PostgreSQL 16.10 (Debian 16.10-1.pgdg13+1) on x86_64-pc-linux-gnu",
            160_010,
        );
        assert_eq!(
            PgCatalogStrategy::detect(PgWireProduct::PostgreSql, &postgres).unwrap(),
            PgCatalogStrategy::PostgreSql(PostgresCatalogVersion::V16)
        );

        let yugabyte = server_facts(
            CERTIFIED_YUGABYTEDB_VERSION,
            "PostgreSQL 15.12-YB-2025.2.3.2-b0 on x86_64-pc-linux-gnu",
            150_012,
        );
        assert!(matches!(
            PgCatalogStrategy::detect(PgWireProduct::PostgreSql, &yugabyte),
            Err(CatalogError::UnsupportedProduct(_))
        ));
        assert_eq!(
            PgCatalogStrategy::detect(PgWireProduct::YugabyteDb, &yugabyte).unwrap(),
            PgCatalogStrategy::YugabyteDb2025_2_3_2
        );
        assert!(matches!(
            PgCatalogStrategy::detect(PgWireProduct::YugabyteDb, &postgres),
            Err(CatalogError::UnsupportedProduct(_))
        ));

        let unsupported_yugabyte = server_facts(
            "15.12-YB-2025.2.4.0-b0",
            "PostgreSQL 15.12-YB-2025.2.4.0-b0 on x86_64-pc-linux-gnu",
            150_012,
        );
        assert!(matches!(
            PgCatalogStrategy::detect(PgWireProduct::YugabyteDb, &unsupported_yugabyte),
            Err(CatalogError::UnsupportedRelease(_))
        ));

        let cockroach = server_facts(
            "15.0",
            "CockroachDB CCL v25.2.1 (x86_64-unknown-linux-gnu)",
            150_000,
        );
        assert!(matches!(
            PgCatalogStrategy::detect(PgWireProduct::PostgreSql, &cockroach),
            Err(CatalogError::UnsupportedProduct(_))
        ));
    }

    fn server_facts(version: &str, version_banner: &str, version_num: i32) -> ServerFacts {
        ServerFacts {
            database: "app".to_owned(),
            version: version.to_owned(),
            version_banner: version_banner.to_owned(),
            version_num,
            current_user: "reader".to_owned(),
            session_user: "reader".to_owned(),
            transaction_read_only: true,
            transaction_isolation: "repeatable read".to_owned(),
            tls: false,
            tls_version: None,
            tls_cipher: None,
        }
    }

    #[test]
    fn statistics_target_representation_is_normalized_by_version() {
        assert_eq!(
            PostgresCatalogVersion::V16
                .statistics_target(Some(-1))
                .unwrap(),
            PostgresStatisticsTarget::Default
        );
        assert_eq!(
            PostgresCatalogVersion::V17.statistics_target(None).unwrap(),
            PostgresStatisticsTarget::Default
        );
        assert_eq!(
            PostgresCatalogVersion::V18
                .statistics_target(Some(0))
                .unwrap(),
            PostgresStatisticsTarget::Disabled
        );
        assert_eq!(
            PostgresCatalogVersion::V14
                .statistics_target(Some(200))
                .unwrap(),
            PostgresStatisticsTarget::Custom(200)
        );
        assert!(PostgresCatalogVersion::V16.statistics_target(None).is_err());
        assert!(PostgresCatalogVersion::V17
            .statistics_target(Some(-1))
            .is_err());
    }
}
