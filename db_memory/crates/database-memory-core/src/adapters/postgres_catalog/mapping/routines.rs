fn routine_properties(routine: &RawRoutine) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "postgres_oid", routine.oid);
    insert_string(
        &mut properties,
        "routine_kind",
        match routine.kind {
            'p' => "procedure",
            'a' => "aggregate",
            'w' => "window",
            _ => "function",
        },
    );
    insert_string(&mut properties, "language", &routine.language);
    insert_string(
        &mut properties,
        "identity_arguments",
        &routine.identity_arguments,
    );
    insert_string(&mut properties, "arguments", &routine.arguments_definition);
    insert_optional_string(
        &mut properties,
        "return_type",
        routine.return_type.as_deref(),
    );
    insert_bool(&mut properties, "returns_set", routine.returns_set);
    insert_bool(
        &mut properties,
        "security_definer",
        routine.security_definer,
    );
    insert_bool(&mut properties, "leakproof", routine.leakproof);
    insert_bool(&mut properties, "strict", routine.strict);
    insert_string(
        &mut properties,
        "volatility",
        match routine.volatility {
            'i' => "immutable",
            's' => "stable",
            _ => "volatile",
        },
    );
    insert_string(
        &mut properties,
        "parallel",
        match routine.parallel {
            's' => "safe",
            'r' => "restricted",
            _ => "unsafe",
        },
    );
    insert_bool(
        &mut properties,
        "body_catalog_tracked",
        routine.body_catalog_tracked,
    );
    properties
}

fn routine_parameter_mode(mode: char) -> &'static str {
    match mode {
        'o' => "out",
        'b' => "inout",
        'v' => "variadic",
        't' => "table",
        _ => "in",
    }
}

fn resolve_routine_dependency(
    dependency: &RawDependency,
    relations: &BTreeMap<i64, ObjectKey>,
    columns: &BTreeMap<(i64, i32), ObjectKey>,
    routines: &BTreeMap<i64, ObjectKey>,
    types: &BTreeMap<i64, ObjectKey>,
) -> Result<Option<ObjectKey>, CatalogError> {
    let schema = dependency.target_schema.as_deref().unwrap_or_default();
    if is_system_schema(schema) {
        return Ok(None);
    }
    let target = match dependency.target_class.as_str() {
        "relation" if dependency.target_sub_id > 0 => columns
            .get(&(dependency.target_oid, dependency.target_sub_id))
            .cloned(),
        "relation" => relations.get(&dependency.target_oid).cloned(),
        "routine" => routines.get(&dependency.target_oid).cloned(),
        "type" => types.get(&dependency.target_oid).cloned(),
        other => {
            return Err(CatalogError::Mapping(format!(
                "unsupported routine dependency target class '{other}'"
            )));
        }
    };
    target.map(Some).ok_or_else(|| {
        CatalogError::Mapping(format!(
            "routine dependency points outside the certified schema scope (class={}, schema={}, oid={}, subid={})",
            dependency.target_class,
            schema,
            dependency.target_oid,
            dependency.target_sub_id
        ))
    })
}
