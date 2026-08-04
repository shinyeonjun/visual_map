fn map_routines(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema_key: &ObjectKey,
    raw_routines: &[RawRoutine],
    raw_parameters: &[RawParameter],
) -> Result<(Vec<RoutineObject>, BTreeMap<String, ObjectKey>), CatalogError> {
    let mut routines = Vec::new();
    let mut routine_keys = BTreeMap::new();
    for routine in raw_routines {
        let kind = match routine.routine_type.as_str() {
            "FUNCTION" => RoutineKind::Function,
            "PROCEDURE" => RoutineKind::Procedure,
            unsupported => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "routine '{}' has unsupported ROUTINE_TYPE '{unsupported}'",
                    routine.name
                )));
            }
        };
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Routine,
            &routine.name,
            Some(routine.specific_name.clone()),
        );
        let normalized = routine.specific_name.to_ascii_lowercase();
        if routine_keys.insert(normalized, key.clone()).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate routine specific name '{}'",
                routine.specific_name
            )));
        }
        routines.push(RoutineObject {
            key: key.clone(),
            schema_key: schema_key.clone(),
            name: routine.name.clone(),
            kind,
            definition: routine.definition.clone(),
            depends_on: Vec::new(),
        });
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "specific_name", &routine.specific_name);
        insert_string(&mut properties, "data_type", &routine.data_type);
        insert_optional_string(
            &mut properties,
            "dtd_identifier",
            routine.dtd_identifier.as_deref(),
        );
        insert_bool(&mut properties, "deterministic", routine.deterministic);
        insert_string(&mut properties, "sql_data_access", &routine.sql_data_access);
        insert_string(&mut properties, "security_type", &routine.security_type);
        insert_string(&mut properties, "sql_mode", &routine.sql_mode);
        insert_string(&mut properties, "comment", &routine.comment);
        insert_string(&mut properties, "definer", &routine.definer);
        insert_optional_string(
            &mut properties,
            "character_set_client",
            routine.character_set.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "collation_connection",
            routine.collation.as_deref(),
        );
        insert_string(
            &mut properties,
            "database_collation",
            &routine.database_collation,
        );
        add_annotation(metadata, &key, None, properties);
    }

    let mut parameter_ids = BTreeSet::new();
    for parameter in raw_parameters {
        let routine_key = routine_keys
            .get(&parameter.specific_name.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "parameter {}:{} has no routine owner",
                    parameter.specific_name, parameter.ordinal
                ))
            })?;
        let owner = raw_routines
            .iter()
            .find(|routine| routine.specific_name == parameter.specific_name)
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "parameter {}:{} lost its raw routine owner",
                    parameter.specific_name, parameter.ordinal
                ))
            })?;
        if parameter.routine_type != owner.routine_type {
            return Err(CatalogError::Mapping(format!(
                "parameter {}:{} routine type '{}' differs from owner type '{}'",
                parameter.specific_name,
                parameter.ordinal,
                parameter.routine_type,
                owner.routine_type
            )));
        }
        let identity = (parameter.specific_name.clone(), parameter.ordinal);
        if !parameter_ids.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate routine parameter {}:{}",
                parameter.specific_name, parameter.ordinal
            )));
        }
        let display_name = parameter.name.clone().unwrap_or_else(|| {
            if parameter.ordinal == 0 {
                "return"
            } else {
                "unnamed"
            }
            .to_owned()
        });
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::RoutineParameter,
            &owner.name,
            Some(format!(
                "{}:{}:{}",
                parameter.specific_name, parameter.ordinal, display_name
            )),
        );
        let mut properties = BTreeMap::new();
        insert_u64(
            &mut properties,
            "ordinal_position",
            parameter.ordinal as u64,
        );
        insert_optional_string(&mut properties, "mode", parameter.mode.as_deref());
        insert_string(&mut properties, "data_type", &parameter.data_type);
        insert_optional_string(
            &mut properties,
            "dtd_identifier",
            parameter.dtd_identifier.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "default_value",
            parameter.default_value.as_deref(),
        );
        metadata.objects.push(MetadataObject {
            key: key.clone(),
            parent_key: Some(routine_key.clone()),
            name: display_name,
            extension_kind: None,
            definition: None,
            properties,
        });
        metadata.relationships.push(MetadataRelationship {
            kind: MetadataRelationshipKind::HasParameter,
            from_key: routine_key,
            to_key: key,
            ordinal: Some(parameter.ordinal),
            properties: BTreeMap::new(),
        });
    }
    Ok((routines, routine_keys))
}
