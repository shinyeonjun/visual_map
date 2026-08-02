fn required<T>(row: &Row, column: &str) -> Result<T, CatalogError>
where
    T: mysql::prelude::FromValue,
{
    row.get_opt(column)
        .ok_or_else(|| CatalogError::Mapping(format!("catalog column '{column}' is missing")))?
        .map_err(|error| {
            CatalogError::Mapping(format!(
                "catalog column '{column}' has an incompatible value: {error}"
            ))
        })
}

fn optional<T>(row: &Row, column: &str) -> Result<Option<T>, CatalogError>
where
    T: mysql::prelude::FromValue,
{
    match row.get_opt::<Option<T>, _>(column) {
        Some(result) => result.map_err(|error| {
            CatalogError::Mapping(format!(
                "catalog column '{column}' has an incompatible optional value: {error}"
            ))
        }),
        None => Err(CatalogError::Mapping(format!(
            "catalog column '{column}' is missing"
        ))),
    }
}

fn optional_at<T>(row: &Row, index: usize) -> Result<Option<T>, CatalogError>
where
    T: mysql::prelude::FromValue,
{
    match row.get_opt::<Option<T>, _>(index) {
        Some(result) => result.map_err(|error| {
            CatalogError::Mapping(format!(
                "catalog column at index {index} has an incompatible optional value: {error}"
            ))
        }),
        None => Err(CatalogError::Mapping(format!(
            "catalog column at index {index} is missing"
        ))),
    }
}

fn required_at<T>(row: &Row, index: usize) -> Result<T, CatalogError>
where
    T: mysql::prelude::FromValue,
{
    row.get_opt(index)
        .ok_or_else(|| {
            CatalogError::Mapping(format!("catalog column at index {index} is missing"))
        })?
        .map_err(|error| {
            CatalogError::Mapping(format!(
                "catalog column at index {index} has an incompatible value: {error}"
            ))
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogSignature(Vec<String>);

impl CatalogSignature {
    fn read<Q: Queryable>(
        connection: &mut Q,
        database: &str,
        strategy: MysqlFamilyVersion,
    ) -> Result<Self, CatalogError> {
        let mut values = Vec::new();
        for query in strategy.signature_queries() {
            let rows = connection.exec::<Row, _, _>(query, (database,))?;
            for row in rows {
                values.push(required(&row, "signature")?);
            }
        }
        if strategy.product() == MysqlProduct::MariaDb {
            let sequence_rows = connection.exec::<Row, _, _>(
                "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'SEQUENCE' ORDER BY TABLE_NAME",
                (database,),
            )?;
            for row in sequence_rows {
                let name: String = required(&row, "TABLE_NAME")?;
                let statement = format!(
                    "SHOW CREATE SEQUENCE {}.{}",
                    quote_identifier(database),
                    quote_identifier(&name)
                );
                let definition_row =
                    connection
                        .query_first::<Row, _>(statement)?
                        .ok_or_else(|| {
                            CatalogError::Mapping(format!(
                            "sequence '{name}' disappeared while computing the catalog signature"
                        ))
                        })?;
                let definition = optional_at::<String>(&definition_row, 1)?.ok_or_else(|| {
                    CatalogError::PermissionDenied(format!(
                        "sequence '{name}' definition is hidden"
                    ))
                })?;
                values.push(format!("sequence-definition:{name}:{definition}"));
            }
        }
        values.sort();
        Ok(Self(values))
    }
}

#[derive(Clone, Debug)]
struct RawTable {
    name: String,
    table_type: String,
    engine: Option<String>,
    row_format: Option<String>,
    collation: Option<String>,
    create_options: Option<String>,
    comment: String,
}

#[derive(Clone, Debug)]
struct RawColumn {
    table: String,
    name: String,
    ordinal: u32,
    data_type: String,
    column_type: String,
    nullable: bool,
    default_value: Option<String>,
    character_set: Option<String>,
    collation: Option<String>,
    extra: String,
    privileges: String,
    comment: String,
    generation_expression: Option<String>,
    spatial_reference_id: Option<u64>,
    system_period_start: bool,
    system_period_end: bool,
}

#[derive(Clone, Debug)]
struct RawConstraint {
    table: String,
    name: String,
    constraint_type: String,
    enforced: bool,
}

#[derive(Clone, Debug)]
struct RawKeyUsage {
    table: String,
    constraint: String,
    column: String,
    ordinal: u32,
    referenced_schema: Option<String>,
    referenced_table: Option<String>,
    referenced_column: Option<String>,
}

#[derive(Clone, Debug)]
struct RawReferenceRule {
    table: String,
    constraint: String,
    match_option: String,
    update_rule: String,
    delete_rule: String,
}

#[derive(Clone, Debug)]
struct RawCheck {
    table: String,
    constraint: String,
    clause: String,
}

#[derive(Clone, Debug)]
struct RawIndexPart {
    table: String,
    index: String,
    non_unique: bool,
    ordinal: u32,
    column: Option<String>,
    collation: Option<String>,
    prefix_length: Option<u64>,
    index_type: String,
    comment: String,
    index_comment: String,
    visible: bool,
    expression: Option<String>,
}

#[derive(Clone, Debug)]
struct RawView {
    name: String,
    definition: Option<String>,
    check_option: String,
    updatable: bool,
    definer: String,
    security_type: String,
    character_set: String,
    collation: String,
    algorithm: Option<String>,
}

#[derive(Clone, Debug)]
struct RawViewTableUsage {
    view: String,
    target_schema: String,
    target_name: String,
}

#[derive(Clone, Debug)]
struct RawViewRoutineUsage {
    view: String,
    routine_schema: String,
    specific_name: String,
}

#[derive(Clone, Debug)]
struct RawRoutine {
    specific_name: String,
    name: String,
    routine_type: String,
    data_type: String,
    dtd_identifier: Option<String>,
    definition: Option<String>,
    deterministic: bool,
    sql_data_access: String,
    security_type: String,
    sql_mode: String,
    comment: String,
    definer: String,
    character_set: Option<String>,
    collation: Option<String>,
    database_collation: String,
}

#[derive(Clone, Debug)]
struct RawParameter {
    specific_name: String,
    ordinal: u32,
    mode: Option<String>,
    name: Option<String>,
    data_type: String,
    dtd_identifier: Option<String>,
    routine_type: String,
    default_value: Option<String>,
}

#[derive(Clone, Debug)]
struct RawTrigger {
    name: String,
    event: String,
    table: String,
    action_order: u64,
    condition: Option<String>,
    statement: Option<String>,
    orientation: String,
    timing: String,
    sql_mode: String,
    definer: String,
    character_set: String,
    collation: String,
    database_collation: String,
}

#[derive(Clone, Debug)]
struct RawEvent {
    name: String,
    definer: String,
    time_zone: String,
    body: String,
    definition: Option<String>,
    event_type: String,
    execute_at: Option<String>,
    interval_value: Option<String>,
    interval_field: Option<String>,
    sql_mode: String,
    starts: Option<String>,
    ends: Option<String>,
    status: String,
    on_completion: String,
    comment: String,
}

#[derive(Clone, Debug)]
struct RawPartition {
    table: String,
    partition: String,
    subpartition: Option<String>,
    partition_ordinal: u32,
    subpartition_ordinal: Option<u32>,
    method: Option<String>,
    subpartition_method: Option<String>,
    expression: Option<String>,
    subpartition_expression: Option<String>,
    description: Option<String>,
    comment: String,
    tablespace: Option<String>,
}

#[derive(Clone, Debug)]
struct RawSequence {
    name: String,
    definition: Option<String>,
    data_type: Option<String>,
    start_value: Option<String>,
    minimum_value: Option<String>,
    maximum_value: Option<String>,
    increment: Option<String>,
    cycles: Option<bool>,
}

#[derive(Clone, Debug)]
struct RawMysqlFamilyCatalog {
    facts: ServerFacts,
    strategy: MysqlFamilyVersion,
    grants: BTreeSet<String>,
    active_roles: Vec<String>,
    transaction_read_only: bool,
    transaction_isolation: String,
    tables: Vec<RawTable>,
    columns: Vec<RawColumn>,
    constraints: Vec<RawConstraint>,
    key_usage: Vec<RawKeyUsage>,
    reference_rules: Vec<RawReferenceRule>,
    checks: Vec<RawCheck>,
    index_parts: Vec<RawIndexPart>,
    views: Vec<RawView>,
    view_table_usage: Vec<RawViewTableUsage>,
    view_routine_usage: Vec<RawViewRoutineUsage>,
    routines: Vec<RawRoutine>,
    parameters: Vec<RawParameter>,
    triggers: Vec<RawTrigger>,
    events: Vec<RawEvent>,
    partitions: Vec<RawPartition>,
    sequences: Vec<RawSequence>,
}

