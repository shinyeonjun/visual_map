async fn rows(client: &mut TdsClient, sql: &str) -> Result<Vec<Row>, CatalogError> {
    Ok(client.simple_query(sql).await?.into_first_result().await?)
}

async fn query_one(client: &mut TdsClient, sql: &str) -> Result<Row, CatalogError> {
    client
        .simple_query(sql)
        .await?
        .into_row()
        .await?
        .ok_or_else(|| CatalogError::Mapping("required catalog query returned no row".to_owned()))
}

fn required_value<'a, T>(row: &'a Row, index: usize, field: &str) -> Result<T, CatalogError>
where
    T: FromSql<'a>,
{
    row.try_get(index)
        .map_err(|error| CatalogError::Mapping(format!("cannot read {field}: {error}")))?
        .ok_or_else(|| CatalogError::Mapping(format!("required {field} is NULL")))
}

fn optional_value<'a, T>(row: &'a Row, index: usize) -> Result<Option<T>, CatalogError>
where
    T: FromSql<'a>,
{
    row.try_get(index).map_err(|error| {
        CatalogError::Mapping(format!(
            "cannot read optional catalog field at column {index}: {error}"
        ))
    })
}

fn required_string(row: &Row, index: usize, field: &str) -> Result<String, CatalogError> {
    let value = required_value::<&str>(row, index, field)?.to_owned();
    if value.is_empty() {
        return Err(CatalogError::Mapping(format!("required {field} is empty")));
    }
    if value.len() > MAX_PROPERTY_STRING_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "{field} exceeds the {MAX_PROPERTY_STRING_BYTES}-byte property limit"
        )));
    }
    Ok(value)
}

fn optional_string(row: &Row, index: usize) -> Result<Option<String>, CatalogError> {
    let value = optional_value::<&str>(row, index)?.map(str::to_owned);
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_PROPERTY_STRING_BYTES)
    {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "catalog property exceeds the {MAX_PROPERTY_STRING_BYTES}-byte limit"
        )));
    }
    Ok(value)
}

struct SqlServerSnapshotMapper {
    connection_alias: String,
    facts: ServerFacts,
    strategy: SqlServerCatalogVersion,
}


