fn ensure_partitioning_type(
    value: &str,
    allow_none: bool,
    subject: &str,
) -> Result<(), CatalogError> {
    if matches!(
        value,
        "RANGE" | "HASH" | "LIST" | "REFERENCE" | "SYSTEM" | "CONSISTENT HASH"
    ) || (allow_none && value == "NONE")
    {
        Ok(())
    } else {
        Err(CatalogError::UnsupportedMetadata(format!(
            "{subject} has unsupported partitioning type '{value}'"
        )))
    }
}

fn ensure_contiguous_positions(
    positions: impl Iterator<Item = i64>,
    subject: &str,
) -> Result<(), CatalogError> {
    let positions = positions.collect::<Vec<_>>();
    if positions
        .iter()
        .enumerate()
        .all(|(offset, position)| *position == (offset + 1) as i64)
    {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "{subject} do not have contiguous 1-based positions"
        )))
    }
}

fn ensure_owner(scope: &DictionaryScope, owner: &str, subject: &str) -> Result<(), CatalogError> {
    if scope.contains_owner(owner) {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "Oracle {subject} owner '{owner}' is outside the certified schema scope"
        )))
    }
}

fn ensure_reference_owner(
    scope: &DictionaryScope,
    owner: &str,
    source: &str,
) -> Result<(), CatalogError> {
    if scope.contains_owner(owner) {
        Ok(())
    } else {
        Err(CatalogError::InvalidScope(format!(
            "Oracle schema selection is not relationship-closed: {source} references application owner '{owner}'; include that owner and retry"
        )))
    }
}

struct OracleSnapshotMapper<'a> {
    connection_alias: &'a str,
    facts: ServerFacts,
    strategy: OracleCatalogVersion,
    scope: DictionaryScope,
}

impl<'a> OracleSnapshotMapper<'a> {
    fn new(
        connection_alias: &'a str,
        facts: ServerFacts,
        strategy: OracleCatalogVersion,
        scope: DictionaryScope,
    ) -> Self {
        Self {
            connection_alias,
            facts,
            strategy,
            scope,
        }
    }

}
include!("mapper/map.rs");

trait NamedCatalogColumn {
    fn name(&self) -> &str;
}
