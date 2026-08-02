fn set_object_count(
    counts: &mut DiscoveryCounts,
    category: ObjectCategory,
    count: u64,
    evidence: &str,
) {
    counts.objects.insert(
        category,
        DiscoveredCount {
            count,
            evidence: evidence.to_owned(),
        },
    );
}

fn set_relationship_count(
    counts: &mut DiscoveryCounts,
    category: RelationshipCategory,
    count: u64,
    evidence: &str,
) {
    counts.relationships.insert(
        category,
        DiscoveredCount {
            count,
            evidence: evidence.to_owned(),
        },
    );
}

fn require_bounded_sql(
    object_type: &str,
    object_name: &str,
    sql: &str,
) -> Result<(), SqliteAdapterError> {
    if sql.len() > MAX_SCHEMA_SQL_BYTES {
        return Err(SqliteAdapterError::mapping(
            format!("{object_type} {object_name}"),
            format!("schema definition exceeds {MAX_SCHEMA_SQL_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn fold_identifier(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn same_identifier(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn quote_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('\"', "\"\""))
}
