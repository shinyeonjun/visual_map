impl ServerFacts {
    async fn read(client: &mut TdsClient) -> Result<Self, CatalogError> {
        let row = query_one(
            client,
            "
            SELECT DB_NAME(),
                   CAST(SERVERPROPERTY('ProductVersion') AS nvarchar(128)),
                   CAST(SERVERPROPERTY('ProductMajorVersion') AS int),
                   CAST(SERVERPROPERTY('EngineEdition') AS int),
                   CAST(SERVERPROPERTY('Edition') AS nvarchar(128)),
                   USER_NAME(),
                   SUSER_SNAME(),
                   ORIGINAL_LOGIN(),
                   d.collation_name,
                   d.compatibility_level,
                   d.is_read_only,
                   d.containment_desc,
                   CASE WHEN CONNECTIONPROPERTY('encrypt_option') = 'TRUE'
                        THEN CAST(1 AS bit) ELSE CAST(0 AS bit) END
            FROM sys.databases d
            WHERE d.database_id = DB_ID()
            ",
        )
        .await?;
        Ok(Self {
            database: required_string(&row, 0, "database")?,
            version: required_string(&row, 1, "product version")?,
            major: required_value(&row, 2, "product major version")?,
            engine_edition: required_value(&row, 3, "engine edition")?,
            edition: required_string(&row, 4, "edition")?,
            current_user: required_string(&row, 5, "current user")?,
            login: required_string(&row, 6, "login")?,
            original_login: required_string(&row, 7, "original login")?,
            collation: required_string(&row, 8, "database collation")?,
            compatibility_level: required_value(&row, 9, "compatibility level")?,
            database_read_only: required_value(&row, 10, "database read-only state")?,
            containment: required_string(&row, 11, "containment")?,
            encrypted_transport: required_value(&row, 12, "transport encryption state")?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SqlServerCatalogVersion {
    V2017,
    V2019,
    V2022,
    V2025,
}

impl SqlServerCatalogVersion {
    fn detect(facts: &ServerFacts) -> Result<Self, CatalogError> {
        if !matches!(facts.engine_edition, 2..=4) {
            return Err(CatalogError::UnsupportedProduct(format!(
                "SQL Server engine edition {} ({}) is not in the live-certified Database Engine matrix",
                facts.engine_edition, facts.edition
            )));
        }
        match facts.major {
            14 => Ok(Self::V2017),
            15 => Ok(Self::V2019),
            16 => Ok(Self::V2022),
            17 => Ok(Self::V2025),
            major => Err(CatalogError::UnsupportedVersion(major)),
        }
    }

    fn strategy_name(self) -> &'static str {
        match self {
            Self::V2017 => "sqlserver-2017",
            Self::V2019 => "sqlserver-2019",
            Self::V2022 => "sqlserver-2022",
            Self::V2025 => "sqlserver-2025",
        }
    }

    fn ledger_expression(self) -> &'static str {
        match self {
            Self::V2017 | Self::V2019 => "N'NONE'",
            Self::V2022 | Self::V2025 => "t.ledger_type_desc",
        }
    }

    fn xml_compression_expression(self) -> &'static str {
        match self {
            Self::V2017 | Self::V2019 => "N'OFF'",
            Self::V2022 | Self::V2025 => "p.xml_compression_desc",
        }
    }

    fn routine_inline_expressions(self) -> (&'static str, &'static str) {
        match self {
            Self::V2017 => ("CAST(0 AS bit)", "CAST(0 AS bit)"),
            Self::V2019 | Self::V2022 | Self::V2025 => (
                "COALESCE(m.is_inlineable, CAST(0 AS bit))",
                "CASE WHEN COALESCE(m.inline_type, 0) = 1 THEN CAST(1 AS bit) ELSE CAST(0 AS bit) END",
            ),
        }
    }

    fn edge_constraint_union(self) -> &'static str {
        match self {
            Self::V2017 => "",
            Self::V2019 | Self::V2022 | Self::V2025 => {
                "UNION ALL
                 SELECT s.name, ec.name, N'EC', N'EDGE_CONSTRAINT'
                 FROM sys.edge_constraints ec
                 JOIN sys.schemas s ON s.schema_id = ec.schema_id"
            }
        }
    }
}

async fn verify_metadata_privileges(client: &mut TdsClient) -> Result<(), CatalogError> {
    let row = query_one(
        client,
        "
        SELECT HAS_PERMS_BY_NAME(DB_NAME(), 'DATABASE', 'VIEW DEFINITION'),
               HAS_PERMS_BY_NAME(N'sys.sql_expression_dependencies', 'OBJECT', 'SELECT')
        ",
    )
    .await?;
    let view_definition: i32 = required_value(&row, 0, "VIEW DEFINITION probe")?;
    let dependency_select: i32 = required_value(&row, 1, "dependency SELECT probe")?;
    if view_definition != 1 || dependency_select != 1 {
        return Err(CatalogError::PermissionDenied(format!(
            "effective metadata permissions are incomplete: VIEW DEFINITION={view_definition}, dependency catalog SELECT={dependency_select}"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSchema {
    id: i32,
    name: String,
    principal_id: i32,
}

async fn read_schemas(client: &mut TdsClient) -> Result<Vec<RawSchema>, CatalogError> {
    rows(
        client,
        "
        SELECT s.schema_id, s.name, s.principal_id
        FROM sys.schemas s
        LEFT JOIN sys.database_principals p ON p.principal_id = s.principal_id
        WHERE s.name NOT IN (N'sys', N'INFORMATION_SCHEMA', N'guest')
          AND NOT (COALESCE(p.is_fixed_role, 0) = 1 AND p.name = s.name)
        ORDER BY s.schema_id
    ",
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(RawSchema {
            id: required_value(&row, 0, "schema id")?,
            name: required_string(&row, 1, "schema name")?,
            principal_id: required_value(&row, 2, "schema owner")?,
        })
    })
    .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPrincipal {
    id: i32,
    name: String,
    type_code: String,
    type_desc: String,
    default_schema: Option<String>,
    authentication_type: String,
    fixed_role: bool,
    owning_principal_id: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTable {
    id: i32,
    schema: String,
    name: String,
    principal_id: Option<i32>,
    lob_data_space_id: i32,
    filestream_data_space_id: Option<i32>,
    replicated: bool,
    merge_published: bool,
    sync_tran_subscribed: bool,
    cdc_tracked: bool,
    lock_on_bulk_load: bool,
    file_table: bool,
    memory_optimized: bool,
    durability: String,
    temporal_type: String,
    history_schema: Option<String>,
    history_table: Option<String>,
    remote_data_archive: bool,
    external: bool,
    node: bool,
    edge: bool,
    ledger_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawColumn {
    object_id: i32,
    object_type: String,
    schema: String,
    relation: String,
    id: i32,
    name: String,
    type_id: i32,
    type_schema: String,
    type_name: String,
    max_length: i16,
    precision: u8,
    scale: u8,
    collation: Option<String>,
    nullable: bool,
    ansi_padded: bool,
    rowguid: bool,
    identity: bool,
    identity_seed: Option<String>,
    identity_increment: Option<String>,
    computed: bool,
    computed_definition: Option<String>,
    computed_definition_bytes: i32,
    persisted: Option<bool>,
    default_definition: Option<String>,
    default_definition_bytes: i32,
    default_object_id: i32,
    filestream: bool,
    replicated: bool,
    non_sql_subscribed: bool,
    merge_published: bool,
    dts_replicated: bool,
    xml_document: bool,
    xml_collection_id: i32,
    sparse: bool,
    column_set: bool,
    generated_always: String,
    encryption_type: Option<String>,
    hidden: bool,
    masked: bool,
    masking_function: Option<String>,
    graph_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawConstraint {
    id: i32,
    schema: String,
    table: String,
    table_id: i32,
    name: String,
    kind: ConstraintKind,
    columns: Vec<RawConstraintColumn>,
    referenced_schema: Option<String>,
    referenced_table: Option<String>,
    referenced_table_id: Option<i32>,
    delete_action: Option<String>,
    update_action: Option<String>,
    disabled: bool,
    not_trusted: bool,
    not_for_replication: bool,
    expression: Option<String>,
    expression_bytes: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawConstraintColumn {
    ordinal: i32,
    column_id: i32,
    name: String,
    referenced_column_id: Option<i32>,
    referenced_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIndex {
    object_id: i32,
    schema: String,
    relation: String,
    relation_type: String,
    id: i32,
    name: String,
    type_code: u8,
    type_desc: String,
    unique: bool,
    primary: bool,
    unique_constraint: bool,
    disabled: bool,
    hypothetical: bool,
    padded: bool,
    fill_factor: u8,
    ignore_duplicate_key: bool,
    allow_row_locks: bool,
    allow_page_locks: bool,
    auto_created: bool,
    filter: Option<String>,
    filter_bytes: i32,
    data_space_id: i32,
    columns: Vec<RawIndexColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIndexColumn {
    index_column_id: i32,
    column_id: i32,
    name: String,
    key_ordinal: i32,
    partition_ordinal: i32,
    descending: bool,
    included: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawView {
    id: i32,
    schema: String,
    name: String,
    principal_id: Option<i32>,
    replicated: bool,
    replication_filter: bool,
    schema_bound: bool,
    ansi_nulls: bool,
    quoted_identifier: bool,
    execute_as_principal_id: Option<i32>,
    definition: Option<String>,
    definition_bytes: i32,
    indexed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawRoutine {
    id: i32,
    schema: String,
    name: String,
    type_code: String,
    type_desc: String,
    principal_id: Option<i32>,
    schema_bound: bool,
    recompiled: bool,
    native_compilation: bool,
    ansi_nulls: bool,
    quoted_identifier: bool,
    execute_as_principal_id: Option<i32>,
    null_on_null_input: bool,
    inlineable: bool,
    inline_type: bool,
    startup: bool,
    replication: bool,
    definition: Option<String>,
    definition_bytes: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawParameter {
    object_id: i32,
    id: i32,
    name: String,
    type_id: i32,
    type_schema: String,
    type_name: String,
    max_length: i16,
    precision: u8,
    scale: u8,
    output: bool,
    readonly: bool,
    nullable: bool,
    default_value: Option<String>,
    xml_collection_id: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTrigger {
    id: i32,
    name: String,
    parent_class: i32,
    parent_id: i32,
    parent_schema: Option<String>,
    parent_name: Option<String>,
    parent_type: Option<String>,
    instead_of: bool,
    disabled: bool,
    not_for_replication: bool,
    schema_bound: bool,
    execute_as_principal_id: Option<i32>,
    definition: Option<String>,
    definition_bytes: i32,
    insert_event: bool,
    update_event: bool,
    delete_event: bool,
    events: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawUserType {
    id: i32,
    schema: String,
    name: String,
    system_type_id: u8,
    base_type: String,
    max_length: i16,
    precision: u8,
    scale: u8,
    collation: Option<String>,
    nullable: bool,
    user_defined: bool,
    assembly: bool,
    table_type: bool,
    table_object_id: Option<i32>,
    memory_optimized: bool,
    default_object_id: i32,
    rule_object_id: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSequence {
    id: i32,
    schema: String,
    name: String,
    principal_id: Option<i32>,
    type_id: i32,
    type_schema: String,
    type_name: String,
    precision: u8,
    scale: u8,
    start_value: String,
    increment: String,
    minimum_value: String,
    maximum_value: String,
    cyclic: bool,
    cache_size: Option<i32>,
    exhausted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSynonym {
    id: i32,
    schema: String,
    name: String,
    principal_id: Option<i32>,
    base_object_name: String,
    server: Option<String>,
    database: Option<String>,
    target_schema: Option<String>,
    target_entity: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawDependency {
    referencing_class: i32,
    referencing_id: i32,
    referencing_minor_id: i32,
    referenced_class: i32,
    referenced_server: Option<String>,
    referenced_database: Option<String>,
    referenced_schema: Option<String>,
    referenced_entity: String,
    referenced_id: Option<i32>,
    referenced_minor_id: i32,
    schema_bound: bool,
    caller_dependent: bool,
    ambiguous: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartitionFunction {
    id: i32,
    name: String,
    fanout: i32,
    boundary_on_right: bool,
    system: bool,
    values: Vec<RawPartitionValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartitionValue {
    boundary_id: i32,
    value: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartitionScheme {
    id: i32,
    name: String,
    function_id: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartition {
    object_id: i32,
    index_id: i32,
    partition_number: i32,
    data_compression: String,
    xml_compression: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSecurityPolicy {
    id: i32,
    schema: String,
    name: String,
    principal_id: Option<i32>,
    enabled: bool,
    schema_bound: bool,
    predicates: Vec<RawSecurityPredicate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSecurityPredicate {
    id: i32,
    target_object_id: i32,
    predicate_type: String,
    operation: Option<String>,
    definition: String,
    definition_bytes: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawXmlSchemaCollection {
    id: i32,
    schema: String,
    name: String,
    principal_id: Option<i32>,
    created_at: String,
    modified_at: String,
    namespaces: Vec<RawXmlSchemaNamespace>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawXmlSchemaNamespace {
    id: i32,
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawExtendedProperty {
    class: u8,
    class_description: String,
    major_id: i32,
    minor_id: i32,
    name: String,
    value_type: Option<String>,
    value_precision: Option<i32>,
    value_scale: Option<i32>,
    value_max_length: Option<i32>,
    value_collation: Option<String>,
    display_value: Option<String>,
    value_hex: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawUnsupportedObject {
    schema: Option<String>,
    name: String,
    type_code: String,
    type_desc: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSqlServerCatalog {
    schemas: Vec<RawSchema>,
    principals: Vec<RawPrincipal>,
    tables: Vec<RawTable>,
    columns: Vec<RawColumn>,
    constraints: Vec<RawConstraint>,
    indexes: Vec<RawIndex>,
    views: Vec<RawView>,
    routines: Vec<RawRoutine>,
    parameters: Vec<RawParameter>,
    triggers: Vec<RawTrigger>,
    user_types: Vec<RawUserType>,
    sequences: Vec<RawSequence>,
    synonyms: Vec<RawSynonym>,
    dependencies: Vec<RawDependency>,
    partition_functions: Vec<RawPartitionFunction>,
    partition_schemes: Vec<RawPartitionScheme>,
    partitions: Vec<RawPartition>,
    security_policies: Vec<RawSecurityPolicy>,
    xml_schema_collections: Vec<RawXmlSchemaCollection>,
    extended_properties: Vec<RawExtendedProperty>,
}

