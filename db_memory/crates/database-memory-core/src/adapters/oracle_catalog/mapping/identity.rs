impl NamedCatalogColumn for RawConstraintColumn {
    fn name(&self) -> &str {
        &self.name
    }
}

impl NamedCatalogColumn for RawIndexColumn {
    fn name(&self) -> &str {
        &self.name
    }
}

fn resolve_named_columns<T: NamedCatalogColumn>(
    owner: &str,
    table: &str,
    raw_columns: &[T],
    column_keys: &BTreeMap<(String, String, String), ObjectKey>,
    subject: &str,
) -> Result<Vec<ObjectKey>, CatalogError> {
    raw_columns
        .iter()
        .map(|column| {
            required(
                column_keys.get(&(owner.to_owned(), table.to_owned(), column.name().to_owned())),
                format!(
                    "Oracle column {}.{}.{} for {}",
                    owner,
                    table,
                    column.name(),
                    subject
                ),
            )
            .cloned()
        })
        .collect()
}
