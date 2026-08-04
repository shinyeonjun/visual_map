pub(crate) fn index_database_for_request(
    request: IndexDatabaseRequest,
) -> Result<IndexDatabaseResult, InterfaceError> {
    let cache_path = cache_path(request.cache_path);
    ensure_parent_dir(&cache_path)
        .map_err(|error| InterfaceError::storage("could not create cache directory", error))?;
    let mut complete = CompleteIndexRequest::new(
        request.source,
        request.path.map(PathBuf::from),
        request.connection_string,
        request.alias,
    );
    complete.requested_catalogs = request.requested_catalogs;
    complete.requested_schemas = request.requested_schemas;
    complete.timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let store = GraphStore::open(&cache_path)
        .map_err(|error| InterfaceError::storage("could not open graph cache", error))?;
    index_complete_source(
        &store,
        &complete,
        now_unix_ms(),
        cache_path.display().to_string(),
    )
}

pub(crate) fn list_databases_for_request(
    request: ListDatabasesRequest,
) -> Result<ListDatabasesResult, String> {
    let cache_path = cache_path(request.cache_path);
    if !cache_path.exists() {
        return Ok(ListDatabasesResult {
            cache_path: cache_path.display().to_string(),
            snapshots: vec![],
        });
    }

    let store = GraphStore::open(&cache_path).map_err(|err| err.to_string())?;
    let snapshots = list_snapshot_summaries(&store).map_err(|error| error.to_string())?;

    Ok(ListDatabasesResult {
        cache_path: cache_path.display().to_string(),
        snapshots,
    })
}

pub(crate) fn get_contract_for_request(_: GetContractRequest) -> GetContractResult {
    product_contract()
}

pub(crate) fn list_snapshots_for_request(
    request: ListSnapshotsRequest,
) -> Result<ListSnapshotsResult, InterfaceError> {
    let cache_path = cache_path(request.cache_path);
    if !cache_path.exists() {
        return Ok(ListSnapshotsResult {
            contract_version: INTERFACE_CONTRACT_VERSION,
            cache_path: cache_path.display().to_string(),
            snapshots: Vec::new(),
        });
    }
    let store = GraphStore::open(&cache_path)
        .map_err(|error| InterfaceError::storage("could not open graph cache", error))?;
    Ok(ListSnapshotsResult {
        contract_version: INTERFACE_CONTRACT_VERSION,
        cache_path: cache_path.display().to_string(),
        snapshots: list_snapshot_summaries(&store)?,
    })
}

pub(crate) fn describe_snapshot_for_request(
    request: DescribeSnapshotRequest,
) -> Result<DescribeSnapshotResult, InterfaceError> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_contract_store(&cache_path)?;
    describe_snapshot(&store, &request.snapshot)
}

pub(crate) fn list_objects_for_request(
    request: ListObjectsRequest,
) -> Result<ObjectsResult, InterfaceError> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_contract_store(&cache_path)?;
    list_generic_objects(
        &store,
        &request.snapshot,
        parse_object_kind(request.kind.as_deref())?,
        None,
        request.offset.unwrap_or(0),
        request.limit,
    )
}

pub(crate) fn find_objects_for_request(
    request: FindObjectsRequest,
) -> Result<ObjectsResult, InterfaceError> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_contract_store(&cache_path)?;
    list_generic_objects(
        &store,
        &request.snapshot,
        parse_object_kind(request.kind.as_deref())?,
        Some(&request.query),
        request.offset.unwrap_or(0),
        request.limit,
    )
}

pub(crate) fn describe_object_for_request(
    request: DescribeObjectRequest,
) -> Result<DescribeObjectResult, InterfaceError> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_contract_store(&cache_path)?;
    describe_generic_object(
        &store,
        &request.snapshot,
        &request.object_key,
        request.relationship_limit,
    )
}

fn parse_object_kind(kind: Option<&str>) -> Result<Option<ObjectKind>, InterfaceError> {
    kind.map(|kind| {
        kind.parse().map_err(|_| {
            InterfaceError::invalid_request(
                InterfaceStage::ObjectLookup,
                format!("unknown object kind '{kind}'"),
                "use an object kind listed by get_contract",
            )
        })
    })
    .transpose()
}

fn open_existing_contract_store(cache_path: &Path) -> Result<GraphStore, InterfaceError> {
    if !cache_path.exists() {
        return Err(InterfaceError::invalid_request(
            InterfaceStage::SnapshotLookup,
            format!("cache path '{}' was not found", cache_path.display()),
            "run index_database first or provide an existing cache_path",
        ));
    }
    GraphStore::open(cache_path)
        .map_err(|error| InterfaceError::storage("could not open graph cache", error))
}

pub(crate) fn list_tables_for_request(
    request: ListTablesRequest,
) -> Result<ListTablesResult, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = resolve_snapshot_key(&store, &request.alias)?;
    require_snapshot(&store, &snapshot_key)?;
    let page = paginate(
        find_tables(&store, &snapshot_key, request.name_filter.as_deref())?,
        request.offset,
        request.limit,
    )?;
    let tables = page.items.iter().map(|item| item.table.clone()).collect();
    Ok(ListTablesResult {
        snapshot_key,
        tables,
        table_matches: page.items,
        page: page.metadata,
    })
}

pub(crate) fn describe_table_for_request(
    request: DescribeTableRequest,
) -> Result<TableDescription, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = resolve_snapshot_key(&store, &request.alias)?;
    require_snapshot(&store, &snapshot_key)?;
    describe_table(&store, &snapshot_key, &request.table_name)
}

pub(crate) fn find_table_for_request(request: FindTableRequest) -> Result<FindTableResult, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = resolve_snapshot_key(&store, &request.alias)?;
    require_snapshot(&store, &snapshot_key)?;
    let page = paginate(
        find_tables(&store, &snapshot_key, Some(&request.query))?,
        request.offset,
        request.limit,
    )?;
    let tables = page.items.iter().map(|item| item.table.clone()).collect();
    Ok(FindTableResult {
        snapshot_key,
        tables,
        table_matches: page.items,
        page: page.metadata,
    })
}

pub(crate) fn find_column_for_request(
    request: FindColumnRequest,
) -> Result<FindColumnResult, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = resolve_snapshot_key(&store, &request.alias)?;
    require_snapshot(&store, &snapshot_key)?;
    let page = paginate(
        find_columns(&store, &snapshot_key, &request.query)?,
        request.offset,
        request.limit,
    )?;
    Ok(FindColumnResult {
        snapshot_key: snapshot_key.clone(),
        columns: page.items,
        page: page.metadata,
    })
}

pub(crate) fn impact_analysis_for_request(request: ImpactAnalysisRequest) -> Result<Value, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = resolve_snapshot_key(&store, &request.alias)?;
    require_snapshot(&store, &snapshot_key)?;
    let direction = parse_direction(&request.direction)?;
    let object_key = resolve_object_key(
        &store,
        &snapshot_key,
        request.object_key.as_deref(),
        request.table.as_deref(),
        request.column.as_deref(),
    )?;
    let max_depth_requested = request.max_depth.unwrap_or(DEFAULT_TRAVERSAL_DEPTH);
    let max_depth_applied = max_depth_requested.min(MAX_TRAVERSAL_DEPTH);
    let result_limit_requested = request.result_limit.unwrap_or(DEFAULT_RESULT_LIMIT);
    let result_limit_applied = result_limit_requested.min(MAX_RESULT_LIMIT);
    let bounded = run_impact_analysis(
        &store,
        &snapshot_key,
        &object_key,
        direction,
        max_depth_applied,
        result_limit_applied,
    )
    .map_err(|err| err.to_string())?;
    let result_count = bounded
        .result
        .groups
        .iter()
        .map(|group| group.nodes.len())
        .sum::<usize>();
    let mut value = impact_json(&bounded.result);
    value["max_depth_requested"] = json!(max_depth_requested);
    value["max_depth_applied"] = json!(max_depth_applied);
    value["max_depth_clamped"] = json!(max_depth_requested != max_depth_applied);
    value["result_limit_requested"] = json!(result_limit_requested);
    value["result_limit_applied"] = json!(result_limit_applied);
    value["result_limit_clamped"] = json!(result_limit_requested != result_limit_applied);
    value["result_count"] = json!(result_count);
    value["truncated"] = json!(bounded.truncated);
    value["capability_warnings"] = json!(snapshot_capability_warnings(&store, &snapshot_key)?);
    Ok(value)
}

pub(crate) fn trace_relationships_for_request(
    request: TraceRelationshipsRequest,
) -> Result<Value, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = resolve_snapshot_key(&store, &request.alias)?;
    require_snapshot(&store, &snapshot_key)?;
    let direction = parse_direction(&request.direction)?;
    required_node(&store, &snapshot_key, &request.start_object_key)?;
    let max_depth_requested = request.max_depth.unwrap_or(DEFAULT_TRAVERSAL_DEPTH);
    let max_depth_applied = max_depth_requested.min(MAX_TRAVERSAL_DEPTH);
    let result_limit_requested = request.result_limit.unwrap_or(DEFAULT_RESULT_LIMIT);
    let result_limit_applied = result_limit_requested.min(MAX_RESULT_LIMIT);
    let bounded = run_trace_relationships(
        &store,
        &snapshot_key,
        &request.start_object_key,
        direction,
        max_depth_applied,
        result_limit_applied,
    )
    .map_err(|err| err.to_string())?;
    Ok(json!({
        "snapshot_key": snapshot_key,
        "start_object_key": request.start_object_key,
        "direction": direction_name(direction),
        "max_depth": max_depth_applied,
        "max_depth_requested": max_depth_requested,
        "max_depth_applied": max_depth_applied,
        "max_depth_clamped": max_depth_requested != max_depth_applied,
        "result_limit_requested": result_limit_requested,
        "result_limit_applied": result_limit_applied,
        "result_limit_clamped": result_limit_requested != result_limit_applied,
        "result_count": bounded.paths.len(),
        "truncated": bounded.truncated,
        "paths": graph_paths_json(&bounded.paths),
        "capability_warnings": snapshot_capability_warnings(&store, &snapshot_key)?,
    }))
}

pub(crate) fn schema_diff_for_request(request: SchemaDiffRequest) -> Result<Value, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let from_snapshot_key = resolve_snapshot_key(&store, &request.from_alias)?;
    let to_snapshot_key = resolve_snapshot_key(&store, &request.to_alias)?;
    require_snapshot(&store, &from_snapshot_key)?;
    require_snapshot(&store, &to_snapshot_key)?;
    let result_limit_requested = request.result_limit.unwrap_or(DEFAULT_RESULT_LIMIT);
    if result_limit_requested == 0 {
        return Err("result_limit must be greater than zero".to_owned());
    }
    let result_limit_applied = result_limit_requested.min(MAX_RESULT_LIMIT);
    let diff = run_schema_diff(
        &store,
        &from_snapshot_key,
        &to_snapshot_key,
        result_limit_applied,
    )
    .map_err(|err| err.to_string())?;
    Ok(schema_diff_json(
        &diff,
        result_limit_requested,
        result_limit_applied,
    ))
}

pub(crate) fn query_graph_for_request(
    request: QueryGraphRequest,
) -> Result<GraphQueryResult, String> {
    let cache_path = cache_path(request.cache_path);
    let store = open_existing_store(&cache_path)?;
    let snapshot_key = match (request.snapshot_key.clone(), request.alias.as_deref()) {
        (Some(snapshot_key), _) => snapshot_key,
        (None, Some(alias)) => resolve_snapshot_key(&store, alias)?,
        (None, None) => return Err("pass snapshot_key or alias".to_owned()),
    };
    require_snapshot(&store, &snapshot_key)?;
    let traversal = request
        .traversal
        .map(|traversal| {
            Ok::<_, String>(GraphQueryTraversal {
                start_node_key: traversal.start_node_key,
                direction: parse_direction(&traversal.direction)?,
                max_depth: traversal.max_depth,
            })
        })
        .transpose()?;
    if let Some(traversal) = &traversal {
        required_node(&store, &snapshot_key, &traversal.start_node_key)?;
    }
    run_query_graph(
        &store,
        &GraphQuery {
            snapshot_key,
            node_label: request.node_label,
            node_key_contains: request.node_key_contains,
            name_contains: request.name_contains,
            edge_type: request.edge_type,
            payload_array_min_len: request
                .payload_array_min_len
                .map(|filter| PayloadArrayMinLen {
                    field: filter.field,
                    min_len: filter.min_len,
                }),
            traversal,
            limit: request.limit,
        },
    )
    .map_err(|err| err.to_string())
}

