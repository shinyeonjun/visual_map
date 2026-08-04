#[allow(clippy::too_many_arguments)]
fn map_triggers(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    lower_case_table_names: u64,
    raw_triggers: &[RawTrigger],
    table_keys: &BTreeMap<String, ObjectKey>,
) -> Result<Vec<TriggerObject>, CatalogError> {
    let mut triggers = Vec::new();
    let mut seen = BTreeSet::new();
    for trigger in raw_triggers {
        if !seen.insert(trigger.name.to_ascii_lowercase()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate trigger '{}'",
                trigger.name
            )));
        }
        let table_name = normalize_object_name(&trigger.table, lower_case_table_names);
        let table_key = table_keys.get(&table_name).cloned().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "trigger '{}' targets missing base table '{}'",
                trigger.name, trigger.table
            ))
        })?;
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Trigger,
            &trigger.table,
            Some(trigger.name.clone()),
        );
        triggers.push(TriggerObject {
            key: key.clone(),
            table_key,
            name: trigger.name.clone(),
            timing: Some(trigger.timing.clone()),
            events: vec![trigger.event.clone()],
            definition: trigger.statement.clone(),
            executes_routine_key: None,
        });
        let mut properties = BTreeMap::new();
        insert_u64(&mut properties, "action_order", trigger.action_order);
        insert_optional_string(
            &mut properties,
            "action_condition",
            trigger.condition.as_deref(),
        );
        insert_string(&mut properties, "orientation", &trigger.orientation);
        insert_string(&mut properties, "sql_mode", &trigger.sql_mode);
        insert_string(&mut properties, "definer", &trigger.definer);
        insert_string(
            &mut properties,
            "character_set_client",
            &trigger.character_set,
        );
        insert_string(&mut properties, "collation_connection", &trigger.collation);
        insert_string(
            &mut properties,
            "database_collation",
            &trigger.database_collation,
        );
        add_annotation(metadata, &key, None, properties);
    }
    Ok(triggers)
}

fn map_events(
    metadata: &mut CanonicalMetadata,
    source_kind: &str,
    connection_alias: &str,
    database: &str,
    schema_key: &ObjectKey,
    raw_events: &[RawEvent],
) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for event in raw_events {
        if !seen.insert(event.name.to_ascii_lowercase()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate scheduled event '{}'",
                event.name
            )));
        }
        let key = family_key(
            source_kind,
            connection_alias,
            database,
            ObjectKind::Event,
            &event.name,
            None,
        );
        let mut properties = BTreeMap::new();
        insert_string(&mut properties, "definer", &event.definer);
        insert_string(&mut properties, "time_zone", &event.time_zone);
        insert_string(&mut properties, "body", &event.body);
        insert_string(&mut properties, "event_type", &event.event_type);
        insert_optional_string(&mut properties, "execute_at", event.execute_at.as_deref());
        insert_optional_string(
            &mut properties,
            "interval_value",
            event.interval_value.as_deref(),
        );
        insert_optional_string(
            &mut properties,
            "interval_field",
            event.interval_field.as_deref(),
        );
        insert_string(&mut properties, "sql_mode", &event.sql_mode);
        insert_optional_string(&mut properties, "starts", event.starts.as_deref());
        insert_optional_string(&mut properties, "ends", event.ends.as_deref());
        insert_string(&mut properties, "status", &event.status);
        insert_string(&mut properties, "on_completion", &event.on_completion);
        insert_string(&mut properties, "comment", &event.comment);
        metadata.objects.push(MetadataObject {
            key,
            parent_key: Some(schema_key.clone()),
            name: event.name.clone(),
            extension_kind: None,
            definition: event.definition.clone(),
            properties,
        });
    }
    Ok(())
}
