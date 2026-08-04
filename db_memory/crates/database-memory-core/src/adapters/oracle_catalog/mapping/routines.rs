fn add_oracle_lob_index_inventory_properties(
    properties: &mut BTreeMap<String, MetadataValue>,
    inventory: &RawInventoryObject,
) {
    insert_i64(properties, "lob_index_object_id", inventory.object_id);
    insert_optional_i64(
        properties,
        "lob_index_data_object_id",
        inventory.data_object_id,
    );
    insert_string(properties, "lob_index_status", &inventory.status);
    insert_bool(properties, "lob_index_generated", inventory.generated);
}

fn oracle_trigger_definition(trigger: &RawTrigger) -> Result<String, CatalogError> {
    let description = trigger.description.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle trigger {}.{} has no complete description",
            trigger.owner, trigger.name
        ))
    })?;
    let body = trigger.body.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle trigger {}.{} has no complete body",
            trigger.owner, trigger.name
        ))
    })?;
    let definition = format!("CREATE OR REPLACE TRIGGER {description}\n{body}");
    if definition.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle trigger definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
            trigger.owner, trigger.name
        )));
    }
    Ok(definition)
}

fn oracle_trigger_properties(
    trigger: &RawTrigger,
    inventory_object: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "trigger_type", &trigger.trigger_type);
    insert_string(
        &mut properties,
        "triggering_event",
        &trigger.triggering_event,
    );
    insert_optional_string(
        &mut properties,
        "table_owner",
        trigger.table_owner.as_deref(),
    );
    insert_string(
        &mut properties,
        "base_object_type",
        &trigger.base_object_type,
    );
    insert_optional_string(&mut properties, "table_name", trigger.table_name.as_deref());
    insert_optional_string(
        &mut properties,
        "column_name",
        trigger.column_name.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "referencing_names",
        trigger.referencing_names.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "when_clause",
        trigger.when_clause.as_deref(),
    );
    insert_string(&mut properties, "status", &trigger.status);
    insert_string(&mut properties, "action_type", &trigger.action_type);
    insert_optional_string(
        &mut properties,
        "crossedition",
        trigger.crossedition.as_deref(),
    );
    insert_optional_string(&mut properties, "fire_once", trigger.fire_once.as_deref());
    insert_optional_string(
        &mut properties,
        "apply_server_only",
        trigger.apply_server_only.as_deref(),
    );
    properties
}

fn oracle_routine_properties(
    routine: &RawRoutine,
    inventory_object: &RawInventoryObject,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_i64(&mut properties, "object_id", routine.object_id);
    insert_i64(&mut properties, "subprogram_id", routine.subprogram_id);
    insert_optional_string(&mut properties, "overload", routine.overload.as_deref());
    insert_string(&mut properties, "object_type", &routine.object_type);
    insert_bool(&mut properties, "aggregate", routine.aggregate);
    insert_bool(&mut properties, "pipelined", routine.pipelined);
    insert_bool(&mut properties, "parallel", routine.parallel);
    insert_bool(&mut properties, "interface", routine.interface);
    insert_bool(&mut properties, "deterministic", routine.deterministic);
    insert_string(&mut properties, "authid", &routine.authid);
    insert_optional_string(
        &mut properties,
        "polymorphic",
        routine.polymorphic.as_deref(),
    );
    properties
}

fn oracle_routine_argument_properties(
    argument: &RawRoutineArgument,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "position", argument.position);
    insert_i64(&mut properties, "sequence", argument.sequence);
    insert_i64(&mut properties, "data_level", argument.data_level);
    insert_string(
        &mut properties,
        "data_type",
        format_oracle_argument_type(argument),
    );
    insert_string(&mut properties, "mode", &argument.mode);
    insert_bool(&mut properties, "defaulted", argument.defaulted);
    insert_optional_i64(&mut properties, "default_length", argument.default_length);
    insert_optional_string(
        &mut properties,
        "default_value",
        argument.default_value.as_deref(),
    );
    insert_optional_i64(&mut properties, "data_length", argument.data_length);
    insert_optional_i64(&mut properties, "data_precision", argument.data_precision);
    insert_optional_i64(&mut properties, "data_scale", argument.data_scale);
    insert_optional_string(&mut properties, "pls_type", argument.pls_type.as_deref());
    insert_optional_i64(&mut properties, "char_length", argument.char_length);
    insert_optional_string(&mut properties, "char_used", argument.char_used.as_deref());
    properties
}

fn validate_package_argument_order(
    routine: &RawPackageRoutine,
    arguments: &[&RawRoutineArgument],
) -> Result<(), CatalogError> {
    let return_count = arguments
        .iter()
        .filter(|argument| argument.position == 0)
        .count();
    if return_count > 1 {
        return Err(CatalogError::Mapping(format!(
            "Oracle package routine {}.{}.{} has {return_count} return rows",
            routine.owner, routine.package, routine.name
        )));
    }
    for (offset, argument) in arguments.iter().enumerate() {
        let expected_sequence = i64::try_from(offset + 1)
            .map_err(|_| CatalogError::Mapping("too many Oracle package arguments".to_owned()))?;
        if argument.sequence != expected_sequence {
            return Err(CatalogError::Mapping(format!(
                "Oracle package argument sequence gap for {}.{}.{}: expected {expected_sequence}, found {}",
                routine.owner, routine.package, routine.name, argument.sequence
            )));
        }
        let expected_position = if return_count == 1 {
            i64::try_from(offset).map_err(|_| {
                CatalogError::Mapping("too many Oracle package arguments".to_owned())
            })?
        } else {
            expected_sequence
        };
        if argument.position != expected_position {
            return Err(CatalogError::Mapping(format!(
                "Oracle package argument position mismatch for {}.{}.{}: expected {expected_position}, found {}",
                routine.owner, routine.package, routine.name, argument.position
            )));
        }
        if argument.position == 0 && (argument.name.is_some() || argument.mode != "OUT") {
            return Err(CatalogError::Mapping(format!(
                "Oracle package function return metadata is malformed for {}.{}.{}",
                routine.owner, routine.package, routine.name
            )));
        }
    }
    Ok(())
}

fn oracle_package_definition(package: &RawPackage) -> Result<String, CatalogError> {
    let specification = package.specification.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle package {}.{} has no specification",
            package.owner, package.name
        ))
    })?;
    let definition = package
        .body
        .as_deref()
        .map(|body| format!("{specification}\n\n{body}"))
        .unwrap_or_else(|| specification.to_owned());
    if definition.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "combined Oracle package definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
            package.owner, package.name
        )));
    }
    Ok(definition)
}

fn oracle_type_definition(user_type: &RawUserType) -> Result<String, CatalogError> {
    let specification = user_type.specification.as_deref().ok_or_else(|| {
        CatalogError::Mapping(format!(
            "Oracle type {}.{} has no specification",
            user_type.owner, user_type.name
        ))
    })?;
    let definition = user_type
        .body
        .as_deref()
        .map(|body| format!("{specification}\n\n{body}"))
        .unwrap_or_else(|| specification.to_owned());
    if definition.len() > MAX_DEFINITION_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "combined Oracle type definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
            user_type.owner, user_type.name
        )));
    }
    Ok(definition)
}

fn oracle_type_properties(
    user_type: &RawUserType,
    inventory_object: &RawInventoryObject,
    body_inventory: Option<&RawInventoryObject>,
    collection: Option<&RawCollectionType>,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "type_oid", &user_type.oid);
    insert_string(&mut properties, "typecode", &user_type.typecode);
    insert_i64(
        &mut properties,
        "attribute_count",
        user_type.attribute_count,
    );
    insert_i64(&mut properties, "method_count", user_type.method_count);
    insert_string(&mut properties, "predefined", &user_type.predefined);
    insert_string(&mut properties, "incomplete", &user_type.incomplete);
    insert_string(&mut properties, "final", &user_type.final_type);
    insert_string(&mut properties, "instantiable", &user_type.instantiable);
    insert_string(&mut properties, "persistable", &user_type.persistable);
    insert_optional_string(
        &mut properties,
        "supertype_owner",
        user_type.supertype_owner.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "supertype_name",
        user_type.supertype_name.as_deref(),
    );
    insert_optional_i64(
        &mut properties,
        "local_attribute_count",
        user_type.local_attribute_count,
    );
    insert_optional_i64(
        &mut properties,
        "local_method_count",
        user_type.local_method_count,
    );
    insert_optional_string(&mut properties, "type_id", user_type.type_id.as_deref());
    insert_bool(&mut properties, "has_body", user_type.body.is_some());
    if let Some(body_inventory) = body_inventory {
        insert_i64(&mut properties, "body_object_id", body_inventory.object_id);
        insert_string(&mut properties, "body_status", &body_inventory.status);
    }
    if let Some(collection) = collection {
        insert_string(
            &mut properties,
            "collection_type",
            &collection.collection_type,
        );
        insert_optional_i64(&mut properties, "upper_bound", collection.upper_bound);
        insert_optional_string(
            &mut properties,
            "element_type_modifier",
            collection.element_type_modifier.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "element_type_owner",
            collection.element_type_owner.as_deref(),
        );
        insert_string(
            &mut properties,
            "element_type_name",
            &collection.element_type_name,
        );
        insert_optional_i64(&mut properties, "element_length", collection.length);
        insert_optional_i64(&mut properties, "element_precision", collection.precision);
        insert_optional_i64(&mut properties, "element_scale", collection.scale);
        insert_optional_string(
            &mut properties,
            "element_character_set",
            collection.character_set.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "element_storage",
            collection.element_storage.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "nulls_stored",
            collection.nulls_stored.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "element_char_used",
            collection.char_used.as_deref(),
        );
    }
    properties
}

fn oracle_type_attribute_properties(
    attribute: &RawTypeAttribute,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "position", attribute.position);
    insert_string(&mut properties, "data_type", &attribute.data_type_name);
    insert_optional_string(
        &mut properties,
        "type_modifier",
        attribute.type_modifier.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "data_type_owner",
        attribute.data_type_owner.as_deref(),
    );
    insert_optional_i64(&mut properties, "length", attribute.length);
    insert_optional_i64(&mut properties, "precision", attribute.precision);
    insert_optional_i64(&mut properties, "scale", attribute.scale);
    insert_optional_string(
        &mut properties,
        "character_set",
        attribute.character_set.as_deref(),
    );
    insert_bool(&mut properties, "inherited", attribute.inherited == "YES");
    insert_optional_string(&mut properties, "char_used", attribute.char_used.as_deref());
    properties
}

fn oracle_type_method_properties(method: &RawTypeMethod) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "method_number", method.method_number);
    insert_string(&mut properties, "method_type", &method.method_type);
    insert_i64(&mut properties, "parameter_count", method.parameter_count);
    insert_i64(&mut properties, "result_count", method.result_count);
    insert_bool(&mut properties, "final", method.final_method == "YES");
    insert_bool(
        &mut properties,
        "instantiable",
        method.instantiable == "YES",
    );
    insert_bool(&mut properties, "overriding", method.overriding == "YES");
    insert_bool(&mut properties, "inherited", method.inherited == "YES");
    properties
}

fn oracle_type_method_parameter_properties(
    parameter: &RawTypeMethodParameter,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "position", parameter.position);
    insert_string(&mut properties, "mode", &parameter.mode);
    insert_string(&mut properties, "data_type", &parameter.data_type_name);
    insert_optional_string(
        &mut properties,
        "type_modifier",
        parameter.type_modifier.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "data_type_owner",
        parameter.data_type_owner.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "character_set",
        parameter.character_set.as_deref(),
    );
    insert_bool(&mut properties, "return_value", parameter.return_value);
    properties
}

fn oracle_package_properties(
    package: &RawPackage,
    inventory_object: &RawInventoryObject,
    body_inventory: Option<&RawInventoryObject>,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = inventory_properties(inventory_object);
    insert_string(&mut properties, "authid", &package.authid);
    insert_bool(&mut properties, "has_body", package.body.is_some());
    insert_i64(
        &mut properties,
        "specification_bytes",
        package
            .specification
            .as_ref()
            .map_or(0, |definition| definition.len()) as i64,
    );
    insert_i64(
        &mut properties,
        "body_bytes",
        package
            .body
            .as_ref()
            .map_or(0, |definition| definition.len()) as i64,
    );
    if let Some(body) = body_inventory {
        insert_i64(&mut properties, "body_object_id", body.object_id);
        insert_optional_i64(&mut properties, "body_data_object_id", body.data_object_id);
        insert_string(&mut properties, "body_status", &body.status);
        insert_bool(&mut properties, "body_generated", body.generated);
    }
    properties
}

fn oracle_package_routine_signature(
    routine: &RawPackageRoutine,
    arguments: &[&RawRoutineArgument],
) -> Result<String, CatalogError> {
    let parameters = arguments
        .iter()
        .filter(|argument| argument.position > 0)
        .map(|argument| {
            format!(
                "{} {}",
                argument.mode,
                format_oracle_argument_type(argument)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let return_type = arguments
        .iter()
        .find(|argument| argument.position == 0)
        .map(|argument| format!("->{}", format_oracle_argument_type(argument)))
        .unwrap_or_default();
    let signature = format!("{}({parameters}){return_type}", routine.name);
    if signature.len() > MAX_ROUTINE_SIGNATURE_BYTES {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle package routine signature exceeds {MAX_ROUTINE_SIGNATURE_BYTES} bytes for {}.{}.{}",
            routine.owner, routine.package, routine.name
        )));
    }
    Ok(signature)
}

fn oracle_package_routine_properties(
    routine: &RawPackageRoutine,
    signature: &str,
) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_i64(&mut properties, "object_id", routine.object_id);
    insert_i64(&mut properties, "subprogram_id", routine.subprogram_id);
    insert_optional_string(&mut properties, "overload", routine.overload.as_deref());
    insert_string(&mut properties, "signature", signature);
    insert_bool(&mut properties, "aggregate", routine.aggregate);
    insert_bool(&mut properties, "pipelined", routine.pipelined);
    insert_bool(&mut properties, "parallel", routine.parallel);
    insert_bool(&mut properties, "interface", routine.interface);
    insert_bool(&mut properties, "deterministic", routine.deterministic);
    insert_string(&mut properties, "authid", &routine.authid);
    insert_optional_string(
        &mut properties,
        "polymorphic",
        routine.polymorphic.as_deref(),
    );
    properties
}

fn format_oracle_argument_type(argument: &RawRoutineArgument) -> String {
    let data_type = argument
        .data_type
        .as_deref()
        .unwrap_or("UNSPECIFIED")
        .to_owned();
    match data_type.as_str() {
        "NUMBER" => match (argument.data_precision, argument.data_scale) {
            (Some(precision), Some(scale)) => format!("{data_type}({precision},{scale})"),
            (Some(precision), None) => format!("{data_type}({precision})"),
            _ => data_type,
        },
        "CHAR" | "VARCHAR2" | "NCHAR" | "NVARCHAR2" => argument
            .char_length
            .map(|length| {
                let unit = match argument.char_used.as_deref() {
                    Some("C") => " CHAR",
                    Some("B") => " BYTE",
                    _ => "",
                };
                format!("{data_type}({length}{unit})")
            })
            .unwrap_or(data_type),
        _ => data_type,
    }
}
