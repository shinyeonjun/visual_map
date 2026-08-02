fn is_timeout_error(error: &oracle::Error) -> bool {
    error.dpi_code() == Some(1067) || error.oci_code() == Some(1013)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPrincipal {
    name: String,
    user_id: i64,
    account_status: String,
    common: bool,
    oracle_maintained: bool,
    default_collation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawInventoryObject {
    owner: String,
    name: String,
    subobject: Option<String>,
    object_id: i64,
    data_object_id: Option<i64>,
    object_type: String,
    status: String,
    temporary: bool,
    generated: bool,
    secondary: bool,
    namespace: i64,
    edition_name: Option<String>,
    editionable: Option<String>,
    default_collation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTable {
    owner: String,
    name: String,
    status: String,
    temporary: bool,
    partitioned: bool,
    iot_type: Option<String>,
    nested: bool,
    read_only: bool,
    has_identity: bool,
    duration: Option<String>,
    external: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawColumn {
    owner: String,
    table: String,
    name: String,
    column_id: Option<i64>,
    internal_column_id: i64,
    data_type: String,
    data_type_owner: Option<String>,
    data_length: i64,
    data_precision: Option<i64>,
    data_scale: Option<i64>,
    nullable: bool,
    default_value: Option<String>,
    hidden: bool,
    virtual_column: bool,
    user_generated: bool,
    default_on_null: bool,
    identity: bool,
    char_length: Option<i64>,
    char_used: Option<String>,
    collation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSequence {
    owner: String,
    name: String,
    min_value: Option<String>,
    max_value: Option<String>,
    increment_by: String,
    cycle: Option<String>,
    ordered: Option<String>,
    cache_size: String,
    scale: Option<String>,
    extend: Option<String>,
    sharded: Option<String>,
    session: Option<String>,
    keep_value: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIdentityColumn {
    owner: String,
    table: String,
    column: String,
    generation_type: Option<String>,
    sequence_name: String,
    options: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawView {
    owner: String,
    name: String,
    text_length: Option<i64>,
    definition: Option<String>,
    type_owner: Option<String>,
    view_type: Option<String>,
    superview: Option<String>,
    editioning: Option<String>,
    read_only: Option<String>,
    container_data: Option<String>,
    bequeath: Option<String>,
    default_collation: Option<String>,
    has_sensitive_column: Option<String>,
    admit_null: Option<String>,
    pdb_local_only: Option<String>,
    duality_view: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawMaterializedView {
    owner: String,
    name: String,
    container_name: String,
    query_length: Option<i64>,
    definition: Option<String>,
    updatable: Option<String>,
    master_link: Option<String>,
    rewrite_enabled: Option<String>,
    rewrite_capability: Option<String>,
    refresh_mode: Option<String>,
    refresh_method: Option<String>,
    build_mode: Option<String>,
    fast_refreshable: Option<String>,
    compile_state: Option<String>,
    use_no_index: Option<String>,
    segment_created: Option<String>,
    default_collation: Option<String>,
    on_query_computation: Option<String>,
    automatic: Option<String>,
    concurrent_refresh: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawSynonym {
    owner: String,
    name: String,
    target_owner: String,
    target_name: String,
    database_link: Option<String>,
    origin_container_id: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartitionedTable {
    owner: String,
    table: String,
    partitioning_type: String,
    subpartitioning_type: String,
    partition_count: i64,
    default_subpartition_count: i64,
    partitioning_key_count: i64,
    subpartitioning_key_count: i64,
    status: String,
    default_tablespace: Option<String>,
    interval: Option<String>,
    autolist: Option<String>,
    interval_subpartition: Option<String>,
    autolist_subpartition: Option<String>,
    automatic: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTablePartition {
    owner: String,
    table: String,
    composite: String,
    name: String,
    subpartition_count: i64,
    high_value: Option<String>,
    high_value_length: i64,
    position: i64,
    tablespace: Option<String>,
    compression: String,
    compress_for: Option<String>,
    interval: String,
    segment_created: String,
    indexing: String,
    read_only: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTableSubpartition {
    owner: String,
    table: String,
    partition: String,
    name: String,
    high_value: Option<String>,
    high_value_length: i64,
    partition_position: i64,
    position: i64,
    tablespace: Option<String>,
    compression: String,
    compress_for: Option<String>,
    interval: String,
    segment_created: String,
    indexing: String,
    read_only: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartitionedIndex {
    owner: String,
    index: String,
    table: String,
    partitioning_type: String,
    subpartitioning_type: String,
    partition_count: i64,
    default_subpartition_count: i64,
    partitioning_key_count: i64,
    subpartitioning_key_count: i64,
    locality: String,
    alignment: String,
    default_tablespace: Option<String>,
    interval: Option<String>,
    autolist: Option<String>,
    interval_subpartition: Option<String>,
    autolist_subpartition: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIndexPartition {
    owner: String,
    index: String,
    composite: String,
    name: String,
    subpartition_count: i64,
    high_value: Option<String>,
    high_value_length: i64,
    position: i64,
    status: String,
    tablespace: Option<String>,
    compression: String,
    interval: String,
    segment_created: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIndexSubpartition {
    owner: String,
    index: String,
    partition: String,
    name: String,
    high_value: Option<String>,
    high_value_length: i64,
    partition_position: i64,
    position: i64,
    status: String,
    tablespace: Option<String>,
    compression: String,
    interval: String,
    segment_created: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPartitionKeyColumn {
    owner: String,
    name: String,
    object_type: String,
    column: String,
    position: i64,
    collated_column_id: Option<i64>,
    subpartition: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawLob {
    owner: String,
    table: String,
    column: String,
    segment_name: String,
    tablespace: Option<String>,
    index_name: String,
    chunk: i64,
    pctversion: Option<i64>,
    retention: Option<i64>,
    freepools: Option<i64>,
    cache: String,
    logging: String,
    encrypt: String,
    compression: String,
    deduplication: String,
    in_row: String,
    format: String,
    partitioned: String,
    securefile: String,
    segment_created: String,
    retention_type: Option<String>,
    retention_value: Option<i64>,
    value_based: Option<String>,
    max_inline: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawLobPartition {
    owner: String,
    table: String,
    column: String,
    lob_name: String,
    table_partition: String,
    name: String,
    index_partition_name: String,
    position: i64,
    composite: String,
    chunk: i64,
    pctversion: Option<i64>,
    cache: String,
    in_row: String,
    tablespace: Option<String>,
    retention: Option<String>,
    logging: String,
    encrypt: String,
    compression: String,
    deduplication: String,
    securefile: String,
    segment_created: String,
    max_inline: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawLobSubpartition {
    owner: String,
    table: String,
    column: String,
    lob_name: String,
    lob_partition_name: String,
    table_subpartition: String,
    name: String,
    index_subpartition_name: String,
    position: i64,
    chunk: i64,
    pctversion: Option<i64>,
    cache: String,
    in_row: String,
    tablespace: Option<String>,
    retention: Option<String>,
    logging: String,
    encrypt: String,
    compression: String,
    deduplication: String,
    securefile: String,
    segment_created: String,
    max_inline: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawUserType {
    owner: String,
    name: String,
    oid: String,
    typecode: String,
    attribute_count: i64,
    method_count: i64,
    predefined: String,
    incomplete: String,
    final_type: String,
    instantiable: String,
    persistable: String,
    supertype_owner: Option<String>,
    supertype_name: Option<String>,
    local_attribute_count: Option<i64>,
    local_method_count: Option<i64>,
    type_id: Option<String>,
    specification: Option<String>,
    body: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTypeAttribute {
    owner: String,
    type_name: String,
    name: String,
    type_modifier: Option<String>,
    data_type_owner: Option<String>,
    data_type_name: String,
    length: Option<i64>,
    precision: Option<i64>,
    scale: Option<i64>,
    character_set: Option<String>,
    position: i64,
    inherited: String,
    char_used: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawCollectionType {
    owner: String,
    type_name: String,
    collection_type: String,
    upper_bound: Option<i64>,
    element_type_modifier: Option<String>,
    element_type_owner: Option<String>,
    element_type_name: String,
    length: Option<i64>,
    precision: Option<i64>,
    scale: Option<i64>,
    character_set: Option<String>,
    element_storage: Option<String>,
    nulls_stored: Option<String>,
    char_used: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTypeMethod {
    owner: String,
    type_name: String,
    name: String,
    method_number: i64,
    method_type: String,
    parameter_count: i64,
    result_count: i64,
    final_method: String,
    instantiable: String,
    overriding: String,
    inherited: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTypeMethodParameter {
    owner: String,
    type_name: String,
    method_name: String,
    method_number: i64,
    name: String,
    position: i64,
    mode: String,
    type_modifier: Option<String>,
    data_type_owner: Option<String>,
    data_type_name: String,
    character_set: Option<String>,
    return_value: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawTrigger {
    owner: String,
    name: String,
    trigger_type: String,
    triggering_event: String,
    table_owner: Option<String>,
    base_object_type: String,
    table_name: Option<String>,
    column_name: Option<String>,
    referencing_names: Option<String>,
    when_clause: Option<String>,
    status: String,
    description: Option<String>,
    action_type: String,
    body: Option<String>,
    crossedition: Option<String>,
    fire_once: Option<String>,
    apply_server_only: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawRoutine {
    owner: String,
    name: String,
    object_id: i64,
    subprogram_id: i64,
    overload: Option<String>,
    object_type: String,
    aggregate: bool,
    pipelined: bool,
    parallel: bool,
    interface: bool,
    deterministic: bool,
    authid: String,
    polymorphic: Option<String>,
    definition: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawRoutineArgument {
    owner: String,
    routine: String,
    package_name: Option<String>,
    name: Option<String>,
    position: i64,
    sequence: i64,
    data_level: i64,
    data_type: Option<String>,
    defaulted: bool,
    default_length: Option<i64>,
    default_value: Option<String>,
    mode: String,
    data_length: Option<i64>,
    data_precision: Option<i64>,
    data_scale: Option<i64>,
    type_owner: Option<String>,
    type_name: Option<String>,
    type_subname: Option<String>,
    pls_type: Option<String>,
    char_length: Option<i64>,
    char_used: Option<String>,
    subprogram_id: i64,
    overload: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPackage {
    owner: String,
    name: String,
    object_id: i64,
    authid: String,
    specification: Option<String>,
    body: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawPackageRoutine {
    owner: String,
    package: String,
    name: String,
    object_id: i64,
    subprogram_id: i64,
    overload: Option<String>,
    aggregate: bool,
    pipelined: bool,
    parallel: bool,
    interface: bool,
    deterministic: bool,
    authid: String,
    polymorphic: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawConstraintColumn {
    name: String,
    position: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawConstraint {
    owner: String,
    table: String,
    name: String,
    constraint_type: String,
    search_condition: Option<String>,
    referenced_owner: Option<String>,
    referenced_constraint: Option<String>,
    delete_rule: Option<String>,
    status: String,
    deferrable: String,
    deferred: String,
    validated: String,
    generated: String,
    index_owner: Option<String>,
    index_name: Option<String>,
    invalid: Option<String>,
    view_related: Option<String>,
    columns: Vec<RawConstraintColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIndexColumn {
    name: String,
    position: i64,
    descending: bool,
    expression: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIndex {
    owner: String,
    table_owner: String,
    table: String,
    name: String,
    index_type: String,
    unique: bool,
    status: String,
    partitioned: bool,
    temporary: bool,
    generated: bool,
    secondary: bool,
    visibility: String,
    function_status: Option<String>,
    constraint_index: bool,
    columns: Vec<RawIndexColumn>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawDependency {
    owner: String,
    name: String,
    object_type: String,
    referenced_owner: String,
    referenced_name: String,
    referenced_type: String,
    referenced_link: Option<String>,
    dependency_type: String,
    referenced_owner_oracle_maintained: bool,
}

type CollapsedDependencyIdentity = (String, String, String, String, String);

#[derive(Default)]
struct CollapsedDependencyEvidence {
    source_object_types: BTreeSet<String>,
    dependency_types: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawOracleCatalog {
    inventory: Vec<RawInventoryObject>,
    tables: Vec<RawTable>,
    columns: Vec<RawColumn>,
    sequences: Vec<RawSequence>,
    identity_columns: Vec<RawIdentityColumn>,
    views: Vec<RawView>,
    view_columns: Vec<RawColumn>,
    materialized_views: Vec<RawMaterializedView>,
    synonyms: Vec<RawSynonym>,
    user_types: Vec<RawUserType>,
    type_attributes: Vec<RawTypeAttribute>,
    collection_types: Vec<RawCollectionType>,
    type_methods: Vec<RawTypeMethod>,
    type_method_parameters: Vec<RawTypeMethodParameter>,
    triggers: Vec<RawTrigger>,
    routines: Vec<RawRoutine>,
    routine_arguments: Vec<RawRoutineArgument>,
    packages: Vec<RawPackage>,
    package_routines: Vec<RawPackageRoutine>,
    package_arguments: Vec<RawRoutineArgument>,
    constraints: Vec<RawConstraint>,
    indexes: Vec<RawIndex>,
    partitioned_tables: Vec<RawPartitionedTable>,
    table_partitions: Vec<RawTablePartition>,
    table_subpartitions: Vec<RawTableSubpartition>,
    partitioned_indexes: Vec<RawPartitionedIndex>,
    index_partitions: Vec<RawIndexPartition>,
    index_subpartitions: Vec<RawIndexSubpartition>,
    partition_key_columns: Vec<RawPartitionKeyColumn>,
    lobs: Vec<RawLob>,
    lob_partitions: Vec<RawLobPartition>,
    lob_subpartitions: Vec<RawLobSubpartition>,
    dependencies: Vec<RawDependency>,
}

impl RawOracleCatalog {
    fn read(
        connection: &Connection,
        scope: &DictionaryScope,
        deadline: Instant,
    ) -> Result<Self, CatalogError> {
        reject_database_links(connection, scope, deadline)
            .map_err(|error| error.catalog_context("database-link"))?;
        let recycle = read_recycle_bin(connection, scope, deadline)
            .map_err(|error| error.catalog_context("recycle-bin"))?;
        let inventory = read_inventory(connection, scope, &recycle, deadline)
            .map_err(|error| error.catalog_context("object-inventory"))?;
        let tables = read_tables(connection, scope, &recycle, deadline)
            .map_err(|error| error.catalog_context("table"))?;
        let columns = read_columns(connection, scope, &recycle, deadline)
            .map_err(|error| error.catalog_context("column"))?;
        let sequences = read_sequences(connection, scope, deadline)
            .map_err(|error| error.catalog_context("sequence"))?;
        let identity_columns = read_identity_columns(connection, scope, deadline)
            .map_err(|error| error.catalog_context("identity-column"))?;
        let views = read_views(connection, scope, deadline)
            .map_err(|error| error.catalog_context("view"))?;
        let view_columns = read_view_columns(connection, scope, deadline)
            .map_err(|error| error.catalog_context("view-column"))?;
        let materialized_views = read_materialized_views(connection, scope, deadline)
            .map_err(|error| error.catalog_context("materialized-view"))?;
        let synonyms = read_synonyms(connection, scope, deadline)
            .map_err(|error| error.catalog_context("synonym"))?;
        let mut user_types = read_user_types(connection, scope, deadline)
            .map_err(|error| error.catalog_context("type"))?;
        attach_type_sources(connection, scope, &mut user_types, deadline)
            .map_err(|error| error.catalog_context("type-source"))?;
        let type_attributes = read_type_attributes(connection, scope, deadline)
            .map_err(|error| error.catalog_context("type-attribute"))?;
        let collection_types = read_collection_types(connection, scope, deadline)
            .map_err(|error| error.catalog_context("collection-type"))?;
        let type_methods = read_type_methods(connection, scope, deadline)
            .map_err(|error| error.catalog_context("type-method"))?;
        let type_method_parameters = read_type_method_parameters(connection, scope, deadline)
            .map_err(|error| error.catalog_context("type-method-parameter"))?;
        let triggers = read_triggers(connection, scope, deadline)
            .map_err(|error| error.catalog_context("trigger"))?;
        let mut routines = read_routines(connection, scope, deadline)
            .map_err(|error| error.catalog_context("routine"))?;
        attach_routine_sources(connection, scope, &mut routines, deadline)
            .map_err(|error| error.catalog_context("routine-source"))?;
        let routine_arguments = read_routine_arguments(connection, scope, deadline)
            .map_err(|error| error.catalog_context("routine-argument"))?;
        let mut packages = read_packages(connection, scope, deadline)
            .map_err(|error| error.catalog_context("package"))?;
        attach_package_sources(connection, scope, &mut packages, deadline)
            .map_err(|error| error.catalog_context("package-source"))?;
        let package_routines = read_package_routines(connection, scope, deadline)
            .map_err(|error| error.catalog_context("package-routine"))?;
        let package_arguments = read_package_arguments(connection, scope, deadline)
            .map_err(|error| error.catalog_context("package-argument"))?;
        let mut constraints = read_constraints(connection, scope, &recycle, deadline)
            .map_err(|error| error.catalog_context("constraint"))?;
        attach_constraint_columns(connection, scope, &mut constraints, deadline)
            .map_err(|error| error.catalog_context("constraint-column"))?;
        let mut indexes = read_indexes(connection, scope, &recycle, deadline)
            .map_err(|error| error.catalog_context("index"))?;
        attach_index_columns(connection, scope, &mut indexes, deadline)
            .map_err(|error| error.catalog_context("index-column"))?;
        attach_index_expressions(connection, scope, &mut indexes, deadline)
            .map_err(|error| error.catalog_context("index-expression"))?;
        let partitioned_tables = read_partitioned_tables(connection, scope, deadline)
            .map_err(|error| error.catalog_context("partitioned-table"))?;
        let table_partitions = read_table_partitions(connection, scope, deadline)
            .map_err(|error| error.catalog_context("table-partition"))?;
        let table_subpartitions = read_table_subpartitions(connection, scope, deadline)
            .map_err(|error| error.catalog_context("table-subpartition"))?;
        let partitioned_indexes = read_partitioned_indexes(connection, scope, deadline)
            .map_err(|error| error.catalog_context("partitioned-index"))?;
        let index_partitions = read_index_partitions(connection, scope, deadline)
            .map_err(|error| error.catalog_context("index-partition"))?;
        let index_subpartitions = read_index_subpartitions(connection, scope, deadline)
            .map_err(|error| error.catalog_context("index-subpartition"))?;
        let partition_key_columns = read_partition_key_columns(connection, scope, deadline)
            .map_err(|error| error.catalog_context("partition-key-column"))?;
        let lobs =
            read_lobs(connection, scope, deadline).map_err(|error| error.catalog_context("lob"))?;
        let lob_partitions = read_lob_partitions(connection, scope, deadline)
            .map_err(|error| error.catalog_context("lob-partition"))?;
        let lob_subpartitions = read_lob_subpartitions(connection, scope, deadline)
            .map_err(|error| error.catalog_context("lob-subpartition"))?;
        let dependencies = read_dependencies(connection, scope, deadline)
            .map_err(|error| error.catalog_context("dependency"))?;
        if Instant::now() >= deadline {
            return Err(CatalogError::Timeout);
        }
        Ok(Self {
            inventory,
            tables,
            columns,
            sequences,
            identity_columns,
            views,
            view_columns,
            materialized_views,
            synonyms,
            user_types,
            type_attributes,
            collection_types,
            type_methods,
            type_method_parameters,
            triggers,
            routines,
            routine_arguments,
            packages,
            package_routines,
            package_arguments,
            constraints,
            indexes,
            partitioned_tables,
            table_partitions,
            table_subpartitions,
            partitioned_indexes,
            index_partitions,
            index_subpartitions,
            partition_key_columns,
            lobs,
            lob_partitions,
            lob_subpartitions,
            dependencies,
        })
    }
}

