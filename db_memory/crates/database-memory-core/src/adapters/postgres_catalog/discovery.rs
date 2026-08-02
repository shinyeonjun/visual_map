impl ServerFacts {
    fn major(&self) -> i32 {
        self.version_num / 10_000
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PostgresCatalogVersion {
    V14,
    V15,
    V16,
    V17,
    V18,
}

impl PostgresCatalogVersion {
    fn detect(version_num: i32) -> Result<Self, CatalogError> {
        let major = version_num / 10_000;
        match major {
            14 => Ok(Self::V14),
            15 => Ok(Self::V15),
            16 => Ok(Self::V16),
            17 => Ok(Self::V17),
            18 => Ok(Self::V18),
            _ => Err(CatalogError::UnsupportedVersion(major)),
        }
    }

    fn major(self) -> i32 {
        match self {
            Self::V14 => 14,
            Self::V15 => 15,
            Self::V16 => 16,
            Self::V17 => 17,
            Self::V18 => 18,
        }
    }

    const fn strategy_name(self) -> &'static str {
        match self {
            Self::V14 => "postgresql-14",
            Self::V15 => "postgresql-15",
            Self::V16 => "postgresql-16",
            Self::V17 => "postgresql-17",
            Self::V18 => "postgresql-18",
        }
    }

    fn statistics_target(
        self,
        raw_value: Option<i32>,
    ) -> Result<PostgresStatisticsTarget, CatalogError> {
        match (self, raw_value) {
            (Self::V14 | Self::V15 | Self::V16, Some(-1))
            | (Self::V17 | Self::V18, None) => Ok(PostgresStatisticsTarget::Default),
            (_, Some(0)) => Ok(PostgresStatisticsTarget::Disabled),
            (_, Some(value @ 1..)) => Ok(PostgresStatisticsTarget::Custom(value)),
            (Self::V14 | Self::V15 | Self::V16, None) => {
                Err(CatalogError::UnsupportedMetadata(format!(
                    "{} returned NULL pg_attribute.attstattarget; expected -1 for the default target",
                    self.strategy_name()
                )))
            }
            (Self::V17 | Self::V18, Some(-1)) => {
                Err(CatalogError::UnsupportedMetadata(format!(
                    "{} returned legacy -1 pg_attribute.attstattarget; expected NULL for the default target",
                    self.strategy_name()
                )))
            }
            (_, Some(value)) => Err(CatalogError::UnsupportedMetadata(format!(
                "{} returned unsupported pg_attribute.attstattarget value {value}",
                self.strategy_name()
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PgCatalogStrategy {
    PostgreSql(PostgresCatalogVersion),
    YugabyteDb2025_2_3_2,
}

impl PgCatalogStrategy {
    fn detect(product: PgWireProduct, server: &ServerFacts) -> Result<Self, CatalogError> {
        let version = server.version.trim();
        let banner = server.version_banner.trim();
        let yugabyte = version.to_ascii_uppercase().contains("-YB-")
            || banner.to_ascii_uppercase().contains("-YB-");

        match product {
            PgWireProduct::PostgreSql if yugabyte => Err(CatalogError::UnsupportedProduct(
                format!(
                    "connected product is YugabyteDB YSQL ({version}), not certified PostgreSQL"
                ),
            )),
            PgWireProduct::PostgreSql if !banner.starts_with("PostgreSQL ") => {
                let reported = banner.split_whitespace().next().unwrap_or("unknown");
                Err(CatalogError::UnsupportedProduct(format!(
                    "connected product reports '{reported}', not certified PostgreSQL"
                )))
            }
            PgWireProduct::PostgreSql => Ok(Self::PostgreSql(
                PostgresCatalogVersion::detect(server.version_num)?,
            )),
            PgWireProduct::YugabyteDb if !yugabyte => {
                Err(CatalogError::UnsupportedProduct(format!(
                    "connected product reports '{banner}', not YugabyteDB YSQL"
                )))
            }
            PgWireProduct::YugabyteDb
                if version != CERTIFIED_YUGABYTEDB_VERSION || server.version_num != 150_012 =>
            {
                Err(CatalogError::UnsupportedRelease(format!(
                    "YugabyteDB YSQL release '{version}' is not the certified {CERTIFIED_YUGABYTEDB_VERSION} release"
                )))
            }
            PgWireProduct::YugabyteDb => Ok(Self::YugabyteDb2025_2_3_2),
        }
    }

    const fn catalog_version(self) -> PostgresCatalogVersion {
        match self {
            Self::PostgreSql(version) => version,
            Self::YugabyteDb2025_2_3_2 => PostgresCatalogVersion::V15,
        }
    }

    const fn strategy_name(self) -> &'static str {
        match self {
            Self::PostgreSql(version) => version.strategy_name(),
            Self::YugabyteDb2025_2_3_2 => "yugabytedb-2025.2.3.2-pg15",
        }
    }

    const fn source_kind(self) -> &'static str {
        match self {
            Self::PostgreSql(_) => POSTGRES_SOURCE,
            Self::YugabyteDb2025_2_3_2 => YUGABYTEDB_SOURCE,
        }
    }

    const fn product_name(self) -> &'static str {
        match self {
            Self::PostgreSql(_) => "PostgreSQL",
            Self::YugabyteDb2025_2_3_2 => "YugabyteDB",
        }
    }

    const fn adapter_name(self) -> &'static str {
        match self {
            Self::PostgreSql(_) => "database-memory-postgres-catalog",
            Self::YugabyteDb2025_2_3_2 => "database-memory-yugabytedb-catalog",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PostgresStatisticsTarget {
    Default,
    Disabled,
    Custom(i32),
}

#[derive(Clone, Debug)]
struct RawSchema {
    oid: i64,
    name: String,
    owner_oid: i64,
    has_usage: bool,
    comment: Option<String>,
}

#[derive(Clone, Debug)]
struct RawPrincipal {
    oid: i64,
    name: String,
    superuser: bool,
    inherit: bool,
    create_role: bool,
    create_database: bool,
    can_login: bool,
    replication: bool,
    bypass_rls: bool,
    valid_until: Option<String>,
}

#[derive(Clone, Debug)]
struct RawRelation {
    oid: i64,
    row_type_oid: i64,
    schema: String,
    name: String,
    relkind: char,
    persistence: char,
    owner_oid: i64,
    is_partition: bool,
    row_security: bool,
    force_row_security: bool,
    replica_identity: char,
    partition_bound: Option<String>,
    definition: Option<String>,
    definition_too_large: bool,
    comment: Option<String>,
}

#[derive(Clone, Debug)]
struct RawColumn {
    relation_oid: i64,
    relation_kind: char,
    schema: String,
    relation: String,
    attnum: i16,
    name: String,
    type_oid: i64,
    type_schema: String,
    data_type: String,
    nullable: bool,
    default_oid: Option<i64>,
    default_expression: Option<String>,
    default_too_large: bool,
    generated: char,
    identity: char,
    collation: Option<String>,
    compression: Option<String>,
    statistics_target: PostgresStatisticsTarget,
    comment: Option<String>,
}

#[derive(Clone, Debug)]
struct RawConstraint {
    oid: i64,
    schema: String,
    relation_oid: Option<i64>,
    domain_type_oid: Option<i64>,
    name: String,
    kind: char,
    columns: Vec<i16>,
    referenced_relation_oid: Option<i64>,
    referenced_columns: Vec<i16>,
    definition: Option<String>,
    definition_too_large: bool,
    deferrable: bool,
    initially_deferred: bool,
    validated: bool,
    no_inherit: bool,
    delete_action: char,
    update_action: char,
    match_type: char,
}

#[derive(Clone, Debug)]
struct RawIndex {
    oid: i64,
    relation_oid: i64,
    schema: String,
    relation: String,
    name: String,
    access_method: String,
    unique: bool,
    primary: bool,
    exclusion: bool,
    immediate: bool,
    clustered: bool,
    valid: bool,
    ready: bool,
    live: bool,
    replica_identity: bool,
    nulls_not_distinct: bool,
    key_count: i16,
    predicate: Option<String>,
    expression: Option<String>,
    definition: Option<String>,
    definition_too_large: bool,
}

#[derive(Clone, Debug)]
struct RawIndexTerm {
    index_oid: i64,
    ordinal: i16,
    column_number: i16,
    column_name: Option<String>,
    definition: String,
    is_key: bool,
    descending: bool,
    nulls_first: bool,
    operator_class: Option<String>,
    collation: Option<String>,
}

#[derive(Clone, Debug)]
struct RawType {
    oid: i64,
    schema: String,
    name: String,
    kind: char,
    owner_oid: i64,
    category: char,
    relation_oid: Option<i64>,
    base_type_oid: Option<i64>,
    base_type_schema: Option<String>,
    element_type_oid: Option<i64>,
    element_type_schema: Option<String>,
    not_null: bool,
    default_value: Option<String>,
    default_too_large: bool,
    collation: Option<String>,
    range_subtype_oid: Option<i64>,
    range_subtype_schema: Option<String>,
    multirange_type_oid: Option<i64>,
    multirange_type_schema: Option<String>,
    comment: Option<String>,
}

#[derive(Clone, Debug)]
struct RawEnumValue {
    type_oid: i64,
    label: String,
    sort_order: String,
}

#[derive(Clone, Debug)]
struct RawSequence {
    relation_oid: i64,
    type_oid: i64,
    start_value: i64,
    min_value: i64,
    max_value: i64,
    increment_by: i64,
    cycle: bool,
    cache_size: i64,
}

#[derive(Clone, Debug)]
struct RawRoutine {
    oid: i64,
    schema: String,
    name: String,
    identity_arguments: String,
    kind: char,
    owner_oid: i64,
    language: String,
    return_type_oid: i64,
    return_type_schema: String,
    return_type: Option<String>,
    returns_set: bool,
    security_definer: bool,
    leakproof: bool,
    strict: bool,
    volatility: char,
    parallel: char,
    definition: Option<String>,
    definition_too_large: bool,
    arguments_definition: String,
    body_catalog_tracked: bool,
}

#[derive(Clone, Debug)]
struct RawRoutineParameter {
    routine_oid: i64,
    ordinal: i32,
    name: Option<String>,
    mode: char,
    type_oid: i64,
    type_schema: String,
    data_type: String,
}

#[derive(Clone, Debug)]
struct RawTrigger {
    oid: i64,
    relation_oid: i64,
    routine_oid: i64,
    name: String,
    timing: String,
    events: Vec<String>,
    orientation: String,
    enabled: char,
    update_columns: Vec<i16>,
    when_expression: Option<String>,
    definition: Option<String>,
    definition_too_large: bool,
}

#[derive(Clone, Debug)]
struct RawInheritance {
    child_oid: i64,
    parent_oid: i64,
    sequence_number: i32,
    child_is_partition: bool,
}

#[derive(Clone, Debug)]
struct RawDependency {
    owner_oid: i64,
    target_class: String,
    target_oid: i64,
    target_sub_id: i32,
    target_schema: Option<String>,
    dependency_type: char,
}

#[derive(Clone, Debug)]
struct RawViewDependency {
    view_oid: i64,
    target_relation_oid: i64,
    target_column_number: i32,
    target_schema: String,
    dependency_type: char,
}

#[derive(Clone, Debug)]
struct RawSequenceUsage {
    column_relation_oid: i64,
    column_number: i32,
    sequence_oid: i64,
    dependency_type: char,
}

#[derive(Clone, Debug)]
struct RawPolicy {
    oid: i64,
    relation_oid: i64,
    name: String,
    command: char,
    permissive: bool,
    role_oids: Vec<i64>,
    using_expression: Option<String>,
    check_expression: Option<String>,
}

#[derive(Clone, Debug)]
struct RawExtension {
    oid: i64,
    name: String,
    owner_oid: i64,
    schema: Option<String>,
    relocatable: bool,
    version: String,
}

#[derive(Clone, Debug)]
struct RawEventTrigger {
    oid: i64,
    name: String,
    event: String,
    owner_oid: i64,
    routine_oid: i64,
    routine_schema: String,
    enabled: char,
    tags: Vec<String>,
}

#[derive(Clone, Debug)]
struct RawYugabyteRelationProperties {
    relation_oid: i64,
    relation_kind: char,
    tablespace_oid: i64,
    num_tablets: Option<i64>,
    num_hash_key_columns: Option<i64>,
    is_colocated: Option<bool>,
    tablegroup_oid: Option<i64>,
    colocation_id: Option<i64>,
    range_split_clause: Option<String>,
}

#[derive(Clone, Debug)]
struct RawYugabyteTablegroup {
    oid: i64,
    name: String,
    owner_oid: i64,
    tablespace_oid: i64,
    acl: Vec<String>,
    options: Vec<String>,
}

#[derive(Clone, Debug)]
struct RawYugabyteTablespace {
    oid: i64,
    name: String,
    owner_oid: i64,
    acl: Vec<String>,
    options: Vec<String>,
    comment: Option<String>,
}

#[derive(Clone, Debug)]
struct RawYugabyteCatalog {
    database_colocated: bool,
    database_default_tablespace_oid: i64,
    relation_properties: Vec<RawYugabyteRelationProperties>,
    tablegroups: Vec<RawYugabyteTablegroup>,
    tablespaces: Vec<RawYugabyteTablespace>,
}

#[derive(Clone, Debug)]
struct RawPostgresCatalog {
    server: ServerFacts,
    strategy: PgCatalogStrategy,
    schemas: Vec<RawSchema>,
    principals: Vec<RawPrincipal>,
    relations: Vec<RawRelation>,
    columns: Vec<RawColumn>,
    constraints: Vec<RawConstraint>,
    indexes: Vec<RawIndex>,
    index_terms: Vec<RawIndexTerm>,
    types: Vec<RawType>,
    enum_values: Vec<RawEnumValue>,
    sequences: Vec<RawSequence>,
    routines: Vec<RawRoutine>,
    routine_parameters: Vec<RawRoutineParameter>,
    triggers: Vec<RawTrigger>,
    inheritance: Vec<RawInheritance>,
    view_dependencies: Vec<RawViewDependency>,
    routine_dependencies: Vec<RawDependency>,
    sequence_usages: Vec<RawSequenceUsage>,
    policies: Vec<RawPolicy>,
    extensions: Vec<RawExtension>,
    event_triggers: Vec<RawEventTrigger>,
    extension_routine_oids: BTreeSet<i64>,
    yugabyte: Option<RawYugabyteCatalog>,
}

struct PostgresSnapshotMapper<'a> {
    connection_alias: &'a str,
    source_kind: &'static str,
}

