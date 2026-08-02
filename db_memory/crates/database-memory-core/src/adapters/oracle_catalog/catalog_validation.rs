fn validate_raw_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
) -> Result<(), CatalogError> {
    let inventory_all = raw
        .inventory
        .iter()
        .filter(|object| !object.secondary)
        .collect::<Vec<_>>();
    let unsupported = inventory_all
        .iter()
        .filter(|object| {
            !matches!(
                object.object_type.as_str(),
                "TABLE"
                    | "INDEX"
                    | "SEQUENCE"
                    | "VIEW"
                    | "MATERIALIZED VIEW"
                    | "TRIGGER"
                    | "FUNCTION"
                    | "PROCEDURE"
                    | "PACKAGE"
                    | "PACKAGE BODY"
                    | "SYNONYM"
                    | "TYPE"
                    | "TYPE BODY"
                    | "TABLE PARTITION"
                    | "TABLE SUBPARTITION"
                    | "INDEX PARTITION"
                    | "INDEX SUBPARTITION"
                    | "LOB"
                    | "LOB PARTITION"
                    | "LOB SUBPARTITION"
            )
        })
        .take(8)
        .map(|object| format!("{}.{} ({})", object.owner, object.name, object.object_type))
        .collect::<Vec<_>>();
    if !unsupported.is_empty() {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle schema contains object types not yet covered by the certified mapper: {}",
            unsupported.join(", ")
        )));
    }
    let inventory = inventory_all
        .iter()
        .copied()
        .filter(|object| object.subobject.is_none())
        .collect::<Vec<_>>();
    let mut inventory_ids = BTreeSet::new();
    let mut inventory_keys = BTreeSet::new();
    let mut inventory_subobject_keys = BTreeSet::new();
    for object in &inventory_all {
        ensure_owner(scope, &object.owner, "inventory object")?;
        if object.status != "VALID" {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle inventory object {}.{} ({}) has non-valid status '{}'",
                object.owner, object.name, object.object_type, object.status
            )));
        }
        if !inventory_ids.insert(object.object_id) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle object id {}",
                object.object_id
            )));
        }
        let partition_subobject = matches!(
            object.object_type.as_str(),
            "TABLE PARTITION"
                | "TABLE SUBPARTITION"
                | "INDEX PARTITION"
                | "INDEX SUBPARTITION"
                | "LOB PARTITION"
                | "LOB SUBPARTITION"
        );
        if partition_subobject != object.subobject.is_some() {
            return Err(CatalogError::Mapping(format!(
                "Oracle inventory subobject identity is inconsistent for {}.{} ({})",
                object.owner, object.name, object.object_type
            )));
        }
        match object.subobject.as_deref() {
            Some(subobject) => {
                let identity = (
                    object.owner.clone(),
                    object.object_type.clone(),
                    object.name.clone(),
                    subobject.to_owned(),
                );
                if !inventory_subobject_keys.insert(identity.clone()) {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate Oracle subobject inventory identity {}.{} ({}, {})",
                        identity.0, identity.2, identity.1, identity.3
                    )));
                }
            }
            None => {
                let identity = (
                    object.owner.clone(),
                    object.object_type.clone(),
                    object.name.clone(),
                );
                if !inventory_keys.insert(identity.clone()) {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate Oracle inventory identity {}.{} ({})",
                        identity.0, identity.2, identity.1
                    )));
                }
            }
        }
    }

    let mut tables = BTreeSet::new();
    for table in &raw.tables {
        ensure_owner(scope, &table.owner, "table")?;
        if !tables.insert((table.owner.clone(), table.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle table {}.{}",
                table.owner, table.name
            )));
        }
        if table.iot_type.is_some() || table.nested || table.external {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle table shape is not yet covered for {}.{} (partitioned={}, iot_type={}, nested={}, external={})",
                table.owner,
                table.name,
                table.partitioned,
                table.iot_type.as_deref().unwrap_or("none"),
                table.nested,
                table.external
            )));
        }
        if !inventory_keys.contains(&(table.owner.clone(), "TABLE".to_owned(), table.name.clone()))
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle table {}.{} is missing from the independent object inventory",
                table.owner, table.name
            )));
        }
    }
    let inventory_table_count = inventory
        .iter()
        .filter(|object| object.object_type == "TABLE")
        .count();
    if inventory_table_count != raw.tables.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle table inventory mismatch: USER/DBA_OBJECTS reports {inventory_table_count}, USER/DBA_TABLES reports {}",
            raw.tables.len()
        )));
    }

    let mut column_keys = BTreeSet::new();
    let mut column_ordinals = BTreeSet::new();
    for column in &raw.columns {
        ensure_owner(scope, &column.owner, "column")?;
        if !tables.contains(&(column.owner.clone(), column.table.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle column {}.{}.{} has no mapped table",
                column.owner, column.table, column.name
            )));
        }
        positive_u32(column.internal_column_id, "Oracle internal column ordinal")?;
        if !column_keys.insert((
            column.owner.clone(),
            column.table.clone(),
            column.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle column {}.{}.{}",
                column.owner, column.table, column.name
            )));
        }
        if !column_ordinals.insert((
            column.owner.clone(),
            column.table.clone(),
            column.internal_column_id,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle internal column ordinal {} for {}.{}",
                column.internal_column_id, column.owner, column.table
            )));
        }
    }

    let mut sequences = BTreeSet::new();
    for sequence in &raw.sequences {
        ensure_owner(scope, &sequence.owner, "sequence")?;
        if sequence.increment_by.trim().is_empty() || sequence.cache_size.trim().is_empty() {
            return Err(CatalogError::Mapping(format!(
                "Oracle sequence {}.{} has incomplete numeric metadata",
                sequence.owner, sequence.name
            )));
        }
        if !sequences.insert((sequence.owner.clone(), sequence.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle sequence {}.{}",
                sequence.owner, sequence.name
            )));
        }
        if !inventory_keys.contains(&(
            sequence.owner.clone(),
            "SEQUENCE".to_owned(),
            sequence.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle sequence {}.{} is missing from the independent object inventory",
                sequence.owner, sequence.name
            )));
        }
    }
    let inventory_sequence_count = inventory
        .iter()
        .filter(|object| object.object_type == "SEQUENCE")
        .count();
    if inventory_sequence_count != raw.sequences.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle sequence inventory mismatch: USER/DBA_OBJECTS reports {inventory_sequence_count}, USER/DBA_SEQUENCES reports {}",
            raw.sequences.len()
        )));
    }

    let identity_columns = raw
        .columns
        .iter()
        .filter(|column| column.identity)
        .map(|column| {
            (
                column.owner.clone(),
                column.table.clone(),
                column.name.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut identity_details = BTreeSet::new();
    for identity in &raw.identity_columns {
        ensure_owner(scope, &identity.owner, "identity column")?;
        let key = (
            identity.owner.clone(),
            identity.table.clone(),
            identity.column.clone(),
        );
        if !identity_details.insert(key.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle identity metadata for {}.{}.{}",
                identity.owner, identity.table, identity.column
            )));
        }
        if !identity_columns.contains(&key) {
            return Err(CatalogError::Mapping(format!(
                "Oracle identity catalog references a non-identity column {}.{}.{}",
                identity.owner, identity.table, identity.column
            )));
        }
        if !sequences.contains(&(identity.owner.clone(), identity.sequence_name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle identity column {}.{}.{} references missing sequence {}.{}",
                identity.owner,
                identity.table,
                identity.column,
                identity.owner,
                identity.sequence_name
            )));
        }
    }
    if identity_columns != identity_details {
        return match identity_columns.difference(&identity_details).next() {
            Some(missing) => Err(CatalogError::Mapping(format!(
                "Oracle identity column {}.{}.{} is missing *_TAB_IDENTITY_COLS metadata",
                missing.0, missing.1, missing.2
            ))),
            None => Err(CatalogError::Mapping(
                "Oracle identity-column catalogs disagree".to_owned(),
            )),
        };
    }
    for table in &raw.tables {
        let discovered_identity = identity_columns
            .iter()
            .any(|(owner, name, _)| owner == &table.owner && name == &table.name);
        if table.has_identity != discovered_identity {
            return Err(CatalogError::Mapping(format!(
                "Oracle table identity flag mismatch for {}.{}",
                table.owner, table.name
            )));
        }
    }

    let mut views = BTreeSet::new();
    for view in &raw.views {
        ensure_owner(scope, &view.owner, "view")?;
        if !views.insert((view.owner.clone(), view.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle view {}.{}",
                view.owner, view.name
            )));
        }
        if view.definition.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle view {}.{} has no complete definition",
                view.owner, view.name
            )));
        }
        if view.type_owner.is_some() || view.view_type.is_some() || view.superview.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "typed Oracle view metadata is not yet covered for {}.{}",
                view.owner, view.name
            )));
        }
        if !inventory_keys.contains(&(view.owner.clone(), "VIEW".to_owned(), view.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle view {}.{} is missing from the independent object inventory",
                view.owner, view.name
            )));
        }
    }
    let inventory_view_count = inventory
        .iter()
        .filter(|object| object.object_type == "VIEW")
        .count();
    if inventory_view_count != raw.views.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle view inventory mismatch: USER/DBA_OBJECTS reports {inventory_view_count}, USER/DBA_VIEWS reports {}",
            raw.views.len()
        )));
    }

    let mut materialized_views = BTreeSet::new();
    for view in &raw.materialized_views {
        ensure_owner(scope, &view.owner, "materialized view")?;
        if !materialized_views.insert((view.owner.clone(), view.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle materialized view {}.{}",
                view.owner, view.name
            )));
        }
        if view.definition.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle materialized view {}.{} has no complete definition",
                view.owner, view.name
            )));
        }
        if view.master_link.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle materialized view {}.{} uses remote master link '{}'",
                view.owner,
                view.name,
                view.master_link.as_deref().unwrap_or_default()
            )));
        }
        if view.compile_state.as_deref() != Some("VALID") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle materialized view {}.{} has compile state '{}'",
                view.owner,
                view.name,
                view.compile_state.as_deref().unwrap_or("missing")
            )));
        }
        if view.container_name != view.name {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle materialized view {}.{} uses non-default container table '{}'",
                view.owner, view.name, view.container_name
            )));
        }
        if !tables.contains(&(view.owner.clone(), view.container_name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle materialized view {}.{} has no storage table {}.{}",
                view.owner, view.name, view.owner, view.container_name
            )));
        }
        for object_type in ["MATERIALIZED VIEW", "TABLE"] {
            if !inventory_keys.contains(&(
                view.owner.clone(),
                object_type.to_owned(),
                view.name.clone(),
            )) {
                return Err(CatalogError::Mapping(format!(
                    "Oracle materialized view {}.{} is missing its {object_type} inventory row",
                    view.owner, view.name
                )));
            }
        }
    }
    let inventory_mview_count = inventory
        .iter()
        .filter(|object| object.object_type == "MATERIALIZED VIEW")
        .count();
    if inventory_mview_count != raw.materialized_views.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle materialized-view inventory mismatch: USER/DBA_OBJECTS reports {inventory_mview_count}, USER/DBA_MVIEWS reports {}",
            raw.materialized_views.len()
        )));
    }

    let mut synonyms = BTreeSet::new();
    for synonym in &raw.synonyms {
        ensure_owner(scope, &synonym.owner, "synonym")?;
        ensure_reference_owner(
            scope,
            &synonym.target_owner,
            &format!("synonym {}.{}", synonym.owner, synonym.name),
        )?;
        if synonym.database_link.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle synonym {}.{} uses remote database link '{}'",
                synonym.owner,
                synonym.name,
                synonym.database_link.as_deref().unwrap_or_default()
            )));
        }
        if synonym.origin_container_id < 0 {
            return Err(CatalogError::Mapping(format!(
                "Oracle synonym {}.{} has invalid origin container id {}",
                synonym.owner, synonym.name, synonym.origin_container_id
            )));
        }
        if !synonyms.insert((synonym.owner.clone(), synonym.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle synonym {}.{}",
                synonym.owner, synonym.name
            )));
        }
        if !inventory_keys.contains(&(
            synonym.owner.clone(),
            "SYNONYM".to_owned(),
            synonym.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle synonym {}.{} is missing from the independent object inventory",
                synonym.owner, synonym.name
            )));
        }
    }
    let inventory_synonym_count = inventory
        .iter()
        .filter(|object| object.object_type == "SYNONYM")
        .count();
    if inventory_synonym_count != raw.synonyms.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle synonym inventory mismatch: USER/DBA_OBJECTS reports {inventory_synonym_count}, USER/DBA_SYNONYMS reports {}",
            raw.synonyms.len()
        )));
    }

    let mut user_types = BTreeMap::new();
    let mut type_oids = BTreeSet::new();
    for user_type in &raw.user_types {
        ensure_owner(scope, &user_type.owner, "type")?;
        if !matches!(user_type.typecode.as_str(), "OBJECT" | "COLLECTION") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle type {}.{} has unsupported typecode '{}'",
                user_type.owner, user_type.name, user_type.typecode
            )));
        }
        for (name, value) in [
            ("predefined", user_type.predefined.as_str()),
            ("incomplete", user_type.incomplete.as_str()),
            ("final", user_type.final_type.as_str()),
            ("instantiable", user_type.instantiable.as_str()),
            ("persistable", user_type.persistable.as_str()),
        ] {
            ensure_yes_no(
                value,
                &format!("Oracle type {}.{} {name}", user_type.owner, user_type.name),
            )?;
        }
        if user_type.predefined != "NO" || user_type.incomplete != "NO" {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle type {}.{} is predefined or incomplete",
                user_type.owner, user_type.name
            )));
        }
        if user_type.attribute_count < 0
            || user_type.method_count < 0
            || user_type
                .local_attribute_count
                .is_some_and(|count| count < 0)
            || user_type.local_method_count.is_some_and(|count| count < 0)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle type {}.{} has negative member counts",
                user_type.owner, user_type.name
            )));
        }
        if user_type.oid.is_empty() || !type_oids.insert(user_type.oid.clone()) {
            return Err(CatalogError::Mapping(format!(
                "Oracle type {}.{} has a missing or duplicate OID",
                user_type.owner, user_type.name
            )));
        }
        if user_type.specification.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle type {}.{} has no complete specification",
                user_type.owner, user_type.name
            )));
        }
        if let Some(body) = user_type.body.as_deref() {
            reject_dynamic_plsql(
                "type body",
                &format!("{}.{}", user_type.owner, user_type.name),
                body,
            )?;
        }
        let identity = (user_type.owner.clone(), user_type.name.clone());
        if user_types.insert(identity, user_type).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle type {}.{}",
                user_type.owner, user_type.name
            )));
        }
        if !inventory_keys.contains(&(
            user_type.owner.clone(),
            "TYPE".to_owned(),
            user_type.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle type {}.{} is missing from the independent object inventory",
                user_type.owner, user_type.name
            )));
        }
        let has_body_inventory = inventory_keys.contains(&(
            user_type.owner.clone(),
            "TYPE BODY".to_owned(),
            user_type.name.clone(),
        ));
        if has_body_inventory != user_type.body.is_some() {
            return Err(CatalogError::Mapping(format!(
                "Oracle type body inventory mismatch for {}.{}",
                user_type.owner, user_type.name
            )));
        }
    }
    let inventory_type_count = inventory
        .iter()
        .filter(|object| object.object_type == "TYPE")
        .count();
    let inventory_type_body_count = inventory
        .iter()
        .filter(|object| object.object_type == "TYPE BODY")
        .count();
    if inventory_type_count != raw.user_types.len()
        || inventory_type_body_count
            != raw
                .user_types
                .iter()
                .filter(|user_type| user_type.body.is_some())
                .count()
    {
        return Err(CatalogError::Mapping(format!(
            "Oracle type inventory mismatch: TYPE={inventory_type_count}, TYPE BODY={inventory_type_body_count}"
        )));
    }
    for user_type in &raw.user_types {
        match (
            user_type.supertype_owner.as_deref(),
            user_type.supertype_name.as_deref(),
        ) {
            (Some(owner), Some(name)) => {
                ensure_reference_owner(
                    scope,
                    owner,
                    &format!("type {}.{}", user_type.owner, user_type.name),
                )?;
                if !user_types.contains_key(&(owner.to_owned(), name.to_owned()))
                    || (owner == user_type.owner && name == user_type.name)
                    || user_type.local_attribute_count.is_none()
                    || user_type.local_method_count.is_none()
                {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle type {}.{} has inconsistent supertype metadata",
                        user_type.owner, user_type.name
                    )));
                }
            }
            (None, None) => {}
            _ => {
                return Err(CatalogError::Mapping(format!(
                    "Oracle type {}.{} has a partial supertype identity",
                    user_type.owner, user_type.name
                )));
            }
        }
    }

    let mut attribute_identities = BTreeSet::new();
    let mut attributes_by_type = BTreeMap::<(String, String), Vec<&RawTypeAttribute>>::new();
    for attribute in &raw.type_attributes {
        ensure_owner(scope, &attribute.owner, "type attribute")?;
        if !user_types.contains_key(&(attribute.owner.clone(), attribute.type_name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle type attribute {}.{}.{} has no parent type",
                attribute.owner, attribute.type_name, attribute.name
            )));
        }
        positive_u32(attribute.position, "Oracle type attribute position")?;
        ensure_yes_no(
            &attribute.inherited,
            &format!(
                "Oracle type attribute {}.{}.{} inherited",
                attribute.owner, attribute.type_name, attribute.name
            ),
        )?;
        ensure_user_type_reference(
            scope,
            &user_types,
            attribute.data_type_owner.as_deref(),
            &attribute.data_type_name,
            &format!(
                "Oracle type attribute {}.{}.{}",
                attribute.owner, attribute.type_name, attribute.name
            ),
        )?;
        if !attribute_identities.insert((
            attribute.owner.clone(),
            attribute.type_name.clone(),
            attribute.position,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle type attribute position {} for {}.{}",
                attribute.position, attribute.owner, attribute.type_name
            )));
        }
        attributes_by_type
            .entry((attribute.owner.clone(), attribute.type_name.clone()))
            .or_default()
            .push(attribute);
    }
    for user_type in &raw.user_types {
        let attributes = attributes_by_type
            .get(&(user_type.owner.clone(), user_type.name.clone()))
            .map(Vec::as_slice)
            .unwrap_or_default();
        if attributes.len() != user_type.attribute_count as usize
            || attributes
                .iter()
                .enumerate()
                .any(|(offset, attribute)| attribute.position != (offset + 1) as i64)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle type attribute catalog mismatch for {}.{}",
                user_type.owner, user_type.name
            )));
        }
    }

    let mut collection_names = BTreeSet::new();
    for collection in &raw.collection_types {
        ensure_owner(scope, &collection.owner, "collection type")?;
        let parent = user_types
            .get(&(collection.owner.clone(), collection.type_name.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle collection {}.{} has no parent type",
                    collection.owner, collection.type_name
                ))
            })?;
        if parent.typecode != "COLLECTION"
            || !matches!(
                collection.collection_type.as_str(),
                "TABLE" | "VARYING ARRAY"
            )
            || (collection.collection_type == "VARYING ARRAY" && collection.upper_bound.is_none())
            || (collection.collection_type == "TABLE" && collection.upper_bound.is_some())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle collection metadata is inconsistent for {}.{}",
                collection.owner, collection.type_name
            )));
        }
        ensure_user_type_reference(
            scope,
            &user_types,
            collection.element_type_owner.as_deref(),
            &collection.element_type_name,
            &format!(
                "Oracle collection {}.{}",
                collection.owner, collection.type_name
            ),
        )?;
        if !collection_names.insert((collection.owner.clone(), collection.type_name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle collection type {}.{}",
                collection.owner, collection.type_name
            )));
        }
    }
    let expected_collection_names = raw
        .user_types
        .iter()
        .filter(|user_type| user_type.typecode == "COLLECTION")
        .map(|user_type| (user_type.owner.clone(), user_type.name.clone()))
        .collect::<BTreeSet<_>>();
    if collection_names != expected_collection_names {
        return Err(CatalogError::Mapping(
            "Oracle USER/DBA_COLL_TYPES does not exactly match collection TYPE rows".to_owned(),
        ));
    }

    let mut method_identities = BTreeSet::new();
    let mut methods_by_type = BTreeMap::<(String, String), Vec<&RawTypeMethod>>::new();
    for method in &raw.type_methods {
        ensure_owner(scope, &method.owner, "type method")?;
        let parent = user_types
            .get(&(method.owner.clone(), method.type_name.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle type method {}.{}.{} has no parent type",
                    method.owner, method.type_name, method.name
                ))
            })?;
        if parent.typecode != "OBJECT" || method.parameter_count < 0 || method.result_count < 0 {
            return Err(CatalogError::Mapping(format!(
                "Oracle type method metadata is malformed for {}.{}.{}",
                method.owner, method.type_name, method.name
            )));
        }
        positive_u32(method.method_number, "Oracle type method number")?;
        for (name, value) in [
            ("final", method.final_method.as_str()),
            ("instantiable", method.instantiable.as_str()),
            ("overriding", method.overriding.as_str()),
            ("inherited", method.inherited.as_str()),
        ] {
            ensure_yes_no(
                value,
                &format!(
                    "Oracle type method {}.{}.{} {name}",
                    method.owner, method.type_name, method.name
                ),
            )?;
        }
        if !method_identities.insert((
            method.owner.clone(),
            method.type_name.clone(),
            method.method_number,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle type method number {} for {}.{}",
                method.method_number, method.owner, method.type_name
            )));
        }
        methods_by_type
            .entry((method.owner.clone(), method.type_name.clone()))
            .or_default()
            .push(method);
    }
    for user_type in &raw.user_types {
        let method_count = methods_by_type
            .get(&(user_type.owner.clone(), user_type.name.clone()))
            .map_or(0, Vec::len);
        if method_count != user_type.method_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle type method catalog mismatch for {}.{}",
                user_type.owner, user_type.name
            )));
        }
    }

    let mut method_parameter_identities = BTreeSet::new();
    let mut parameters_by_method =
        BTreeMap::<(String, String, i64), Vec<&RawTypeMethodParameter>>::new();
    for parameter in &raw.type_method_parameters {
        ensure_owner(scope, &parameter.owner, "type method parameter")?;
        let method_key = (
            parameter.owner.clone(),
            parameter.type_name.clone(),
            parameter.method_number,
        );
        let method = raw
            .type_methods
            .iter()
            .find(|method| {
                method.owner == parameter.owner
                    && method.type_name == parameter.type_name
                    && method.method_number == parameter.method_number
            })
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle type method parameter {}.{}.{} has no method",
                    parameter.owner, parameter.type_name, parameter.name
                ))
            })?;
        if parameter.method_name != method.name
            || parameter.position < 0
            || !matches!(parameter.mode.as_str(), "IN" | "OUT" | "IN/OUT")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle type method parameter metadata is malformed for {}.{}.{}",
                parameter.owner, parameter.type_name, parameter.name
            )));
        }
        ensure_user_type_reference(
            scope,
            &user_types,
            parameter.data_type_owner.as_deref(),
            &parameter.data_type_name,
            &format!(
                "Oracle type method parameter {}.{}.{}",
                parameter.owner, parameter.type_name, parameter.name
            ),
        )?;
        if !method_parameter_identities.insert((
            method_key.clone(),
            parameter.return_value,
            parameter.position,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle type method parameter position {} for {}.{}",
                parameter.position, parameter.owner, parameter.type_name
            )));
        }
        parameters_by_method
            .entry(method_key)
            .or_default()
            .push(parameter);
    }
    for method in &raw.type_methods {
        let parameters = parameters_by_method
            .get(&(
                method.owner.clone(),
                method.type_name.clone(),
                method.method_number,
            ))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let parameter_count = parameters
            .iter()
            .filter(|parameter| !parameter.return_value)
            .count();
        let result_count = parameters
            .iter()
            .filter(|parameter| parameter.return_value)
            .count();
        if parameter_count != method.parameter_count as usize
            || result_count != method.result_count as usize
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle type method parameter catalog mismatch for {}.{}.{}",
                method.owner, method.type_name, method.name
            )));
        }
    }

    for column in raw.columns.iter().chain(&raw.view_columns) {
        ensure_user_type_reference(
            scope,
            &user_types,
            column.data_type_owner.as_deref(),
            &column.data_type,
            &format!(
                "Oracle column {}.{}.{}",
                column.owner, column.table, column.name
            ),
        )?;
    }

    let mut triggers = BTreeSet::new();
    for trigger in &raw.triggers {
        ensure_owner(scope, &trigger.owner, "trigger")?;
        if !triggers.insert((trigger.owner.clone(), trigger.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle trigger {}.{}",
                trigger.owner, trigger.name
            )));
        }
        if !inventory_keys.contains(&(
            trigger.owner.clone(),
            "TRIGGER".to_owned(),
            trigger.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle trigger {}.{} is missing from the independent object inventory",
                trigger.owner, trigger.name
            )));
        }
        match trigger.base_object_type.as_str() {
            "TABLE" | "VIEW" => {
                let target_owner = trigger.table_owner.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle {} trigger {}.{} has no target owner",
                        trigger.base_object_type.to_lowercase(),
                        trigger.owner,
                        trigger.name
                    ))
                })?;
                let target_name = trigger.table_name.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle {} trigger {}.{} has no target object",
                        trigger.base_object_type.to_lowercase(),
                        trigger.owner,
                        trigger.name
                    ))
                })?;
                ensure_owner(scope, target_owner, "trigger target")?;
                if trigger.owner != target_owner {
                    return Err(CatalogError::UnsupportedMetadata(format!(
                        "cross-owner Oracle trigger {}.{} on {}.{target_name} is outside the certified contract",
                        trigger.owner, trigger.name, target_owner
                    )));
                }
                let target_exists = if trigger.base_object_type == "TABLE" {
                    tables.contains(&(target_owner.to_owned(), target_name.to_owned()))
                        && !materialized_views
                            .contains(&(target_owner.to_owned(), target_name.to_owned()))
                } else {
                    views.contains(&(target_owner.to_owned(), target_name.to_owned()))
                };
                if !target_exists {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle trigger {}.{} targets missing {} {}.{}",
                        trigger.owner,
                        trigger.name,
                        trigger.base_object_type.to_lowercase(),
                        target_owner,
                        target_name
                    )));
                }
            }
            "SCHEMA" => {
                let target_owner = trigger.table_owner.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle schema trigger {}.{} has no target owner",
                        trigger.owner, trigger.name
                    ))
                })?;
                ensure_owner(scope, target_owner, "schema trigger target")?;
                if trigger.owner != target_owner || trigger.table_name.is_some() {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle schema trigger {}.{} has inconsistent target metadata",
                        trigger.owner, trigger.name
                    )));
                }
            }
            "DATABASE" => {
                if trigger.table_name.is_some() {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle database trigger {}.{} unexpectedly names a table target",
                        trigger.owner, trigger.name
                    )));
                }
            }
            other => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle trigger target kind '{other}' is not covered for {}.{}",
                    trigger.owner, trigger.name
                )));
            }
        }
        if !matches!(trigger.action_type.as_str(), "PL/SQL" | "CALL") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle trigger action type '{}' is not covered for {}.{}",
                trigger.action_type, trigger.owner, trigger.name
            )));
        }
        if !matches!(trigger.status.as_str(), "ENABLED" | "DISABLED") {
            return Err(CatalogError::Mapping(format!(
                "Oracle trigger {}.{} has unrecognized status '{}'",
                trigger.owner, trigger.name, trigger.status
            )));
        }
        let body = trigger.body.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle trigger {}.{} has no complete body",
                trigger.owner, trigger.name
            ))
        })?;
        reject_dynamic_plsql(
            "trigger",
            &format!("{}.{}", trigger.owner, trigger.name),
            body,
        )?;
        oracle_trigger_timing(&trigger.trigger_type)?;
    }
    let inventory_trigger_count = inventory
        .iter()
        .filter(|object| object.object_type == "TRIGGER")
        .count();
    if inventory_trigger_count != raw.triggers.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle trigger inventory mismatch: USER/DBA_OBJECTS reports {inventory_trigger_count}, USER/DBA_TRIGGERS reports {}",
            raw.triggers.len()
        )));
    }

    let mut routines = BTreeMap::new();
    let mut routines_by_name = BTreeMap::new();
    for routine in &raw.routines {
        ensure_owner(scope, &routine.owner, "routine")?;
        if !matches!(routine.object_type.as_str(), "FUNCTION" | "PROCEDURE") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle routine type '{}' is not covered for {}.{}",
                routine.object_type, routine.owner, routine.name
            )));
        }
        let identity = (
            routine.owner.clone(),
            routine.name.clone(),
            routine.object_type.clone(),
        );
        if routines.insert(identity, routine).is_some()
            || routines_by_name
                .insert((routine.owner.clone(), routine.name.clone()), routine)
                .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle standalone routine {}.{}",
                routine.owner, routine.name
            )));
        }
        let inventory_object = inventory
            .iter()
            .find(|object| {
                object.owner == routine.owner
                    && object.object_type == routine.object_type
                    && object.name == routine.name
            })
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle routine {}.{} is missing from the independent object inventory",
                    routine.owner, routine.name
                ))
            })?;
        if inventory_object.object_id != routine.object_id {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine object id mismatch for {}.{}: inventory={}, procedure catalog={}",
                routine.owner, routine.name, inventory_object.object_id, routine.object_id
            )));
        }
        if routine.subprogram_id != 1 || routine.overload.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle standalone routine {}.{} has unexpected overload identity subprogram_id={} overload='{}'",
                routine.owner,
                routine.name,
                routine.subprogram_id,
                routine.overload.as_deref().unwrap_or("none")
            )));
        }
        if routine.aggregate
            || routine.pipelined
            || routine.interface
            || routine.polymorphic.is_some()
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle routine shape is not yet covered for {}.{} (aggregate={}, pipelined={}, interface={}, polymorphic={})",
                routine.owner,
                routine.name,
                routine.aggregate,
                routine.pipelined,
                routine.interface,
                routine.polymorphic.as_deref().unwrap_or("none")
            )));
        }
        if !matches!(routine.authid.as_str(), "DEFINER" | "CURRENT_USER") {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine {}.{} has unrecognized AUTHID '{}'",
                routine.owner, routine.name, routine.authid
            )));
        }
        let definition = routine.definition.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle routine {}.{} has no complete definition",
                routine.owner, routine.name
            ))
        })?;
        reject_dynamic_plsql(
            "routine",
            &format!("{}.{}", routine.owner, routine.name),
            definition,
        )?;
    }
    let inventory_routine_count = inventory
        .iter()
        .filter(|object| matches!(object.object_type.as_str(), "FUNCTION" | "PROCEDURE"))
        .count();
    if inventory_routine_count != raw.routines.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle routine inventory mismatch: USER/DBA_OBJECTS reports {inventory_routine_count}, USER/DBA_PROCEDURES reports {} standalone routine(s)",
            raw.routines.len()
        )));
    }

    let mut argument_identities = BTreeSet::new();
    let mut arguments_by_routine = BTreeMap::<(String, String), Vec<&RawRoutineArgument>>::new();
    for argument in &raw.routine_arguments {
        ensure_owner(scope, &argument.owner, "routine argument")?;
        if argument.package_name.is_some() {
            return Err(CatalogError::Mapping(format!(
                "standalone Oracle argument {}.{} unexpectedly belongs to package '{}'",
                argument.owner,
                argument.routine,
                argument.package_name.as_deref().unwrap_or_default()
            )));
        }
        let routine = routines_by_name
            .get(&(argument.owner.clone(), argument.routine.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle argument references missing standalone routine {}.{}",
                    argument.owner, argument.routine
                ))
            })?;
        if argument.subprogram_id != routine.subprogram_id || argument.overload != routine.overload
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle argument overload identity does not match routine {}.{}",
                argument.owner, argument.routine
            )));
        }
        if argument.data_level != 0 || argument.type_subname.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle nested or package-defined routine argument is not covered for {}.{} position {}",
                argument.owner, argument.routine, argument.position
            )));
        }
        match (
            argument.type_owner.as_deref(),
            argument.type_name.as_deref(),
        ) {
            (Some(owner), Some(name)) => ensure_user_type_reference(
                scope,
                &user_types,
                Some(owner),
                name,
                &format!(
                    "Oracle routine argument {}.{} position {}",
                    argument.owner, argument.routine, argument.position
                ),
            )?,
            (None, None) => {}
            _ => {
                return Err(CatalogError::Mapping(format!(
                    "Oracle routine argument {}.{} position {} has a partial type identity",
                    argument.owner, argument.routine, argument.position
                )));
            }
        }
        if argument.data_type.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine argument {}.{} position {} has no data type",
                argument.owner, argument.routine, argument.position
            )));
        }
        if !matches!(argument.mode.as_str(), "IN" | "OUT" | "IN/OUT") {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine argument {}.{} position {} has unrecognized mode '{}'",
                argument.owner, argument.routine, argument.position, argument.mode
            )));
        }
        positive_u32(argument.sequence, "Oracle routine argument sequence")?;
        if argument.position < 0 {
            return Err(CatalogError::Mapping(format!(
                "Oracle routine argument {}.{} has negative position {}",
                argument.owner, argument.routine, argument.position
            )));
        }
        if !argument_identities.insert((
            argument.owner.clone(),
            argument.routine.clone(),
            argument.subprogram_id,
            argument.sequence,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle routine argument sequence {} for {}.{}",
                argument.sequence, argument.owner, argument.routine
            )));
        }
        arguments_by_routine
            .entry((argument.owner.clone(), argument.routine.clone()))
            .or_default()
            .push(argument);
    }
    for routine in &raw.routines {
        let arguments = arguments_by_routine
            .get(&(routine.owner.clone(), routine.name.clone()))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let return_count = arguments
            .iter()
            .filter(|argument| argument.position == 0)
            .count();
        let expected_return_count = usize::from(routine.object_type == "FUNCTION");
        if return_count != expected_return_count {
            return Err(CatalogError::Mapping(format!(
                "Oracle {} {}.{} has {return_count} return rows; expected {expected_return_count}",
                routine.object_type, routine.owner, routine.name
            )));
        }
        for (offset, argument) in arguments.iter().enumerate() {
            let expected_sequence = i64::try_from(offset + 1).map_err(|_| {
                CatalogError::Mapping("too many Oracle routine arguments".to_owned())
            })?;
            if argument.sequence != expected_sequence {
                return Err(CatalogError::Mapping(format!(
                    "Oracle routine argument sequence gap for {}.{}: expected {expected_sequence}, found {}",
                    routine.owner, routine.name, argument.sequence
                )));
            }
            let expected_position = if routine.object_type == "FUNCTION" {
                i64::try_from(offset).map_err(|_| {
                    CatalogError::Mapping("too many Oracle routine arguments".to_owned())
                })?
            } else {
                expected_sequence
            };
            if argument.position != expected_position {
                return Err(CatalogError::Mapping(format!(
                    "Oracle routine argument position mismatch for {}.{}: expected {expected_position}, found {}",
                    routine.owner, routine.name, argument.position
                )));
            }
            if argument.position == 0 && (argument.name.is_some() || argument.mode != "OUT") {
                return Err(CatalogError::Mapping(format!(
                    "Oracle function return metadata is malformed for {}.{}",
                    routine.owner, routine.name
                )));
            }
        }
    }

    let mut packages = BTreeMap::new();
    for package in &raw.packages {
        ensure_owner(scope, &package.owner, "package")?;
        if packages
            .insert((package.owner.clone(), package.name.clone()), package)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle package {}.{}",
                package.owner, package.name
            )));
        }
        if !matches!(package.authid.as_str(), "DEFINER" | "CURRENT_USER") {
            return Err(CatalogError::Mapping(format!(
                "Oracle package {}.{} has unrecognized AUTHID '{}'",
                package.owner, package.name, package.authid
            )));
        }
        if package.specification.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle package {}.{} has no complete specification",
                package.owner, package.name
            )));
        }
        if let Some(body) = package.body.as_deref() {
            reject_dynamic_plsql(
                "package body",
                &format!("{}.{}", package.owner, package.name),
                body,
            )?;
        }
        let inventory_object = inventory
            .iter()
            .find(|object| {
                object.owner == package.owner
                    && object.object_type == "PACKAGE"
                    && object.name == package.name
            })
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle package {}.{} is missing from the independent object inventory",
                    package.owner, package.name
                ))
            })?;
        if inventory_object.object_id != package.object_id {
            return Err(CatalogError::Mapping(format!(
                "Oracle package object id mismatch for {}.{}: inventory={}, procedure catalog={}",
                package.owner, package.name, inventory_object.object_id, package.object_id
            )));
        }
        let body_in_inventory = inventory_keys.contains(&(
            package.owner.clone(),
            "PACKAGE BODY".to_owned(),
            package.name.clone(),
        ));
        if body_in_inventory != package.body.is_some() {
            return Err(CatalogError::Mapping(format!(
                "Oracle package body inventory/source mismatch for {}.{}",
                package.owner, package.name
            )));
        }
    }
    let inventory_package_count = inventory
        .iter()
        .filter(|object| object.object_type == "PACKAGE")
        .count();
    let inventory_package_body_count = inventory
        .iter()
        .filter(|object| object.object_type == "PACKAGE BODY")
        .count();
    if inventory_package_count != raw.packages.len()
        || inventory_package_body_count
            != raw
                .packages
                .iter()
                .filter(|package| package.body.is_some())
                .count()
    {
        return Err(CatalogError::Mapping(format!(
            "Oracle package inventory mismatch: packages={inventory_package_count}/{}, bodies={inventory_package_body_count}/{}",
            raw.packages.len(),
            raw.packages
                .iter()
                .filter(|package| package.body.is_some())
                .count()
        )));
    }

    let mut package_routines = BTreeMap::new();
    for routine in &raw.package_routines {
        ensure_owner(scope, &routine.owner, "package routine")?;
        let package = packages
            .get(&(routine.owner.clone(), routine.package.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle package routine {}.{}.{} has no package",
                    routine.owner, routine.package, routine.name
                ))
            })?;
        if routine.object_id != package.object_id || routine.subprogram_id <= 0 {
            return Err(CatalogError::Mapping(format!(
                "Oracle package routine identity is malformed for {}.{}.{}",
                routine.owner, routine.package, routine.name
            )));
        }
        if routine.aggregate
            || routine.pipelined
            || routine.interface
            || routine.polymorphic.is_some()
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle package routine shape is not yet covered for {}.{}.{} (aggregate={}, pipelined={}, interface={}, polymorphic={})",
                routine.owner,
                routine.package,
                routine.name,
                routine.aggregate,
                routine.pipelined,
                routine.interface,
                routine.polymorphic.as_deref().unwrap_or("none")
            )));
        }
        if routine.authid != package.authid {
            return Err(CatalogError::Mapping(format!(
                "Oracle package routine AUTHID mismatch for {}.{}.{}",
                routine.owner, routine.package, routine.name
            )));
        }
        if package_routines
            .insert(
                (
                    routine.owner.clone(),
                    routine.package.clone(),
                    routine.subprogram_id,
                ),
                routine,
            )
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle package subprogram id {} for {}.{}",
                routine.subprogram_id, routine.owner, routine.package
            )));
        }
    }

    let mut package_argument_identities = BTreeSet::new();
    let mut package_arguments_by_routine =
        BTreeMap::<(String, String, i64), Vec<&RawRoutineArgument>>::new();
    for argument in &raw.package_arguments {
        ensure_owner(scope, &argument.owner, "package argument")?;
        let package_name = argument.package_name.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle package argument {}.{} has no package name",
                argument.owner, argument.routine
            ))
        })?;
        let routine = package_routines
            .get(&(
                argument.owner.clone(),
                package_name.to_owned(),
                argument.subprogram_id,
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle package argument references missing routine {}.{}.{}",
                    argument.owner, package_name, argument.routine
                ))
            })?;
        if argument.routine != routine.name || argument.overload != routine.overload {
            return Err(CatalogError::Mapping(format!(
                "Oracle package argument overload identity does not match {}.{}.{}",
                argument.owner, package_name, argument.routine
            )));
        }
        if argument.data_level != 0 || argument.type_subname.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle nested or package-defined package argument is not covered for {}.{}.{} position {}",
                argument.owner, package_name, argument.routine, argument.position
            )));
        }
        match (
            argument.type_owner.as_deref(),
            argument.type_name.as_deref(),
        ) {
            (Some(owner), Some(name)) => ensure_user_type_reference(
                scope,
                &user_types,
                Some(owner),
                name,
                &format!(
                    "Oracle package argument {}.{}.{} position {}",
                    argument.owner, package_name, argument.routine, argument.position
                ),
            )?,
            (None, None) => {}
            _ => {
                return Err(CatalogError::Mapping(format!(
                    "Oracle package argument {}.{}.{} position {} has a partial type identity",
                    argument.owner, package_name, argument.routine, argument.position
                )));
            }
        }
        if argument.data_type.is_none()
            || !matches!(argument.mode.as_str(), "IN" | "OUT" | "IN/OUT")
            || argument.position < 0
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle package argument metadata is malformed for {}.{}.{} position {}",
                argument.owner, package_name, argument.routine, argument.position
            )));
        }
        positive_u32(argument.sequence, "Oracle package argument sequence")?;
        if !package_argument_identities.insert((
            argument.owner.clone(),
            package_name.to_owned(),
            argument.subprogram_id,
            argument.sequence,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle package argument sequence {} for {}.{}.{}",
                argument.sequence, argument.owner, package_name, argument.routine
            )));
        }
        package_arguments_by_routine
            .entry((
                argument.owner.clone(),
                package_name.to_owned(),
                argument.subprogram_id,
            ))
            .or_default()
            .push(argument);
    }
    let mut package_signatures = BTreeSet::new();
    for routine in &raw.package_routines {
        let arguments = package_arguments_by_routine
            .get(&(
                routine.owner.clone(),
                routine.package.clone(),
                routine.subprogram_id,
            ))
            .map(Vec::as_slice)
            .unwrap_or_default();
        validate_package_argument_order(routine, arguments)?;
        let signature = oracle_package_routine_signature(routine, arguments)?;
        if !package_signatures.insert((
            routine.owner.clone(),
            routine.package.clone(),
            signature.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle package routine signature {}.{}.{signature}",
                routine.owner, routine.package
            )));
        }
    }

    let mut view_column_keys = BTreeSet::new();
    let mut view_column_ordinals = BTreeSet::new();
    for column in &raw.view_columns {
        ensure_owner(scope, &column.owner, "view column")?;
        if !views.contains(&(column.owner.clone(), column.table.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle view column {}.{}.{} has no mapped view",
                column.owner, column.table, column.name
            )));
        }
        positive_u32(column.internal_column_id, "Oracle view-column ordinal")?;
        if !view_column_keys.insert((
            column.owner.clone(),
            column.table.clone(),
            column.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle view column {}.{}.{}",
                column.owner, column.table, column.name
            )));
        }
        if !view_column_ordinals.insert((
            column.owner.clone(),
            column.table.clone(),
            column.internal_column_id,
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle view-column ordinal {} for {}.{}",
                column.internal_column_id, column.owner, column.table
            )));
        }
    }

    for dependency in &raw.dependencies {
        ensure_owner(scope, &dependency.owner, "dependency source")?;
        if dependency.referenced_link.is_some() {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle dependency {}.{} uses remote database link '{}'",
                dependency.owner,
                dependency.name,
                dependency.referenced_link.as_deref().unwrap_or_default()
            )));
        }
        let source_is_view = dependency.object_type == "VIEW"
            && views.contains(&(dependency.owner.clone(), dependency.name.clone()));
        let source_is_mview = dependency.object_type == "MATERIALIZED VIEW"
            && materialized_views.contains(&(dependency.owner.clone(), dependency.name.clone()));
        let source_is_trigger = dependency.object_type == "TRIGGER"
            && triggers.contains(&(dependency.owner.clone(), dependency.name.clone()));
        let source_is_routine = matches!(dependency.object_type.as_str(), "FUNCTION" | "PROCEDURE")
            && routines.contains_key(&(
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.object_type.clone(),
            ));
        let source_is_package =
            matches!(dependency.object_type.as_str(), "PACKAGE" | "PACKAGE BODY")
                && packages.contains_key(&(dependency.owner.clone(), dependency.name.clone()));
        let source_is_synonym = dependency.object_type == "SYNONYM"
            && synonyms.contains(&(dependency.owner.clone(), dependency.name.clone()));
        let source_is_type = matches!(dependency.object_type.as_str(), "TYPE" | "TYPE BODY")
            && user_types.contains_key(&(dependency.owner.clone(), dependency.name.clone()));
        let source_is_table = dependency.object_type == "TABLE"
            && tables.contains(&(dependency.owner.clone(), dependency.name.clone()));
        if !source_is_view
            && !source_is_mview
            && !source_is_trigger
            && !source_is_routine
            && !source_is_package
            && !source_is_synonym
            && !source_is_type
            && !source_is_table
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle dependency source is not yet covered: {}.{} ({})",
                dependency.owner, dependency.name, dependency.object_type
            )));
        }
        let expected_dependency_type = if source_is_mview { "REF" } else { "HARD" };
        if dependency.dependency_type != expected_dependency_type {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle dependency type '{}' is not covered for {}.{}; expected '{}'",
                dependency.dependency_type,
                dependency.owner,
                dependency.name,
                expected_dependency_type
            )));
        }
        if dependency.referenced_owner_oracle_maintained {
            continue;
        }
        if source_is_table && dependency.referenced_type != "TYPE" {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle table dependency {}.{} -> {}.{} ({}) is not covered by typed-column mapping",
                dependency.owner,
                dependency.name,
                dependency.referenced_owner,
                dependency.referenced_name,
                dependency.referenced_type
            )));
        }
        ensure_reference_owner(
            scope,
            &dependency.referenced_owner,
            &format!(
                "dependency {}.{} ({})",
                dependency.owner, dependency.name, dependency.object_type
            ),
        )?;
        let target_exists = match dependency.referenced_type.as_str() {
            "TABLE" => tables.contains(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            "VIEW" => views.contains(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            "MATERIALIZED VIEW" => materialized_views.contains(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            "SEQUENCE" => sequences.contains(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            "FUNCTION" | "PROCEDURE" => routines.contains_key(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
                dependency.referenced_type.clone(),
            )),
            "PACKAGE" => packages.contains_key(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            "SYNONYM" => synonyms.contains(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            "TYPE" => user_types.contains_key(&(
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
            )),
            _ => false,
        };
        if !target_exists {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle dependency target is outside the covered object set: {}.{} ({})",
                dependency.referenced_owner, dependency.referenced_name, dependency.referenced_type
            )));
        }
    }
    for synonym in &raw.synonyms {
        let target_dependency_count = raw
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.object_type == "SYNONYM"
                    && dependency.owner == synonym.owner
                    && dependency.name == synonym.name
                    && !dependency.referenced_owner_oracle_maintained
                    && dependency.referenced_owner == synonym.target_owner
                    && dependency.referenced_name == synonym.target_name
            })
            .count();
        if target_dependency_count != 1 {
            return Err(CatalogError::Mapping(format!(
                "Oracle synonym {}.{} has {target_dependency_count} matching target dependency rows; expected exactly one",
                synonym.owner, synonym.name
            )));
        }
    }
    let typed_column_dependencies = raw
        .columns
        .iter()
        .filter_map(|column| {
            Some((
                column.owner.as_str(),
                column.table.as_str(),
                column.data_type_owner.as_deref()?,
                column.data_type.as_str(),
            ))
        })
        .collect::<BTreeSet<_>>();
    for (owner, table, type_owner, type_name) in typed_column_dependencies {
        let dependency_count = raw
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.owner == owner
                    && dependency.name == table
                    && dependency.object_type == "TABLE"
                    && dependency.referenced_owner == type_owner
                    && dependency.referenced_name == type_name
                    && dependency.referenced_type == "TYPE"
                    && !dependency.referenced_owner_oracle_maintained
            })
            .count();
        if dependency_count != 1 {
            return Err(CatalogError::Mapping(format!(
                "Oracle typed table {owner}.{table} has {dependency_count} dependency rows for {type_owner}.{type_name}; expected exactly one"
            )));
        }
    }
    for view in &raw.materialized_views {
        let storage_dependency_count = raw
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.object_type == "MATERIALIZED VIEW"
                    && dependency.owner == view.owner
                    && dependency.name == view.name
                    && dependency.referenced_type == "TABLE"
                    && dependency.referenced_owner == view.owner
                    && dependency.referenced_name == view.container_name
            })
            .count();
        if storage_dependency_count != 1 {
            return Err(CatalogError::Mapping(format!(
                "Oracle materialized view {}.{} has {storage_dependency_count} storage-table dependency rows; expected exactly one",
                view.owner, view.name
            )));
        }
    }
    for trigger in raw
        .triggers
        .iter()
        .filter(|trigger| matches!(trigger.base_object_type.as_str(), "TABLE" | "VIEW"))
    {
        let target_owner = trigger.table_owner.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle trigger {}.{} has no target owner",
                trigger.owner, trigger.name
            ))
        })?;
        let target_name = trigger.table_name.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle trigger {}.{} has no target table",
                trigger.owner, trigger.name
            ))
        })?;
        let target_dependency_count = raw
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.object_type == "TRIGGER"
                    && dependency.owner == trigger.owner
                    && dependency.name == trigger.name
                    && !dependency.referenced_owner_oracle_maintained
                    && dependency.referenced_type == trigger.base_object_type
                    && dependency.referenced_owner == target_owner
                    && dependency.referenced_name == target_name
            })
            .count();
        if target_dependency_count != 1 {
            return Err(CatalogError::Mapping(format!(
                "Oracle trigger {}.{} has {target_dependency_count} target-{} dependency rows; expected exactly one",
                trigger.owner,
                trigger.name,
                trigger.base_object_type.to_lowercase()
            )));
        }
    }
    for package in raw.packages.iter().filter(|package| package.body.is_some()) {
        let body_link_count = raw
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.object_type == "PACKAGE BODY"
                    && dependency.owner == package.owner
                    && dependency.name == package.name
                    && !dependency.referenced_owner_oracle_maintained
                    && dependency.referenced_type == "PACKAGE"
                    && dependency.referenced_owner == package.owner
                    && dependency.referenced_name == package.name
            })
            .count();
        if body_link_count != 1 {
            return Err(CatalogError::Mapping(format!(
                "Oracle package body {}.{} has {body_link_count} specification-link dependency rows; expected exactly one",
                package.owner, package.name
            )));
        }
    }

    let mut constraints = BTreeMap::new();
    for constraint in &raw.constraints {
        ensure_owner(scope, &constraint.owner, "constraint")?;
        if !tables.contains(&(constraint.owner.clone(), constraint.table.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle constraint {}.{} has no mapped table {}",
                constraint.owner, constraint.name, constraint.table
            )));
        }
        if !matches!(constraint.constraint_type.as_str(), "P" | "U" | "R" | "C") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle constraint type '{}' is not covered for {}.{}",
                constraint.constraint_type, constraint.owner, constraint.name
            )));
        }
        if materialized_views.contains(&(constraint.owner.clone(), constraint.table.clone()))
            && !matches!(constraint.constraint_type.as_str(), "P" | "U" | "C")
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle materialized-view constraint type '{}' is not covered for {}.{}",
                constraint.constraint_type, constraint.owner, constraint.name
            )));
        }
        if matches!(constraint.constraint_type.as_str(), "P" | "U" | "R")
            && constraint.columns.is_empty()
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle constraint {}.{} has no catalog columns",
                constraint.owner, constraint.name
            )));
        }
        let mut positions = BTreeSet::new();
        for column in &constraint.columns {
            if let Some(position) = column.position {
                positive_u32(position, "Oracle constraint column ordinal")?;
                if !positions.insert(position) {
                    return Err(CatalogError::Mapping(format!(
                        "duplicate Oracle constraint column ordinal {} for {}.{}",
                        position, constraint.owner, constraint.name
                    )));
                }
            } else if constraint.constraint_type != "C" {
                return Err(CatalogError::Mapping(format!(
                    "Oracle constraint {}.{} has a column without an ordinal",
                    constraint.owner, constraint.name
                )));
            }
            if !column_keys.contains(&(
                constraint.owner.clone(),
                constraint.table.clone(),
                column.name.clone(),
            )) {
                return Err(CatalogError::Mapping(format!(
                    "Oracle constraint {}.{} references missing column {}.{}.{}",
                    constraint.owner,
                    constraint.name,
                    constraint.owner,
                    constraint.table,
                    column.name
                )));
            }
        }
        let identity = (constraint.owner.clone(), constraint.name.clone());
        if constraints.insert(identity.clone(), constraint).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle constraint identity {}.{}",
                identity.0, identity.1
            )));
        }
    }
    for constraint in &raw.constraints {
        if constraint.constraint_type != "R" {
            continue;
        }
        let referenced_owner = constraint.referenced_owner.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle foreign key {}.{} has no referenced owner",
                constraint.owner, constraint.name
            ))
        })?;
        ensure_reference_owner(
            scope,
            referenced_owner,
            &format!("foreign key {}.{}", constraint.owner, constraint.name),
        )?;
        let referenced_name = constraint.referenced_constraint.as_deref().ok_or_else(|| {
            CatalogError::Mapping(format!(
                "Oracle foreign key {}.{} has no referenced constraint",
                constraint.owner, constraint.name
            ))
        })?;
        let referenced = constraints
            .get(&(referenced_owner.to_owned(), referenced_name.to_owned()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle foreign key {}.{} references constraint outside the certified scope: {}.{}",
                    constraint.owner, constraint.name, referenced_owner, referenced_name
                ))
            })?;
        if !matches!(referenced.constraint_type.as_str(), "P" | "U") {
            return Err(CatalogError::Mapping(format!(
                "Oracle foreign key {}.{} references non-key constraint {}.{}",
                constraint.owner, constraint.name, referenced_owner, referenced_name
            )));
        }
        if referenced.columns.len() != constraint.columns.len() {
            return Err(CatalogError::Mapping(format!(
                "Oracle foreign key {}.{} has {} column(s), referenced constraint {}.{} has {}",
                constraint.owner,
                constraint.name,
                constraint.columns.len(),
                referenced_owner,
                referenced_name,
                referenced.columns.len()
            )));
        }
    }

    let columns_by_identity = raw
        .columns
        .iter()
        .map(|column| {
            (
                (
                    column.owner.clone(),
                    column.table.clone(),
                    column.name.clone(),
                ),
                column,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut indexes = BTreeSet::new();
    for index in &raw.indexes {
        ensure_owner(scope, &index.owner, "index")?;
        ensure_owner(scope, &index.table_owner, "indexed table")?;
        if index.owner != index.table_owner {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "cross-owner Oracle index {}.{} on {}.{} is outside the certified contract",
                index.owner, index.name, index.table_owner, index.table
            )));
        }
        if !tables.contains(&(index.table_owner.clone(), index.table.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle index {}.{} has no mapped table {}.{}",
                index.owner, index.name, index.table_owner, index.table
            )));
        }
        let function_based = matches!(
            index.index_type.as_str(),
            "FUNCTION-BASED NORMAL" | "FUNCTION-BASED BITMAP"
        );
        if !matches!(
            index.index_type.as_str(),
            "NORMAL" | "BITMAP" | "FUNCTION-BASED NORMAL" | "FUNCTION-BASED BITMAP"
        ) || index.secondary
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle index shape is not yet covered for {}.{} (type={}, partitioned={}, secondary={})",
                index.owner, index.name, index.index_type, index.partitioned, index.secondary
            )));
        }
        if index.columns.is_empty() {
            return Err(CatalogError::Mapping(format!(
                "Oracle index {}.{} has no catalog columns",
                index.owner, index.name
            )));
        }
        let mut positions = BTreeSet::new();
        let mut expression_count = 0;
        for column in &index.columns {
            positive_u32(column.position, "Oracle index column ordinal")?;
            if !positions.insert(column.position) {
                return Err(CatalogError::Mapping(format!(
                    "duplicate Oracle index column ordinal {} for {}.{}",
                    column.position, index.owner, index.name
                )));
            }
            let column_identity = (
                index.table_owner.clone(),
                index.table.clone(),
                column.name.clone(),
            );
            if !column_keys.contains(&column_identity) {
                return Err(CatalogError::Mapping(format!(
                    "Oracle index {}.{} references missing column {}.{}.{}",
                    index.owner, index.name, index.table_owner, index.table, column.name
                )));
            }
            let referenced_column = columns_by_identity
                .get(&column_identity)
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle index {}.{} has no column metadata for {}",
                        index.owner, index.name, column.name
                    ))
                })?;
            match column.expression.as_deref() {
                Some(expression) => {
                    expression_count += 1;
                    if !function_based
                        || expression.trim().is_empty()
                        || !referenced_column.hidden
                        || referenced_column.user_generated
                    {
                        return Err(CatalogError::Mapping(format!(
                            "Oracle index expression metadata is inconsistent for {}.{} position {}",
                            index.owner, index.name, column.position
                        )));
                    }
                }
                None if function_based
                    && referenced_column.hidden
                    && !referenced_column.user_generated =>
                {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle function-based index {}.{} is missing expression metadata at position {}",
                        index.owner, index.name, column.position
                    )));
                }
                None => {}
            }
        }
        if function_based != (expression_count > 0) {
            return Err(CatalogError::Mapping(format!(
                "Oracle index type and expression catalog disagree for {}.{}",
                index.owner, index.name
            )));
        }
        match (function_based, index.function_status.as_deref()) {
            (true, Some("ENABLED")) | (false, None) => {}
            (true, status) => {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle function-based index {}.{} is not enabled (status={})",
                    index.owner,
                    index.name,
                    status.unwrap_or("missing")
                )));
            }
            (false, Some(status)) => {
                return Err(CatalogError::Mapping(format!(
                    "Oracle non-function index {}.{} unexpectedly reports function status '{status}'",
                    index.owner, index.name
                )));
            }
        }
        let expression = oracle_index_expression(index);
        if expression
            .as_ref()
            .is_some_and(|expression| expression.len() > MAX_DEFINITION_BYTES)
        {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle index expression exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{}",
                index.owner, index.name
            )));
        }
        if !indexes.insert((index.owner.clone(), index.name.clone())) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle index identity {}.{}",
                index.owner, index.name
            )));
        }
        if !inventory_keys.contains(&(index.owner.clone(), "INDEX".to_owned(), index.name.clone()))
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle index {}.{} is missing from the independent object inventory",
                index.owner, index.name
            )));
        }
    }
    let inventory_index_count = inventory
        .iter()
        .filter(|object| object.object_type == "INDEX")
        .count();
    if inventory_index_count != raw.indexes.len() + raw.lobs.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle index inventory mismatch: USER/DBA_OBJECTS reports {inventory_index_count}, regular indexes plus LOB indexes report {}",
            raw.indexes.len() + raw.lobs.len()
        )));
    }

    validate_partition_catalog(
        raw,
        scope,
        &inventory_subobject_keys,
        &tables,
        &column_keys,
        &indexes,
    )?;
    validate_lob_catalog(
        raw,
        scope,
        &inventory_keys,
        &inventory_subobject_keys,
        &tables,
        &column_keys,
    )?;

    Ok(())
}

fn validate_partition_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
    inventory_subobject_keys: &BTreeSet<(String, String, String, String)>,
    tables: &BTreeSet<(String, String)>,
    column_keys: &BTreeSet<(String, String, String)>,
    indexes: &BTreeSet<(String, String)>,
) -> Result<(), CatalogError> {
    let lob_index_names = raw
        .lobs
        .iter()
        .map(|lob| (lob.owner.clone(), lob.index_name.clone()))
        .collect::<BTreeSet<_>>();
    let raw_tables = raw
        .tables
        .iter()
        .map(|table| ((table.owner.clone(), table.name.clone()), table))
        .collect::<BTreeMap<_, _>>();
    let expected_partitioned_tables = raw
        .tables
        .iter()
        .filter(|table| table.partitioned)
        .map(|table| (table.owner.clone(), table.name.clone()))
        .collect::<BTreeSet<_>>();
    let mut partitioned_tables = BTreeMap::new();
    for table in &raw.partitioned_tables {
        ensure_owner(scope, &table.owner, "partitioned table")?;
        if !tables.contains(&(table.owner.clone(), table.table.clone()))
            || !raw_tables
                .get(&(table.owner.clone(), table.table.clone()))
                .is_some_and(|raw_table| raw_table.partitioned)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition metadata references non-partitioned table {}.{}",
                table.owner, table.table
            )));
        }
        ensure_partitioning_type(
            &table.partitioning_type,
            false,
            &format!("Oracle table {}.{}", table.owner, table.table),
        )?;
        ensure_partitioning_type(
            &table.subpartitioning_type,
            true,
            &format!("Oracle table {}.{}", table.owner, table.table),
        )?;
        if table.status != "VALID"
            || table.partition_count <= 0
            || table.partitioning_key_count <= 0
            || table.default_subpartition_count < 0
            || table.subpartitioning_key_count < 0
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition header is malformed for {}.{}",
                table.owner, table.table
            )));
        }
        let has_subpartitions = table.subpartitioning_type != "NONE";
        if has_subpartitions
            != (table.default_subpartition_count > 0 && table.subpartitioning_key_count > 0)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle subpartition header is inconsistent for {}.{}",
                table.owner, table.table
            )));
        }
        for (name, value) in [
            ("autolist", table.autolist.as_deref()),
            (
                "autolist_subpartition",
                table.autolist_subpartition.as_deref(),
            ),
            ("auto", table.automatic.as_deref()),
        ] {
            if let Some(value) = value {
                ensure_yes_no(
                    value,
                    &format!("Oracle table {}.{} {name}", table.owner, table.table),
                )?;
            }
        }
        let identity = (table.owner.clone(), table.table.clone());
        if partitioned_tables.insert(identity.clone(), table).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle partitioned-table header {}.{}",
                identity.0, identity.1
            )));
        }
    }
    if partitioned_tables.keys().cloned().collect::<BTreeSet<_>>() != expected_partitioned_tables {
        return Err(CatalogError::Mapping(
            "Oracle USER/DBA_PART_TABLES does not exactly match partitioned USER/DBA_TABLES rows"
                .to_owned(),
        ));
    }

    let mut table_partitions_by_table =
        BTreeMap::<(String, String), Vec<&RawTablePartition>>::new();
    let mut table_partition_identities = BTreeMap::new();
    for partition in &raw.table_partitions {
        ensure_owner(scope, &partition.owner, "table partition")?;
        let header = partitioned_tables
            .get(&(partition.owner.clone(), partition.table.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle table partition {}.{}.{} has no partitioned-table header",
                    partition.owner, partition.table, partition.name
                ))
            })?;
        positive_u32(partition.position, "Oracle table partition position")?;
        if partition.subpartition_count < 0
            || !matches!(partition.composite.as_str(), "YES" | "NO")
            || (partition.composite == "YES") != (header.subpartitioning_type != "NONE")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle table partition metadata is malformed for {}.{}.{}",
                partition.owner, partition.table, partition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "TABLE PARTITION".to_owned(),
            partition.table.clone(),
            partition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle table partition {}.{}.{} is missing from the independent object inventory",
                partition.owner, partition.table, partition.name
            )));
        }
        let identity = (
            partition.owner.clone(),
            partition.table.clone(),
            partition.name.clone(),
        );
        if table_partition_identities
            .insert(identity.clone(), partition)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle table partition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        table_partitions_by_table
            .entry((partition.owner.clone(), partition.table.clone()))
            .or_default()
            .push(partition);
    }
    for (identity, header) in &partitioned_tables {
        let partitions = table_partitions_by_table
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if partitions.len() != header.partition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle table partition count mismatch for {}.{}",
                identity.0, identity.1
            )));
        }
        ensure_contiguous_positions(
            partitions.iter().map(|partition| partition.position),
            &format!("Oracle table partitions {}.{}", identity.0, identity.1),
        )?;
    }
    let inventory_table_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "TABLE PARTITION")
        .count();
    if inventory_table_partition_count != raw.table_partitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle table-partition inventory mismatch: USER/DBA_OBJECTS reports {inventory_table_partition_count}, USER/DBA_TAB_PARTITIONS reports {}",
            raw.table_partitions.len()
        )));
    }

    let mut table_subpartitions_by_partition =
        BTreeMap::<(String, String, String), Vec<&RawTableSubpartition>>::new();
    let mut table_subpartition_identities = BTreeSet::new();
    for subpartition in &raw.table_subpartitions {
        ensure_owner(scope, &subpartition.owner, "table subpartition")?;
        let parent = table_partition_identities
            .get(&(
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.partition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle table subpartition {}.{}.{} has no parent partition {}",
                    subpartition.owner,
                    subpartition.table,
                    subpartition.name,
                    subpartition.partition
                ))
            })?;
        positive_u32(subpartition.position, "Oracle table subpartition position")?;
        if subpartition.partition_position != parent.position {
            return Err(CatalogError::Mapping(format!(
                "Oracle table subpartition parent position mismatch for {}.{}.{}",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "TABLE SUBPARTITION".to_owned(),
            subpartition.table.clone(),
            subpartition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle table subpartition {}.{}.{} is missing from the independent object inventory",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        let identity = (
            subpartition.owner.clone(),
            subpartition.table.clone(),
            subpartition.name.clone(),
        );
        if !table_subpartition_identities.insert(identity.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle table subpartition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        table_subpartitions_by_partition
            .entry((
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.partition.clone(),
            ))
            .or_default()
            .push(subpartition);
    }
    for (identity, parent) in &table_partition_identities {
        let subpartitions = table_subpartitions_by_partition
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if subpartitions.len() != parent.subpartition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle table subpartition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            subpartitions
                .iter()
                .map(|subpartition| subpartition.position),
            &format!(
                "Oracle table subpartitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_table_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "TABLE SUBPARTITION")
        .count();
    if inventory_table_subpartition_count != raw.table_subpartitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle table-subpartition inventory mismatch: USER/DBA_OBJECTS reports {inventory_table_subpartition_count}, USER/DBA_TAB_SUBPARTITIONS reports {}",
            raw.table_subpartitions.len()
        )));
    }

    let raw_indexes = raw
        .indexes
        .iter()
        .map(|index| ((index.owner.clone(), index.name.clone()), index))
        .collect::<BTreeMap<_, _>>();
    let expected_partitioned_indexes = raw
        .indexes
        .iter()
        .filter(|index| index.partitioned)
        .map(|index| (index.owner.clone(), index.name.clone()))
        .collect::<BTreeSet<_>>();
    let mut partitioned_indexes = BTreeMap::new();
    for index in &raw.partitioned_indexes {
        ensure_owner(scope, &index.owner, "partitioned index")?;
        if !indexes.contains(&(index.owner.clone(), index.index.clone())) {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition metadata references missing index {}.{}",
                index.owner, index.index
            )));
        }
        let raw_index = raw_indexes
            .get(&(index.owner.clone(), index.index.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle partitioned-index header has no index {}.{}",
                    index.owner, index.index
                ))
            })?;
        if !raw_index.partitioned || raw_index.table != index.table {
            return Err(CatalogError::Mapping(format!(
                "Oracle partitioned-index header disagrees with USER/DBA_INDEXES for {}.{}",
                index.owner, index.index
            )));
        }
        ensure_partitioning_type(
            &index.partitioning_type,
            false,
            &format!("Oracle index {}.{}", index.owner, index.index),
        )?;
        ensure_partitioning_type(
            &index.subpartitioning_type,
            true,
            &format!("Oracle index {}.{}", index.owner, index.index),
        )?;
        if index.partition_count <= 0
            || index.partitioning_key_count <= 0
            || index.default_subpartition_count < 0
            || index.subpartitioning_key_count < 0
            || !matches!(index.locality.as_str(), "LOCAL" | "GLOBAL")
            || !matches!(index.alignment.as_str(), "PREFIXED" | "NON_PREFIXED")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle partitioned-index header is malformed for {}.{}",
                index.owner, index.index
            )));
        }
        let has_subpartitions = index.subpartitioning_type != "NONE";
        if has_subpartitions
            != (index.default_subpartition_count > 0 && index.subpartitioning_key_count > 0)
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition header is inconsistent for {}.{}",
                index.owner, index.index
            )));
        }
        for (name, value) in [
            ("autolist", index.autolist.as_deref()),
            (
                "autolist_subpartition",
                index.autolist_subpartition.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                ensure_yes_no(
                    value,
                    &format!("Oracle index {}.{} {name}", index.owner, index.index),
                )?;
            }
        }
        let identity = (index.owner.clone(), index.index.clone());
        if partitioned_indexes
            .insert(identity.clone(), index)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle partitioned-index header {}.{}",
                identity.0, identity.1
            )));
        }
    }
    if partitioned_indexes.keys().cloned().collect::<BTreeSet<_>>() != expected_partitioned_indexes
    {
        return Err(CatalogError::Mapping(
            "Oracle USER/DBA_PART_INDEXES does not exactly match partitioned USER/DBA_INDEXES rows"
                .to_owned(),
        ));
    }

    let mut index_partitions_by_index =
        BTreeMap::<(String, String), Vec<&RawIndexPartition>>::new();
    let mut index_partition_identities = BTreeMap::new();
    for partition in &raw.index_partitions {
        ensure_owner(scope, &partition.owner, "index partition")?;
        let header = partitioned_indexes
            .get(&(partition.owner.clone(), partition.index.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle index partition {}.{}.{} has no partitioned-index header",
                    partition.owner, partition.index, partition.name
                ))
            })?;
        positive_u32(partition.position, "Oracle index partition position")?;
        if partition.subpartition_count < 0
            || !matches!(partition.composite.as_str(), "YES" | "NO")
            || (partition.composite == "YES") != (header.subpartitioning_type != "NONE")
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle index partition metadata is malformed for {}.{}.{}",
                partition.owner, partition.index, partition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "INDEX PARTITION".to_owned(),
            partition.index.clone(),
            partition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle index partition {}.{}.{} is missing from the independent object inventory",
                partition.owner, partition.index, partition.name
            )));
        }
        let identity = (
            partition.owner.clone(),
            partition.index.clone(),
            partition.name.clone(),
        );
        if index_partition_identities
            .insert(identity.clone(), partition)
            .is_some()
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle index partition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        index_partitions_by_index
            .entry((partition.owner.clone(), partition.index.clone()))
            .or_default()
            .push(partition);
    }
    for (identity, header) in &partitioned_indexes {
        let partitions = index_partitions_by_index
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if partitions.len() != header.partition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle index partition count mismatch for {}.{}",
                identity.0, identity.1
            )));
        }
        ensure_contiguous_positions(
            partitions.iter().map(|partition| partition.position),
            &format!("Oracle index partitions {}.{}", identity.0, identity.1),
        )?;
    }
    let inventory_index_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX PARTITION" && !lob_index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_index_partition_count != raw.index_partitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle index-partition inventory mismatch: USER/DBA_OBJECTS reports {inventory_index_partition_count}, USER/DBA_IND_PARTITIONS reports {}",
            raw.index_partitions.len()
        )));
    }

    let mut index_subpartitions_by_partition =
        BTreeMap::<(String, String, String), Vec<&RawIndexSubpartition>>::new();
    let mut index_subpartition_identities = BTreeSet::new();
    for subpartition in &raw.index_subpartitions {
        ensure_owner(scope, &subpartition.owner, "index subpartition")?;
        let parent = index_partition_identities
            .get(&(
                subpartition.owner.clone(),
                subpartition.index.clone(),
                subpartition.partition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle index subpartition {}.{}.{} has no parent partition {}",
                    subpartition.owner,
                    subpartition.index,
                    subpartition.name,
                    subpartition.partition
                ))
            })?;
        positive_u32(subpartition.position, "Oracle index subpartition position")?;
        if subpartition.partition_position != parent.position {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition parent position mismatch for {}.{}.{}",
                subpartition.owner, subpartition.index, subpartition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "INDEX SUBPARTITION".to_owned(),
            subpartition.index.clone(),
            subpartition.name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition {}.{}.{} is missing from the independent object inventory",
                subpartition.owner, subpartition.index, subpartition.name
            )));
        }
        let identity = (
            subpartition.owner.clone(),
            subpartition.index.clone(),
            subpartition.name.clone(),
        );
        if !index_subpartition_identities.insert(identity.clone()) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle index subpartition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        index_subpartitions_by_partition
            .entry((
                subpartition.owner.clone(),
                subpartition.index.clone(),
                subpartition.partition.clone(),
            ))
            .or_default()
            .push(subpartition);
    }
    for (identity, parent) in &index_partition_identities {
        let subpartitions = index_subpartitions_by_partition
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if subpartitions.len() != parent.subpartition_count as usize {
            return Err(CatalogError::Mapping(format!(
                "Oracle index subpartition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            subpartitions
                .iter()
                .map(|subpartition| subpartition.position),
            &format!(
                "Oracle index subpartitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_index_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX SUBPARTITION"
                && !lob_index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_index_subpartition_count != raw.index_subpartitions.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle index-subpartition inventory mismatch: USER/DBA_OBJECTS reports {inventory_index_subpartition_count}, USER/DBA_IND_SUBPARTITIONS reports {}",
            raw.index_subpartitions.len()
        )));
    }

    let mut keys_by_object =
        BTreeMap::<(String, String, String, bool), Vec<&RawPartitionKeyColumn>>::new();
    let mut key_identities = BTreeSet::new();
    for key_column in &raw.partition_key_columns {
        ensure_owner(scope, &key_column.owner, "partition key column")?;
        if !matches!(key_column.object_type.as_str(), "TABLE" | "INDEX") {
            return Err(CatalogError::UnsupportedMetadata(format!(
                "Oracle partition key {}.{} has unsupported object type '{}'",
                key_column.owner, key_column.name, key_column.object_type
            )));
        }
        positive_u32(key_column.position, "Oracle partition key column position")?;
        if key_column.collated_column_id.is_some_and(|id| id <= 0) {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition key {}.{}.{} has invalid collated column id",
                key_column.owner, key_column.name, key_column.column
            )));
        }
        let target_table = if key_column.object_type == "TABLE" {
            key_column.name.as_str()
        } else {
            raw_indexes
                .get(&(key_column.owner.clone(), key_column.name.clone()))
                .map(|index| index.table.as_str())
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle index partition key references missing index {}.{}",
                        key_column.owner, key_column.name
                    ))
                })?
        };
        if !column_keys.contains(&(
            key_column.owner.clone(),
            target_table.to_owned(),
            key_column.column.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle partition key {}.{}.{} references a missing column",
                key_column.owner, key_column.name, key_column.column
            )));
        }
        let identity = (
            key_column.owner.clone(),
            key_column.name.clone(),
            key_column.object_type.clone(),
            key_column.subpartition,
            key_column.position,
        );
        if !key_identities.insert(identity) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle partition key position for {}.{}",
                key_column.owner, key_column.name
            )));
        }
        keys_by_object
            .entry((
                key_column.owner.clone(),
                key_column.name.clone(),
                key_column.object_type.clone(),
                key_column.subpartition,
            ))
            .or_default()
            .push(key_column);
    }
    for table in &raw.partitioned_tables {
        for (subpartition, expected) in [
            (false, table.partitioning_key_count),
            (true, table.subpartitioning_key_count),
        ] {
            let key = (
                table.owner.clone(),
                table.table.clone(),
                "TABLE".to_owned(),
                subpartition,
            );
            let columns = keys_by_object
                .get(&key)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if columns.len() != expected as usize {
                return Err(CatalogError::Mapping(format!(
                    "Oracle table partition-key count mismatch for {}.{}",
                    table.owner, table.table
                )));
            }
            ensure_contiguous_positions(
                columns.iter().map(|column| column.position),
                &format!(
                    "Oracle table partition keys {}.{}",
                    table.owner, table.table
                ),
            )?;
        }
    }
    for index in &raw.partitioned_indexes {
        for (subpartition, expected) in [
            (false, index.partitioning_key_count),
            (true, index.subpartitioning_key_count),
        ] {
            let key = (
                index.owner.clone(),
                index.index.clone(),
                "INDEX".to_owned(),
                subpartition,
            );
            let columns = keys_by_object
                .get(&key)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if columns.len() != expected as usize {
                return Err(CatalogError::Mapping(format!(
                    "Oracle index partition-key count mismatch for {}.{}",
                    index.owner, index.index
                )));
            }
            ensure_contiguous_positions(
                columns.iter().map(|column| column.position),
                &format!(
                    "Oracle index partition keys {}.{}",
                    index.owner, index.index
                ),
            )?;
        }
    }
    let expected_key_count = raw
        .partitioned_tables
        .iter()
        .map(|table| table.partitioning_key_count + table.subpartitioning_key_count)
        .chain(
            raw.partitioned_indexes
                .iter()
                .map(|index| index.partitioning_key_count + index.subpartitioning_key_count),
        )
        .sum::<i64>();
    if expected_key_count < 0 || raw.partition_key_columns.len() != expected_key_count as usize {
        return Err(CatalogError::Mapping(
            "Oracle partition-key catalogs contain unclaimed or missing rows".to_owned(),
        ));
    }

    Ok(())
}

fn validate_lob_catalog(
    raw: &RawOracleCatalog,
    scope: &DictionaryScope,
    inventory_keys: &BTreeSet<(String, String, String)>,
    inventory_subobject_keys: &BTreeSet<(String, String, String, String)>,
    tables: &BTreeSet<(String, String)>,
    column_keys: &BTreeSet<(String, String, String)>,
) -> Result<(), CatalogError> {
    let raw_tables = raw
        .tables
        .iter()
        .map(|table| ((table.owner.clone(), table.name.clone()), table))
        .collect::<BTreeMap<_, _>>();
    let mut lobs = BTreeMap::new();
    let mut segment_names = BTreeSet::new();
    let mut index_names = BTreeSet::new();
    for lob in &raw.lobs {
        ensure_owner(scope, &lob.owner, "LOB")?;
        let table = raw_tables
            .get(&(lob.owner.clone(), lob.table.clone()))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB {}.{}.{} has no parent table",
                    lob.owner, lob.table, lob.column
                ))
            })?;
        if !tables.contains(&(lob.owner.clone(), lob.table.clone()))
            || !column_keys.contains(&(lob.owner.clone(), lob.table.clone(), lob.column.clone()))
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB {}.{}.{} has no parent column",
                lob.owner, lob.table, lob.column
            )));
        }
        ensure_yes_no(
            &lob.partitioned,
            &format!(
                "Oracle LOB {}.{}.{} partitioned",
                lob.owner, lob.table, lob.column
            ),
        )?;
        ensure_yes_no(
            &lob.securefile,
            &format!(
                "Oracle LOB {}.{}.{} securefile",
                lob.owner, lob.table, lob.column
            ),
        )?;
        if (lob.partitioned == "YES") != table.partitioned
            || lob.chunk <= 0
            || lob.pctversion.is_some_and(|value| value < 0)
            || lob.retention.is_some_and(|value| value < 0)
            || lob.freepools.is_some_and(|value| value < 0)
            || lob.retention_value.is_some_and(|value| value < 0)
            || lob.max_inline.is_some_and(|value| value < 0)
            || [
                lob.cache.as_str(),
                lob.logging.as_str(),
                lob.encrypt.as_str(),
                lob.compression.as_str(),
                lob.deduplication.as_str(),
                lob.in_row.as_str(),
                lob.format.as_str(),
                lob.segment_created.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB metadata is malformed for {}.{}.{}",
                lob.owner, lob.table, lob.column
            )));
        }
        if !inventory_keys.contains(&(
            lob.owner.clone(),
            "LOB".to_owned(),
            lob.segment_name.clone(),
        )) || !inventory_keys.contains(&(
            lob.owner.clone(),
            "INDEX".to_owned(),
            lob.index_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB {}.{}.{} is missing its segment or index inventory row",
                lob.owner, lob.table, lob.column
            )));
        }
        if !segment_names.insert((lob.owner.clone(), lob.segment_name.clone()))
            || !index_names.insert((lob.owner.clone(), lob.index_name.clone()))
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB segment or index identity for {}.{}.{}",
                lob.owner, lob.table, lob.column
            )));
        }
        let identity = (lob.owner.clone(), lob.table.clone(), lob.column.clone());
        if lobs.insert(identity.clone(), lob).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB column {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
    }
    let inventory_lob_count = inventory_keys.iter().filter(|key| key.1 == "LOB").count();
    if inventory_lob_count != raw.lobs.len() {
        return Err(CatalogError::Mapping(format!(
            "Oracle LOB inventory mismatch: USER/DBA_OBJECTS reports {inventory_lob_count}, USER/DBA_LOBS reports {}",
            raw.lobs.len()
        )));
    }

    let table_partitions = raw
        .table_partitions
        .iter()
        .map(|partition| {
            (
                (
                    partition.owner.clone(),
                    partition.table.clone(),
                    partition.name.clone(),
                ),
                partition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut lob_partitions = BTreeMap::new();
    let mut lob_partitions_by_lob =
        BTreeMap::<(String, String, String), Vec<&RawLobPartition>>::new();
    let mut lob_index_partition_names = BTreeSet::new();
    for partition in &raw.lob_partitions {
        ensure_owner(scope, &partition.owner, "LOB partition")?;
        let lob = lobs
            .get(&(
                partition.owner.clone(),
                partition.table.clone(),
                partition.column.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB partition {}.{}.{} has no parent LOB",
                    partition.owner, partition.table, partition.name
                ))
            })?;
        let table_partition = table_partitions
            .get(&(
                partition.owner.clone(),
                partition.table.clone(),
                partition.table_partition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB partition {}.{}.{} has no table partition {}",
                    partition.owner, partition.table, partition.name, partition.table_partition
                ))
            })?;
        positive_u32(partition.position, "Oracle LOB partition position")?;
        if lob.segment_name != partition.lob_name
            || partition.position != table_partition.position
            || partition.composite != table_partition.composite
            || partition.chunk <= 0
            || partition.pctversion.is_some_and(|value| value < 0)
            || partition.max_inline.is_some_and(|value| value < 0)
            || [
                partition.cache.as_str(),
                partition.in_row.as_str(),
                partition.logging.as_str(),
                partition.encrypt.as_str(),
                partition.compression.as_str(),
                partition.deduplication.as_str(),
                partition.securefile.as_str(),
                partition.segment_created.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB partition metadata is inconsistent for {}.{}.{}",
                partition.owner, partition.table, partition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "LOB PARTITION".to_owned(),
            partition.lob_name.clone(),
            partition.name.clone(),
        )) || !inventory_subobject_keys.contains(&(
            partition.owner.clone(),
            "INDEX PARTITION".to_owned(),
            lob.index_name.clone(),
            partition.index_partition_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB partition {}.{}.{} is missing its segment or index inventory row",
                partition.owner, partition.table, partition.name
            )));
        }
        if !lob_index_partition_names.insert((
            partition.owner.clone(),
            lob.index_name.clone(),
            partition.index_partition_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB index partition {}.{}",
                partition.owner, partition.index_partition_name
            )));
        }
        let identity = (
            partition.owner.clone(),
            partition.lob_name.clone(),
            partition.name.clone(),
        );
        if lob_partitions.insert(identity.clone(), partition).is_some() {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB partition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        lob_partitions_by_lob
            .entry((
                partition.owner.clone(),
                partition.table.clone(),
                partition.column.clone(),
            ))
            .or_default()
            .push(partition);
    }
    for (identity, lob) in &lobs {
        let partitions = lob_partitions_by_lob
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let expected = if lob.partitioned == "YES" {
            raw.table_partitions
                .iter()
                .filter(|partition| partition.owner == lob.owner && partition.table == lob.table)
                .count()
        } else {
            0
        };
        if partitions.len() != expected {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB partition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            partitions.iter().map(|partition| partition.position),
            &format!(
                "Oracle LOB partitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_lob_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "LOB PARTITION")
        .count();
    let inventory_lob_index_partition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX PARTITION" && index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_lob_partition_count != raw.lob_partitions.len()
        || inventory_lob_index_partition_count != raw.lob_partitions.len()
        || lob_index_partition_names.len() != raw.lob_partitions.len()
    {
        return Err(CatalogError::Mapping(format!(
            "Oracle LOB-partition inventory mismatch: LOB={inventory_lob_partition_count}, INDEX={inventory_lob_index_partition_count}, catalog={}",
            lob_index_partition_names.len()
        )));
    }

    let table_subpartitions = raw
        .table_subpartitions
        .iter()
        .map(|subpartition| {
            (
                (
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.name.clone(),
                ),
                subpartition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut lob_subpartition_identities = BTreeSet::new();
    let mut lob_subpartitions_by_partition =
        BTreeMap::<(String, String, String), Vec<&RawLobSubpartition>>::new();
    let mut lob_index_subpartition_names = BTreeSet::new();
    for subpartition in &raw.lob_subpartitions {
        ensure_owner(scope, &subpartition.owner, "LOB subpartition")?;
        let lob = lobs
            .get(&(
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.column.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB subpartition {}.{}.{} has no parent LOB",
                    subpartition.owner, subpartition.table, subpartition.name
                ))
            })?;
        let parent = lob_partitions
            .get(&(
                subpartition.owner.clone(),
                subpartition.lob_name.clone(),
                subpartition.lob_partition_name.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB subpartition {}.{}.{} has no parent LOB partition",
                    subpartition.owner, subpartition.table, subpartition.name
                ))
            })?;
        let table_subpartition = table_subpartitions
            .get(&(
                subpartition.owner.clone(),
                subpartition.table.clone(),
                subpartition.table_subpartition.clone(),
            ))
            .copied()
            .ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle LOB subpartition {}.{}.{} has no table subpartition {}",
                    subpartition.owner,
                    subpartition.table,
                    subpartition.name,
                    subpartition.table_subpartition
                ))
            })?;
        positive_u32(subpartition.position, "Oracle LOB subpartition position")?;
        if subpartition.lob_name != lob.segment_name
            || table_subpartition.partition != parent.table_partition
            || subpartition.position != table_subpartition.position
            || subpartition.chunk <= 0
            || subpartition.pctversion.is_some_and(|value| value < 0)
            || subpartition.max_inline.is_some_and(|value| value < 0)
            || [
                subpartition.cache.as_str(),
                subpartition.in_row.as_str(),
                subpartition.logging.as_str(),
                subpartition.encrypt.as_str(),
                subpartition.compression.as_str(),
                subpartition.deduplication.as_str(),
                subpartition.securefile.as_str(),
                subpartition.segment_created.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB subpartition metadata is inconsistent for {}.{}.{}",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        if !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "LOB SUBPARTITION".to_owned(),
            subpartition.lob_name.clone(),
            subpartition.name.clone(),
        )) || !inventory_subobject_keys.contains(&(
            subpartition.owner.clone(),
            "INDEX SUBPARTITION".to_owned(),
            lob.index_name.clone(),
            subpartition.index_subpartition_name.clone(),
        )) {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB subpartition {}.{}.{} is missing its segment or index inventory row",
                subpartition.owner, subpartition.table, subpartition.name
            )));
        }
        let identity = (
            subpartition.owner.clone(),
            subpartition.lob_name.clone(),
            subpartition.name.clone(),
        );
        if !lob_subpartition_identities.insert(identity.clone())
            || !lob_index_subpartition_names.insert((
                subpartition.owner.clone(),
                lob.index_name.clone(),
                subpartition.index_subpartition_name.clone(),
            ))
        {
            return Err(CatalogError::Mapping(format!(
                "duplicate Oracle LOB subpartition {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        lob_subpartitions_by_partition
            .entry((
                subpartition.owner.clone(),
                subpartition.lob_name.clone(),
                subpartition.lob_partition_name.clone(),
            ))
            .or_default()
            .push(subpartition);
    }
    for (identity, partition) in &lob_partitions {
        let subpartitions = lob_subpartitions_by_partition
            .get(identity)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let expected = table_partitions
            .get(&(
                partition.owner.clone(),
                partition.table.clone(),
                partition.table_partition.clone(),
            ))
            .map_or(0, |table_partition| {
                table_partition.subpartition_count as usize
            });
        if subpartitions.len() != expected {
            return Err(CatalogError::Mapping(format!(
                "Oracle LOB subpartition count mismatch for {}.{}.{}",
                identity.0, identity.1, identity.2
            )));
        }
        ensure_contiguous_positions(
            subpartitions
                .iter()
                .map(|subpartition| subpartition.position),
            &format!(
                "Oracle LOB subpartitions {}.{}.{}",
                identity.0, identity.1, identity.2
            ),
        )?;
    }
    let inventory_lob_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| key.1 == "LOB SUBPARTITION")
        .count();
    let inventory_lob_index_subpartition_count = inventory_subobject_keys
        .iter()
        .filter(|key| {
            key.1 == "INDEX SUBPARTITION" && index_names.contains(&(key.0.clone(), key.2.clone()))
        })
        .count();
    if inventory_lob_subpartition_count != raw.lob_subpartitions.len()
        || inventory_lob_index_subpartition_count != raw.lob_subpartitions.len()
        || lob_index_subpartition_names.len() != raw.lob_subpartitions.len()
    {
        return Err(CatalogError::Mapping(format!(
            "Oracle LOB-subpartition inventory mismatch: LOB={inventory_lob_subpartition_count}, INDEX={inventory_lob_index_subpartition_count}, catalog={}",
            lob_index_subpartition_names.len()
        )));
    }

    Ok(())
}

fn ensure_partitioning_type(
    value: &str,
    allow_none: bool,
    subject: &str,
) -> Result<(), CatalogError> {
    if matches!(
        value,
        "RANGE" | "HASH" | "LIST" | "REFERENCE" | "SYSTEM" | "CONSISTENT HASH"
    ) || (allow_none && value == "NONE")
    {
        Ok(())
    } else {
        Err(CatalogError::UnsupportedMetadata(format!(
            "{subject} has unsupported partitioning type '{value}'"
        )))
    }
}

fn ensure_contiguous_positions(
    positions: impl Iterator<Item = i64>,
    subject: &str,
) -> Result<(), CatalogError> {
    let positions = positions.collect::<Vec<_>>();
    if positions
        .iter()
        .enumerate()
        .all(|(offset, position)| *position == (offset + 1) as i64)
    {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "{subject} do not have contiguous 1-based positions"
        )))
    }
}

fn ensure_owner(scope: &DictionaryScope, owner: &str, subject: &str) -> Result<(), CatalogError> {
    if scope.contains_owner(owner) {
        Ok(())
    } else {
        Err(CatalogError::Mapping(format!(
            "Oracle {subject} owner '{owner}' is outside the certified schema scope"
        )))
    }
}

fn ensure_reference_owner(
    scope: &DictionaryScope,
    owner: &str,
    source: &str,
) -> Result<(), CatalogError> {
    if scope.contains_owner(owner) {
        Ok(())
    } else {
        Err(CatalogError::InvalidScope(format!(
            "Oracle schema selection is not relationship-closed: {source} references application owner '{owner}'; include that owner and retry"
        )))
    }
}

struct OracleSnapshotMapper<'a> {
    connection_alias: &'a str,
    facts: ServerFacts,
    strategy: OracleCatalogVersion,
    scope: DictionaryScope,
}

impl<'a> OracleSnapshotMapper<'a> {
    fn new(
        connection_alias: &'a str,
        facts: ServerFacts,
        strategy: OracleCatalogVersion,
        scope: DictionaryScope,
    ) -> Self {
        Self {
            connection_alias,
            facts,
            strategy,
            scope,
        }
    }

    fn map(self, raw: RawOracleCatalog) -> Result<CatalogDiscovery, CatalogError> {
        let database_name = self.facts.container.clone();
        let database_key = oracle_key(
            self.connection_alias,
            &database_name,
            &database_name,
            ObjectKind::Database,
            &database_name,
            None,
        );
        let database = DatabaseObject {
            key: database_key.clone(),
            name: database_name.clone(),
        };

        let schemas = self
            .scope
            .owners
            .iter()
            .map(|owner| SchemaObject {
                key: oracle_key(
                    self.connection_alias,
                    &database_name,
                    owner,
                    ObjectKind::Schema,
                    owner,
                    None,
                ),
                database_key: database_key.clone(),
                name: owner.clone(),
            })
            .collect::<Vec<_>>();
        let schema_keys = schemas
            .iter()
            .map(|schema| (schema.name.clone(), schema.key.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut metadata = CanonicalMetadata::default();
        for principal in &self.scope.principals {
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &database_name,
                ObjectKind::Principal,
                &principal.name,
                None,
            );
            let mut properties = BTreeMap::new();
            insert_i64(&mut properties, "oracle_user_id", principal.user_id);
            insert_string(&mut properties, "account_status", &principal.account_status);
            insert_bool(&mut properties, "common", principal.common);
            insert_bool(
                &mut properties,
                "oracle_maintained",
                principal.oracle_maintained,
            );
            insert_optional_string(
                &mut properties,
                "default_collation",
                principal.default_collation.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(database_key.clone()),
                name: principal.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let inventory = raw
            .inventory
            .iter()
            .filter(|object| !object.secondary && object.subobject.is_none())
            .map(|object| {
                (
                    (
                        object.owner.clone(),
                        object.object_type.clone(),
                        object.name.clone(),
                    ),
                    object,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let subobject_inventory = raw
            .inventory
            .iter()
            .filter(|object| !object.secondary)
            .filter_map(|object| {
                Some((
                    (
                        object.owner.clone(),
                        object.object_type.clone(),
                        object.name.clone(),
                        object.subobject.clone()?,
                    ),
                    object,
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let partitioned_tables = raw
            .partitioned_tables
            .iter()
            .map(|table| ((table.owner.clone(), table.table.clone()), table))
            .collect::<BTreeMap<_, _>>();
        let partitioned_indexes = raw
            .partitioned_indexes
            .iter()
            .map(|index| ((index.owner.clone(), index.index.clone()), index))
            .collect::<BTreeMap<_, _>>();

        let collection_by_type = raw
            .collection_types
            .iter()
            .map(|collection| {
                (
                    (collection.owner.clone(), collection.type_name.clone()),
                    collection,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut type_keys = BTreeMap::new();
        for user_type in &raw.user_types {
            let schema_key = required(
                schema_keys.get(&user_type.owner),
                format!(
                    "schema key for Oracle type {}.{}",
                    user_type.owner, user_type.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &user_type.owner,
                ObjectKind::UserDefinedType,
                &user_type.name,
                None,
            );
            type_keys.insert(
                (user_type.owner.clone(), user_type.name.clone()),
                key.clone(),
            );
            let inventory_object = required(
                inventory.get(&(
                    user_type.owner.clone(),
                    "TYPE".to_owned(),
                    user_type.name.clone(),
                )),
                format!(
                    "inventory row for Oracle type {}.{}",
                    user_type.owner, user_type.name
                ),
            )?;
            let body_inventory = inventory
                .get(&(
                    user_type.owner.clone(),
                    "TYPE BODY".to_owned(),
                    user_type.name.clone(),
                ))
                .copied();
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(schema_key.clone()),
                name: user_type.name.clone(),
                extension_kind: None,
                definition: Some(oracle_type_definition(user_type)?),
                properties: oracle_type_properties(
                    user_type,
                    inventory_object,
                    body_inventory,
                    collection_by_type
                        .get(&(user_type.owner.clone(), user_type.name.clone()))
                        .copied(),
                ),
            });
        }
        for user_type in &raw.user_types {
            let Some(supertype_owner) = user_type.supertype_owner.as_deref() else {
                continue;
            };
            let supertype_name = user_type.supertype_name.as_deref().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle type {}.{} has no supertype name",
                    user_type.owner, user_type.name
                ))
            })?;
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::InheritsFrom,
                from_key: required(
                    type_keys.get(&(user_type.owner.clone(), user_type.name.clone())),
                    format!("subtype key for {}.{}", user_type.owner, user_type.name),
                )?
                .clone(),
                to_key: required(
                    type_keys.get(&(supertype_owner.to_owned(), supertype_name.to_owned())),
                    format!("supertype key for {supertype_owner}.{supertype_name}"),
                )?
                .clone(),
                ordinal: None,
                properties: BTreeMap::new(),
            });
        }
        for attribute in &raw.type_attributes {
            let parent_key = required(
                type_keys.get(&(attribute.owner.clone(), attribute.type_name.clone())),
                format!(
                    "parent type key for Oracle attribute {}.{}.{}",
                    attribute.owner, attribute.type_name, attribute.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &attribute.owner,
                ObjectKind::Extension,
                &attribute.type_name,
                Some(format!(
                    "attribute:{}:{}",
                    attribute.position, attribute.name
                )),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent_key.clone()),
                name: attribute.name.clone(),
                extension_kind: Some("oracle_type_attribute".to_owned()),
                definition: None,
                properties: oracle_type_attribute_properties(attribute),
            });
            if let Some(owner) = attribute.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), attribute.data_type_name.clone())),
                        format!(
                            "type key for Oracle attribute {}.{}.{}",
                            attribute.owner, attribute.type_name, attribute.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }
        for collection in &raw.collection_types {
            let Some(element_owner) = collection.element_type_owner.as_deref() else {
                continue;
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::UsesType,
                from_key: required(
                    type_keys.get(&(collection.owner.clone(), collection.type_name.clone())),
                    format!(
                        "collection type key for {}.{}",
                        collection.owner, collection.type_name
                    ),
                )?
                .clone(),
                to_key: required(
                    type_keys.get(&(
                        element_owner.to_owned(),
                        collection.element_type_name.clone(),
                    )),
                    format!(
                        "element type key for {}.{}",
                        element_owner, collection.element_type_name
                    ),
                )?
                .clone(),
                ordinal: None,
                properties: BTreeMap::new(),
            });
        }

        let mut type_method_keys = BTreeMap::new();
        for method in &raw.type_methods {
            let parent_key = required(
                type_keys.get(&(method.owner.clone(), method.type_name.clone())),
                format!(
                    "parent type key for Oracle method {}.{}.{}",
                    method.owner, method.type_name, method.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &method.owner,
                ObjectKind::Routine,
                &method.type_name,
                Some(format!("method:{}:{}", method.method_number, method.name)),
            );
            type_method_keys.insert(
                (
                    method.owner.clone(),
                    method.type_name.clone(),
                    method.method_number,
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: method.name.clone(),
                extension_kind: None,
                definition: None,
                properties: oracle_type_method_properties(method),
            });
        }
        for parameter in &raw.type_method_parameters {
            let method_key = required(
                type_method_keys.get(&(
                    parameter.owner.clone(),
                    parameter.type_name.clone(),
                    parameter.method_number,
                )),
                format!(
                    "method key for Oracle parameter {}.{}.{}",
                    parameter.owner, parameter.type_name, parameter.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &parameter.owner,
                ObjectKind::RoutineParameter,
                &parameter.type_name,
                Some(format!(
                    "method:{}:{}#parameter:{}:{}",
                    parameter.method_number,
                    parameter.method_name,
                    parameter.position,
                    parameter.name
                )),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(method_key.clone()),
                name: parameter.name.clone(),
                extension_kind: None,
                definition: None,
                properties: oracle_type_method_parameter_properties(parameter),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: method_key.clone(),
                to_key: key.clone(),
                ordinal: Some(positive_u32(
                    parameter.position + 1,
                    "Oracle type method parameter relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
            if let Some(owner) = parameter.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), parameter.data_type_name.clone())),
                        format!(
                            "type key for Oracle method parameter {}.{}.{}",
                            parameter.owner, parameter.type_name, parameter.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut sequence_keys = BTreeMap::new();
        for sequence in &raw.sequences {
            let schema_key = required(
                schema_keys.get(&sequence.owner),
                format!(
                    "schema key for Oracle sequence {}.{}",
                    sequence.owner, sequence.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &sequence.owner,
                ObjectKind::Sequence,
                &sequence.name,
                None,
            );
            sequence_keys.insert((sequence.owner.clone(), sequence.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    sequence.owner.clone(),
                    "SEQUENCE".to_owned(),
                    sequence.name.clone(),
                )),
                format!(
                    "inventory row for Oracle sequence {}.{}",
                    sequence.owner, sequence.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_optional_string(&mut properties, "minimum", sequence.min_value.as_deref());
            insert_optional_string(&mut properties, "maximum", sequence.max_value.as_deref());
            insert_string(&mut properties, "increment", &sequence.increment_by);
            insert_string(&mut properties, "cache_size", &sequence.cache_size);
            insert_optional_string(&mut properties, "cycle", sequence.cycle.as_deref());
            insert_optional_string(&mut properties, "ordered", sequence.ordered.as_deref());
            insert_optional_string(&mut properties, "scale", sequence.scale.as_deref());
            insert_optional_string(&mut properties, "extend", sequence.extend.as_deref());
            insert_optional_string(&mut properties, "sharded", sequence.sharded.as_deref());
            insert_optional_string(&mut properties, "session", sequence.session.as_deref());
            insert_optional_string(
                &mut properties,
                "keep_value",
                sequence.keep_value.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: sequence.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }

        let materialized_view_names = raw
            .materialized_views
            .iter()
            .map(|view| (view.owner.clone(), view.name.clone()))
            .collect::<BTreeSet<_>>();
        let mut materialized_view_keys = BTreeMap::new();
        for view in &raw.materialized_views {
            let schema_key = required(
                schema_keys.get(&view.owner),
                format!(
                    "schema key for Oracle materialized view {}.{}",
                    view.owner, view.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &view.owner,
                ObjectKind::MaterializedView,
                &view.name,
                None,
            );
            materialized_view_keys.insert((view.owner.clone(), view.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    view.owner.clone(),
                    "MATERIALIZED VIEW".to_owned(),
                    view.name.clone(),
                )),
                format!(
                    "inventory row for Oracle materialized view {}.{}",
                    view.owner, view.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            let storage_object = required(
                inventory.get(&(
                    view.owner.clone(),
                    "TABLE".to_owned(),
                    view.container_name.clone(),
                )),
                format!(
                    "storage inventory row for Oracle materialized view {}.{}",
                    view.owner, view.name
                ),
            )?;
            insert_i64(
                &mut properties,
                "storage_object_id",
                storage_object.object_id,
            );
            insert_optional_i64(
                &mut properties,
                "storage_data_object_id",
                storage_object.data_object_id,
            );
            insert_string(
                &mut properties,
                "storage_object_status",
                &storage_object.status,
            );
            insert_bool(
                &mut properties,
                "storage_generated",
                storage_object.generated,
            );
            insert_string(&mut properties, "container_name", &view.container_name);
            insert_optional_i64(&mut properties, "query_length", view.query_length);
            insert_optional_string(&mut properties, "updatable", view.updatable.as_deref());
            insert_optional_string(
                &mut properties,
                "rewrite_enabled",
                view.rewrite_enabled.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "rewrite_capability",
                view.rewrite_capability.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "refresh_mode",
                view.refresh_mode.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "refresh_method",
                view.refresh_method.as_deref(),
            );
            insert_optional_string(&mut properties, "build_mode", view.build_mode.as_deref());
            insert_optional_string(
                &mut properties,
                "fast_refreshable",
                view.fast_refreshable.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "compile_state",
                view.compile_state.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "use_no_index",
                view.use_no_index.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "segment_created",
                view.segment_created.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "default_collation",
                view.default_collation.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "on_query_computation",
                view.on_query_computation.as_deref(),
            );
            insert_optional_string(&mut properties, "automatic", view.automatic.as_deref());
            insert_optional_string(
                &mut properties,
                "concurrent_refresh",
                view.concurrent_refresh.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: view.name.clone(),
                extension_kind: None,
                definition: view.definition.clone(),
                properties,
            });
        }

        let mut tables = Vec::new();
        let mut table_keys = BTreeMap::new();
        for table in &raw.tables {
            if materialized_view_names.contains(&(table.owner.clone(), table.name.clone())) {
                continue;
            }
            let schema_key = required(
                schema_keys.get(&table.owner),
                format!("schema key for Oracle table {}.{}", table.owner, table.name),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &table.owner,
                ObjectKind::Table,
                &table.name,
                None,
            );
            table_keys.insert((table.owner.clone(), table.name.clone()), key.clone());
            tables.push(TableObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: table.name.clone(),
                kind: if table.partitioned {
                    TableKind::Partitioned
                } else if table.temporary {
                    TableKind::Temporary
                } else {
                    TableKind::BaseTable
                },
            });
            let inventory_object = required(
                inventory.get(&(table.owner.clone(), "TABLE".to_owned(), table.name.clone())),
                format!(
                    "inventory row for Oracle table {}.{}",
                    table.owner, table.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_string(&mut properties, "table_status", &table.status);
            insert_bool(&mut properties, "temporary", table.temporary);
            insert_bool(&mut properties, "read_only", table.read_only);
            insert_bool(&mut properties, "has_identity", table.has_identity);
            insert_optional_string(&mut properties, "duration", table.duration.as_deref());
            if let Some(partitioning) =
                partitioned_tables.get(&(table.owner.clone(), table.name.clone()))
            {
                add_oracle_partitioned_table_properties(
                    &mut properties,
                    partitioning,
                    &raw.partition_key_columns,
                );
            }
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties,
            });
        }

        let mut views = Vec::new();
        let mut view_keys = BTreeMap::new();
        let mut view_positions = BTreeMap::new();
        for view in &raw.views {
            let schema_key = required(
                schema_keys.get(&view.owner),
                format!("schema key for Oracle view {}.{}", view.owner, view.name),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &view.owner,
                ObjectKind::View,
                &view.name,
                None,
            );
            view_keys.insert((view.owner.clone(), view.name.clone()), key.clone());
            view_positions.insert((view.owner.clone(), view.name.clone()), views.len());
            views.push(ViewObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: view.name.clone(),
                definition: view.definition.clone(),
                depends_on: Vec::new(),
            });
            let inventory_object = required(
                inventory.get(&(view.owner.clone(), "VIEW".to_owned(), view.name.clone())),
                format!("inventory row for Oracle view {}.{}", view.owner, view.name),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_optional_i64(&mut properties, "text_length", view.text_length);
            insert_optional_string(&mut properties, "editioning", view.editioning.as_deref());
            insert_optional_string(&mut properties, "read_only", view.read_only.as_deref());
            insert_optional_string(
                &mut properties,
                "container_data",
                view.container_data.as_deref(),
            );
            insert_optional_string(&mut properties, "bequeath", view.bequeath.as_deref());
            insert_optional_string(
                &mut properties,
                "default_collation",
                view.default_collation.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "has_sensitive_column",
                view.has_sensitive_column.as_deref(),
            );
            insert_optional_string(&mut properties, "admit_null", view.admit_null.as_deref());
            insert_optional_string(
                &mut properties,
                "pdb_local_only",
                view.pdb_local_only.as_deref(),
            );
            insert_optional_string(
                &mut properties,
                "duality_view",
                view.duality_view.as_deref(),
            );
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties,
            });
        }

        for column in &raw.view_columns {
            let view_key = required(
                view_keys.get(&(column.owner.clone(), column.table.clone())),
                format!(
                    "view key for Oracle output column {}.{}.{}",
                    column.owner, column.table, column.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &column.owner,
                ObjectKind::ViewColumn,
                &column.table,
                Some(column.name.clone()),
            );
            let mut properties = oracle_column_properties(column);
            insert_i64(
                &mut properties,
                "ordinal_position",
                i64::from(positive_u32(
                    column.internal_column_id,
                    "Oracle view-column ordinal",
                )?),
            );
            insert_string(
                &mut properties,
                "data_type",
                format_oracle_data_type(column),
            );
            insert_bool(&mut properties, "nullable", column.nullable);
            insert_optional_string(
                &mut properties,
                "default_value",
                column.default_value.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(view_key.clone()),
                name: column.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            if let Some(owner) = column.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), column.data_type.clone())),
                        format!(
                            "type key for Oracle view column {}.{}.{}",
                            column.owner, column.table, column.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut materialized_view_column_keys = BTreeMap::new();
        for column in raw.columns.iter().filter(|column| {
            materialized_view_names.contains(&(column.owner.clone(), column.table.clone()))
        }) {
            let view_key = required(
                materialized_view_keys.get(&(column.owner.clone(), column.table.clone())),
                format!(
                    "materialized-view key for Oracle output column {}.{}.{}",
                    column.owner, column.table, column.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &column.owner,
                ObjectKind::ViewColumn,
                &column.table,
                Some(column.name.clone()),
            );
            materialized_view_column_keys.insert(
                (
                    column.owner.clone(),
                    column.table.clone(),
                    column.name.clone(),
                ),
                key.clone(),
            );
            let mut properties = oracle_column_properties(column);
            insert_i64(
                &mut properties,
                "ordinal_position",
                i64::from(positive_u32(
                    column.internal_column_id,
                    "Oracle materialized-view column ordinal",
                )?),
            );
            insert_string(
                &mut properties,
                "data_type",
                format_oracle_data_type(column),
            );
            insert_bool(&mut properties, "nullable", column.nullable);
            insert_optional_string(
                &mut properties,
                "default_value",
                column.default_value.as_deref(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(view_key.clone()),
                name: column.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
            if let Some(owner) = column.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), column.data_type.clone())),
                        format!(
                            "type key for Oracle materialized-view column {}.{}.{}",
                            column.owner, column.table, column.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut routines = Vec::new();
        let mut routine_keys = BTreeMap::new();
        let mut routine_positions = BTreeMap::new();
        for routine in &raw.routines {
            let schema_key = required(
                schema_keys.get(&routine.owner),
                format!(
                    "schema key for Oracle routine {}.{}",
                    routine.owner, routine.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &routine.owner,
                ObjectKind::Routine,
                &routine.name,
                None,
            );
            let identity = (
                routine.owner.clone(),
                routine.name.clone(),
                routine.object_type.clone(),
            );
            routine_keys.insert(identity.clone(), key.clone());
            routine_positions.insert(identity, routines.len());
            routines.push(RoutineObject {
                key: key.clone(),
                schema_key: schema_key.clone(),
                name: routine.name.clone(),
                kind: match routine.object_type.as_str() {
                    "FUNCTION" => RoutineKind::Function,
                    "PROCEDURE" => RoutineKind::Procedure,
                    other => {
                        return Err(CatalogError::Mapping(format!(
                            "unmapped Oracle routine type '{other}'"
                        )));
                    }
                },
                definition: routine.definition.clone(),
                depends_on: Vec::new(),
            });
            let inventory_object = required(
                inventory.get(&(
                    routine.owner.clone(),
                    routine.object_type.clone(),
                    routine.name.clone(),
                )),
                format!(
                    "inventory row for Oracle routine {}.{}",
                    routine.owner, routine.name
                ),
            )?;
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: oracle_routine_properties(routine, inventory_object),
            });
        }
        for argument in &raw.routine_arguments {
            let routine = raw
                .routines
                .iter()
                .find(|routine| routine.owner == argument.owner && routine.name == argument.routine)
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "parent routine for Oracle argument {}.{}",
                        argument.owner, argument.routine
                    ))
                })?;
            let routine_key = required(
                routine_keys.get(&(
                    routine.owner.clone(),
                    routine.name.clone(),
                    routine.object_type.clone(),
                )),
                format!(
                    "parent key for Oracle argument {}.{}",
                    argument.owner, argument.routine
                ),
            )?;
            let display_name = if argument.position == 0 {
                "RETURN".to_owned()
            } else {
                argument
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("ARGUMENT_{}", argument.position))
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &argument.owner,
                ObjectKind::RoutineParameter,
                &argument.routine,
                Some(format!("{}:{display_name}", argument.sequence)),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(routine_key.clone()),
                name: display_name,
                extension_kind: None,
                definition: argument.default_value.clone(),
                properties: oracle_routine_argument_properties(argument),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: routine_key.clone(),
                to_key: key.clone(),
                ordinal: Some(positive_u32(
                    argument.sequence,
                    "Oracle routine argument relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
            if let (Some(owner), Some(name)) = (
                argument.type_owner.as_deref(),
                argument.type_name.as_deref(),
            ) {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), name.to_owned())),
                        format!(
                            "type key for Oracle routine argument {}.{}",
                            argument.owner, argument.routine
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut package_keys = BTreeMap::new();
        for package in &raw.packages {
            let schema_key = required(
                schema_keys.get(&package.owner),
                format!(
                    "schema key for Oracle package {}.{}",
                    package.owner, package.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &package.owner,
                ObjectKind::Package,
                &package.name,
                None,
            );
            package_keys.insert((package.owner.clone(), package.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    package.owner.clone(),
                    "PACKAGE".to_owned(),
                    package.name.clone(),
                )),
                format!(
                    "inventory row for Oracle package {}.{}",
                    package.owner, package.name
                ),
            )?;
            let body_inventory = inventory
                .get(&(
                    package.owner.clone(),
                    "PACKAGE BODY".to_owned(),
                    package.name.clone(),
                ))
                .copied();
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: package.name.clone(),
                extension_kind: None,
                definition: Some(oracle_package_definition(package)?),
                properties: oracle_package_properties(package, inventory_object, body_inventory),
            });
        }
        let package_arguments_by_routine = raw.package_arguments.iter().fold(
            BTreeMap::<(String, String, i64), Vec<&RawRoutineArgument>>::new(),
            |mut map, argument| {
                if let Some(package) = argument.package_name.as_deref() {
                    map.entry((
                        argument.owner.clone(),
                        package.to_owned(),
                        argument.subprogram_id,
                    ))
                    .or_default()
                    .push(argument);
                }
                map
            },
        );
        let mut package_routine_keys = BTreeMap::new();
        let mut package_routine_signatures = BTreeMap::new();
        for routine in &raw.package_routines {
            let package_key = required(
                package_keys.get(&(routine.owner.clone(), routine.package.clone())),
                format!(
                    "package key for Oracle routine {}.{}.{}",
                    routine.owner, routine.package, routine.name
                ),
            )?;
            let arguments = package_arguments_by_routine
                .get(&(
                    routine.owner.clone(),
                    routine.package.clone(),
                    routine.subprogram_id,
                ))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let signature = oracle_package_routine_signature(routine, arguments)?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &routine.owner,
                ObjectKind::Routine,
                &routine.package,
                Some(signature.clone()),
            );
            let identity = (
                routine.owner.clone(),
                routine.package.clone(),
                routine.subprogram_id,
            );
            package_routine_keys.insert(identity.clone(), key.clone());
            package_routine_signatures.insert(identity, signature.clone());
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(package_key.clone()),
                name: routine.name.clone(),
                extension_kind: None,
                definition: None,
                properties: oracle_package_routine_properties(routine, &signature),
            });
        }
        for argument in &raw.package_arguments {
            let package_name = argument.package_name.as_deref().ok_or_else(|| {
                CatalogError::Mapping(format!(
                    "Oracle package argument {}.{} has no package",
                    argument.owner, argument.routine
                ))
            })?;
            let identity = (
                argument.owner.clone(),
                package_name.to_owned(),
                argument.subprogram_id,
            );
            let routine_key = required(
                package_routine_keys.get(&identity),
                format!(
                    "package routine key for Oracle argument {}.{}.{}",
                    argument.owner, package_name, argument.routine
                ),
            )?;
            let signature = required(
                package_routine_signatures.get(&identity),
                format!(
                    "package routine signature for Oracle argument {}.{}.{}",
                    argument.owner, package_name, argument.routine
                ),
            )?;
            let display_name = if argument.position == 0 {
                "RETURN".to_owned()
            } else {
                argument
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("ARGUMENT_{}", argument.position))
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &argument.owner,
                ObjectKind::RoutineParameter,
                package_name,
                Some(format!("{signature}#{}:{display_name}", argument.sequence)),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(routine_key.clone()),
                name: display_name,
                extension_kind: None,
                definition: argument.default_value.clone(),
                properties: oracle_routine_argument_properties(argument),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::HasParameter,
                from_key: routine_key.clone(),
                to_key: key.clone(),
                ordinal: Some(positive_u32(
                    argument.sequence,
                    "Oracle package argument relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
            if let (Some(owner), Some(name)) = (
                argument.type_owner.as_deref(),
                argument.type_name.as_deref(),
            ) {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), name.to_owned())),
                        format!(
                            "type key for Oracle package argument {}.{}.{}",
                            argument.owner, package_name, argument.routine
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let mut synonym_keys = BTreeMap::new();
        for synonym in &raw.synonyms {
            let schema_key = required(
                schema_keys.get(&synonym.owner),
                format!(
                    "schema key for Oracle synonym {}.{}",
                    synonym.owner, synonym.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &synonym.owner,
                ObjectKind::Synonym,
                &synonym.name,
                None,
            );
            synonym_keys.insert((synonym.owner.clone(), synonym.name.clone()), key.clone());
            let inventory_object = required(
                inventory.get(&(
                    synonym.owner.clone(),
                    "SYNONYM".to_owned(),
                    synonym.name.clone(),
                )),
                format!(
                    "inventory row for Oracle synonym {}.{}",
                    synonym.owner, synonym.name
                ),
            )?;
            let mut properties = inventory_properties(inventory_object);
            insert_string(&mut properties, "target_owner", &synonym.target_owner);
            insert_string(&mut properties, "target_name", &synonym.target_name);
            insert_optional_string(
                &mut properties,
                "database_link",
                synonym.database_link.as_deref(),
            );
            insert_i64(
                &mut properties,
                "origin_container_id",
                synonym.origin_container_id,
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(schema_key.clone()),
                name: synonym.name.clone(),
                extension_kind: None,
                definition: None,
                properties,
            });
        }
        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "SYNONYM")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            let source_key = required(
                synonym_keys.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "source key for Oracle synonym dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let target_key = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "table target for Oracle synonym dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "view target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "materialized-view target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "sequence target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                        dependency.referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "package target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SYNONYM" => required(
                    synonym_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "synonym target for Oracle dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "type target for Oracle synonym dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle synonym target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::SynonymFor,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([(
                    "oracle_dependency_type".to_owned(),
                    MetadataValue::String(dependency.dependency_type.clone()),
                )]),
            });
        }

        for dependency in &raw.dependencies {
            if dependency.object_type != "VIEW" || dependency.referenced_owner_oracle_maintained {
                continue;
            }
            let source_position = required(
                view_positions.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "view position for Oracle dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let target_key = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "table target for Oracle view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "view target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "materialized-view target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "sequence target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                        dependency.referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "package target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "type target for Oracle view dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle view dependency target type '{other}'"
                    )));
                }
            };
            if dependency.referenced_type == "TYPE" {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: views[*source_position].key.clone(),
                    to_key: target_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::from([(
                        "oracle_dependency_type".to_owned(),
                        MetadataValue::String(dependency.dependency_type.clone()),
                    )]),
                });
            } else {
                views[*source_position].depends_on.push(target_key.clone());
            }
        }

        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "MATERIALIZED VIEW")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            if dependency.referenced_type == "TABLE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name
            {
                continue;
            }
            let source_key = required(
                materialized_view_keys.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "source key for Oracle materialized-view dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let (target_key, relationship_kind) = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => (key, MetadataRelationshipKind::DependsOn),
                    None => (
                        required(
                            table_keys.get(&(
                                dependency.referenced_owner.clone(),
                                dependency.referenced_name.clone(),
                            )),
                            format!(
                                "table target for Oracle materialized-view dependency {}.{}",
                                dependency.referenced_owner, dependency.referenced_name
                            ),
                        )?,
                        MetadataRelationshipKind::Materializes,
                    ),
                },
                "VIEW" => (
                    required(
                        view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "view target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::Materializes,
                ),
                "MATERIALIZED VIEW" => (
                    required(
                        materialized_view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "materialized-view target for Oracle dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "SEQUENCE" => (
                    required(
                        sequence_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "sequence target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "FUNCTION" | "PROCEDURE" => (
                    required(
                        routine_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                            dependency.referenced_type.clone(),
                        )),
                        format!(
                            "routine target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "PACKAGE" => (
                    required(
                        package_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "package target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "TYPE" => (
                    required(
                        type_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "type target for Oracle materialized-view dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle materialized-view dependency target type '{other}'"
                    )));
                }
            };
            let mut properties = BTreeMap::new();
            insert_string(
                &mut properties,
                "oracle_dependency_type",
                &dependency.dependency_type,
            );
            metadata.relationships.push(MetadataRelationship {
                kind: relationship_kind,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties,
            });
        }

        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| {
                matches!(dependency.object_type.as_str(), "FUNCTION" | "PROCEDURE")
            })
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            let source_identity = (
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.object_type.clone(),
            );
            let source_position = required(
                routine_positions.get(&source_identity),
                format!(
                    "source position for Oracle routine dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let target_key = match dependency.referenced_type.as_str() {
                "TABLE" => match materialized_view_keys.get(&(
                    dependency.referenced_owner.clone(),
                    dependency.referenced_name.clone(),
                )) {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "table target for Oracle routine dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "view target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "materialized-view target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "sequence target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                        dependency.referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "package target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )),
                    format!(
                        "type target for Oracle routine dependency {}.{}",
                        dependency.referenced_owner, dependency.referenced_name
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle routine dependency target type '{other}'"
                    )));
                }
            };
            if dependency.referenced_type == "TYPE" {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::DependsOn,
                    from_key: routines[*source_position].key.clone(),
                    to_key: target_key.clone(),
                    ordinal: None,
                    properties: BTreeMap::from([(
                        "oracle_dependency_type".to_owned(),
                        MetadataValue::String(dependency.dependency_type.clone()),
                    )]),
                });
            } else {
                routines[*source_position]
                    .depends_on
                    .push(target_key.clone());
            }
        }

        for (identity, evidence) in oracle_package_dependency_groups(&raw.dependencies) {
            let (owner, package, referenced_owner, referenced_name, referenced_type) = identity;
            let source_key = required(
                package_keys.get(&(owner.clone(), package.clone())),
                format!("source key for Oracle package dependency {owner}.{package}"),
            )?;
            let target_key = match referenced_type.as_str() {
                "TABLE" => match materialized_view_keys
                    .get(&(referenced_owner.clone(), referenced_name.clone()))
                {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                        format!(
                            "table target for Oracle package dependency {referenced_owner}.{referenced_name}"
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "view target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys
                        .get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "materialized-view target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "sequence target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        referenced_owner.clone(),
                        referenced_name.clone(),
                        referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "package target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "type target for Oracle package dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle package dependency target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::DependsOn,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([
                    (
                        "oracle_source_object_types".to_owned(),
                        MetadataValue::StringList(
                            evidence.source_object_types.into_iter().collect(),
                        ),
                    ),
                    (
                        "oracle_dependency_types".to_owned(),
                        MetadataValue::StringList(evidence.dependency_types.into_iter().collect()),
                    ),
                ]),
            });
        }

        for (identity, evidence) in oracle_type_dependency_groups(&raw.dependencies) {
            let (owner, type_name, referenced_owner, referenced_name, referenced_type) = identity;
            let source_key = required(
                type_keys.get(&(owner.clone(), type_name.clone())),
                format!("source key for Oracle type dependency {owner}.{type_name}"),
            )?;
            let target_key = match referenced_type.as_str() {
                "TABLE" => match materialized_view_keys
                    .get(&(referenced_owner.clone(), referenced_name.clone()))
                {
                    Some(key) => key,
                    None => required(
                        table_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                        format!(
                            "table target for Oracle type dependency {referenced_owner}.{referenced_name}"
                        ),
                    )?,
                },
                "VIEW" => required(
                    view_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "view target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "MATERIALIZED VIEW" => required(
                    materialized_view_keys
                        .get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "materialized-view target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "SEQUENCE" => required(
                    sequence_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "sequence target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "FUNCTION" | "PROCEDURE" => required(
                    routine_keys.get(&(
                        referenced_owner.clone(),
                        referenced_name.clone(),
                        referenced_type.clone(),
                    )),
                    format!(
                        "routine target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "PACKAGE" => required(
                    package_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "package target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "SYNONYM" => required(
                    synonym_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "synonym target for Oracle type dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                "TYPE" => required(
                    type_keys.get(&(referenced_owner.clone(), referenced_name.clone())),
                    format!(
                        "type target for Oracle dependency {referenced_owner}.{referenced_name}"
                    ),
                )?,
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle type dependency target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::DependsOn,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([
                    (
                        "oracle_source_object_types".to_owned(),
                        MetadataValue::StringList(
                            evidence.source_object_types.into_iter().collect(),
                        ),
                    ),
                    (
                        "oracle_dependency_types".to_owned(),
                        MetadataValue::StringList(evidence.dependency_types.into_iter().collect()),
                    ),
                ]),
            });
        }

        let identities = raw
            .identity_columns
            .iter()
            .map(|identity| {
                (
                    (
                        identity.owner.clone(),
                        identity.table.clone(),
                        identity.column.clone(),
                    ),
                    identity,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut columns = Vec::new();
        let mut column_keys = BTreeMap::new();
        for column in &raw.columns {
            if materialized_view_names.contains(&(column.owner.clone(), column.table.clone())) {
                continue;
            }
            let table_key = required(
                table_keys.get(&(column.owner.clone(), column.table.clone())),
                format!(
                    "table key for Oracle column {}.{}.{}",
                    column.owner, column.table, column.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &column.owner,
                ObjectKind::Column,
                &column.table,
                Some(column.name.clone()),
            );
            column_keys.insert(
                (
                    column.owner.clone(),
                    column.table.clone(),
                    column.name.clone(),
                ),
                key.clone(),
            );
            columns.push(ColumnObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: column.name.clone(),
                ordinal_position: positive_u32(
                    column.internal_column_id,
                    "Oracle internal column ordinal",
                )?,
                data_type: format_oracle_data_type(column),
                is_nullable: column.nullable,
                default_value: column.default_value.clone(),
                is_generated: column.virtual_column
                    || column.hidden
                    || !column.user_generated
                    || column.identity,
            });
            let mut properties = oracle_column_properties(column);
            if let Some(identity) = identities.get(&(
                column.owner.clone(),
                column.table.clone(),
                column.name.clone(),
            )) {
                insert_optional_string(
                    &mut properties,
                    "identity_generation_type",
                    identity.generation_type.as_deref(),
                );
                insert_optional_string(
                    &mut properties,
                    "identity_options",
                    identity.options.as_deref(),
                );
                let sequence_key = required(
                    sequence_keys.get(&(identity.owner.clone(), identity.sequence_name.clone())),
                    format!(
                        "identity sequence key {}.{}",
                        identity.owner, identity.sequence_name
                    ),
                )?;
                let mut relationship_properties = BTreeMap::new();
                insert_optional_string(
                    &mut relationship_properties,
                    "generation_type",
                    identity.generation_type.as_deref(),
                );
                insert_optional_string(
                    &mut relationship_properties,
                    "identity_options",
                    identity.options.as_deref(),
                );
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesSequence,
                    from_key: key.clone(),
                    to_key: sequence_key.clone(),
                    ordinal: None,
                    properties: relationship_properties,
                });
            }
            metadata.annotations.push(ObjectAnnotation {
                object_key: key.clone(),
                definition: None,
                properties,
            });
            if let Some(owner) = column.data_type_owner.as_deref() {
                metadata.relationships.push(MetadataRelationship {
                    kind: MetadataRelationshipKind::UsesType,
                    from_key: key,
                    to_key: required(
                        type_keys.get(&(owner.to_owned(), column.data_type.clone())),
                        format!(
                            "type key for Oracle column {}.{}.{}",
                            column.owner, column.table, column.name
                        ),
                    )?
                    .clone(),
                    ordinal: None,
                    properties: BTreeMap::new(),
                });
            }
        }

        let constraint_by_identity = raw
            .constraints
            .iter()
            .map(|constraint| {
                (
                    (constraint.owner.clone(), constraint.name.clone()),
                    constraint,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut constraints = Vec::new();
        for constraint in &raw.constraints {
            if let Some(materialized_view_key) =
                materialized_view_keys.get(&(constraint.owner.clone(), constraint.table.clone()))
            {
                let object_kind = match constraint.constraint_type.as_str() {
                    "P" => ObjectKind::PrimaryKey,
                    "U" => ObjectKind::UniqueConstraint,
                    "C" => ObjectKind::CheckConstraint,
                    other => {
                        return Err(CatalogError::Mapping(format!(
                            "unmapped Oracle materialized-view constraint type '{other}'"
                        )));
                    }
                };
                let key = oracle_key(
                    self.connection_alias,
                    &database_name,
                    &constraint.owner,
                    object_kind,
                    &constraint.table,
                    Some(constraint.name.clone()),
                );
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(materialized_view_key.clone()),
                    name: constraint.name.clone(),
                    extension_kind: None,
                    definition: constraint.search_condition.clone(),
                    properties: constraint_properties(constraint),
                });
                for column in &constraint.columns {
                    let column_key = required(
                        materialized_view_column_keys.get(&(
                            constraint.owner.clone(),
                            constraint.table.clone(),
                            column.name.clone(),
                        )),
                        format!(
                            "column {} for Oracle materialized-view constraint {}.{}",
                            column.name, constraint.owner, constraint.name
                        ),
                    )?;
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::Extension(
                            "oracle_constraint_column".to_owned(),
                        ),
                        from_key: key.clone(),
                        to_key: column_key.clone(),
                        ordinal: column
                            .position
                            .map(|position| {
                                positive_u32(
                                    position,
                                    "Oracle materialized-view constraint ordinal",
                                )
                            })
                            .transpose()?,
                        properties: BTreeMap::new(),
                    });
                }
                continue;
            }
            let table_key = required(
                table_keys.get(&(constraint.owner.clone(), constraint.table.clone())),
                format!(
                    "table key for Oracle constraint {}.{}",
                    constraint.owner, constraint.name
                ),
            )?;
            let (kind, object_kind) = match constraint.constraint_type.as_str() {
                "P" => (ConstraintKind::PrimaryKey, ObjectKind::PrimaryKey),
                "U" => (ConstraintKind::Unique, ObjectKind::UniqueConstraint),
                "R" => (ConstraintKind::ForeignKey, ObjectKind::ForeignKey),
                "C" => (ConstraintKind::Check, ObjectKind::CheckConstraint),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle constraint type '{other}' for {}.{}",
                        constraint.owner, constraint.name
                    )));
                }
            };
            let local_columns = resolve_named_columns(
                &constraint.owner,
                &constraint.table,
                &constraint.columns,
                &column_keys,
                &constraint.name,
            )?;
            let (referenced_table_key, referenced_columns) = if kind == ConstraintKind::ForeignKey {
                let referenced_owner = constraint.referenced_owner.as_deref().ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "foreign key {}.{} has no referenced owner",
                        constraint.owner, constraint.name
                    ))
                })?;
                let referenced_name =
                    constraint.referenced_constraint.as_deref().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "foreign key {}.{} has no referenced constraint",
                            constraint.owner, constraint.name
                        ))
                    })?;
                let referenced = required(
                    constraint_by_identity
                        .get(&(referenced_owner.to_owned(), referenced_name.to_owned())),
                    format!(
                        "referenced Oracle constraint {}.{}",
                        referenced_owner, referenced_name
                    ),
                )?;
                let referenced_table = required(
                    table_keys.get(&(referenced.owner.clone(), referenced.table.clone())),
                    format!(
                        "referenced Oracle table {}.{}",
                        referenced.owner, referenced.table
                    ),
                )?;
                let referenced_columns = resolve_named_columns(
                    &referenced.owner,
                    &referenced.table,
                    &referenced.columns,
                    &column_keys,
                    &constraint.name,
                )?;
                (Some(referenced_table.clone()), referenced_columns)
            } else {
                (None, Vec::new())
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &constraint.owner,
                object_kind,
                &constraint.table,
                Some(constraint.name.clone()),
            );
            constraints.push(ConstraintObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: constraint.name.clone(),
                kind,
                columns: local_columns,
                referenced_table_key,
                referenced_columns,
                expression: (kind == ConstraintKind::Check)
                    .then(|| constraint.search_condition.clone())
                    .flatten(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: None,
                properties: constraint_properties(constraint),
            });
        }

        let primary_indexes = raw
            .constraints
            .iter()
            .filter(|constraint| constraint.constraint_type == "P")
            .filter_map(|constraint| {
                Some((
                    constraint.index_owner.clone()?,
                    constraint.index_name.clone()?,
                ))
            })
            .collect::<BTreeSet<_>>();
        let mut indexes = Vec::new();
        let mut index_keys = BTreeMap::new();
        for index in &raw.indexes {
            let expression = oracle_index_expression(index);
            let inventory_object = required(
                inventory.get(&(index.owner.clone(), "INDEX".to_owned(), index.name.clone())),
                format!(
                    "inventory row for Oracle index {}.{}",
                    index.owner, index.name
                ),
            )?;
            let mut properties = oracle_index_properties(index, inventory_object);
            if let Some(partitioning) =
                partitioned_indexes.get(&(index.owner.clone(), index.name.clone()))
            {
                add_oracle_partitioned_index_properties(
                    &mut properties,
                    partitioning,
                    &raw.partition_key_columns,
                );
            }
            if let Some(materialized_view_key) =
                materialized_view_keys.get(&(index.table_owner.clone(), index.table.clone()))
            {
                let key = oracle_key(
                    self.connection_alias,
                    &database_name,
                    &index.table_owner,
                    ObjectKind::Index,
                    &index.table,
                    Some(index.name.clone()),
                );
                index_keys.insert((index.owner.clone(), index.name.clone()), key.clone());
                metadata.objects.push(MetadataObject {
                    key: key.clone(),
                    parent_key: Some(materialized_view_key.clone()),
                    name: index.name.clone(),
                    extension_kind: None,
                    definition: expression,
                    properties,
                });
                for column in index
                    .columns
                    .iter()
                    .filter(|column| column.expression.is_none())
                {
                    let column_key = required(
                        materialized_view_column_keys.get(&(
                            index.table_owner.clone(),
                            index.table.clone(),
                            column.name.clone(),
                        )),
                        format!(
                            "column {} for Oracle materialized-view index {}.{}",
                            column.name, index.owner, index.name
                        ),
                    )?;
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::IncludesColumn,
                        from_key: key.clone(),
                        to_key: column_key.clone(),
                        ordinal: Some(positive_u32(
                            column.position,
                            "Oracle materialized-view index ordinal",
                        )?),
                        properties: BTreeMap::from([(
                            "descending".to_owned(),
                            MetadataValue::Boolean(column.descending),
                        )]),
                    });
                }
                continue;
            }
            let table_key = required(
                table_keys.get(&(index.table_owner.clone(), index.table.clone())),
                format!("table key for Oracle index {}.{}", index.owner, index.name),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &index.table_owner,
                ObjectKind::Index,
                &index.table,
                Some(index.name.clone()),
            );
            index_keys.insert((index.owner.clone(), index.name.clone()), key.clone());
            let direct_columns = index
                .columns
                .iter()
                .filter(|column| column.expression.is_none())
                .cloned()
                .collect::<Vec<_>>();
            let index_columns = resolve_named_columns(
                &index.table_owner,
                &index.table,
                &direct_columns,
                &column_keys,
                &index.name,
            )?;
            indexes.push(IndexObject {
                key: key.clone(),
                table_key: table_key.clone(),
                name: index.name.clone(),
                columns: index_columns,
                is_unique: index.unique,
                is_primary: primary_indexes.contains(&(index.owner.clone(), index.name.clone())),
                predicate: None,
                expression: expression.clone(),
            });
            metadata.annotations.push(ObjectAnnotation {
                object_key: key,
                definition: expression,
                properties,
            });
        }

        let mut table_partition_keys = BTreeMap::new();
        for partition in &raw.table_partitions {
            let parent_key = match materialized_view_keys
                .get(&(partition.owner.clone(), partition.table.clone()))
            {
                Some(key) => key,
                None => required(
                    table_keys.get(&(partition.owner.clone(), partition.table.clone())),
                    format!(
                        "parent table for Oracle partition {}.{}.{}",
                        partition.owner, partition.table, partition.name
                    ),
                )?,
            };
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &partition.owner,
                ObjectKind::Extension,
                &partition.table,
                Some(format!("partition:{}", partition.name)),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "TABLE PARTITION".to_owned(),
                    partition.table.clone(),
                    partition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle table partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            table_partition_keys.insert(
                (
                    partition.owner.clone(),
                    partition.table.clone(),
                    partition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: partition.name.clone(),
                extension_kind: Some("oracle_table_partition".to_owned()),
                definition: partition.high_value.clone(),
                properties: oracle_table_partition_properties(partition, inventory_object),
            });
        }
        let mut table_subpartition_keys = BTreeMap::new();
        for subpartition in &raw.table_subpartitions {
            let parent_key = required(
                table_partition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.partition.clone(),
                )),
                format!(
                    "parent partition for Oracle table subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &subpartition.owner,
                ObjectKind::Extension,
                &subpartition.table,
                Some(format!(
                    "partition:{}:subpartition:{}",
                    subpartition.partition, subpartition.name
                )),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "TABLE SUBPARTITION".to_owned(),
                    subpartition.table.clone(),
                    subpartition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle table subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            table_subpartition_keys.insert(
                (
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: subpartition.name.clone(),
                extension_kind: Some("oracle_table_subpartition".to_owned()),
                definition: subpartition.high_value.clone(),
                properties: oracle_table_subpartition_properties(subpartition, inventory_object),
            });
        }

        let mut index_partition_keys = BTreeMap::new();
        for partition in &raw.index_partitions {
            let parent_key = required(
                index_keys.get(&(partition.owner.clone(), partition.index.clone())),
                format!(
                    "parent index for Oracle partition {}.{}.{}",
                    partition.owner, partition.index, partition.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &partition.owner,
                ObjectKind::Extension,
                &parent_key.object_name,
                Some(format!(
                    "index:{}:partition:{}",
                    partition.index, partition.name
                )),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "INDEX PARTITION".to_owned(),
                    partition.index.clone(),
                    partition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle index partition {}.{}.{}",
                    partition.owner, partition.index, partition.name
                ),
            )?;
            index_partition_keys.insert(
                (
                    partition.owner.clone(),
                    partition.index.clone(),
                    partition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: partition.name.clone(),
                extension_kind: Some("oracle_index_partition".to_owned()),
                definition: partition.high_value.clone(),
                properties: oracle_index_partition_properties(partition, inventory_object),
            });
        }
        for subpartition in &raw.index_subpartitions {
            let parent_key = required(
                index_partition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.index.clone(),
                    subpartition.partition.clone(),
                )),
                format!(
                    "parent partition for Oracle index subpartition {}.{}.{}",
                    subpartition.owner, subpartition.index, subpartition.name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &subpartition.owner,
                ObjectKind::Extension,
                &parent_key.object_name,
                Some(format!(
                    "index:{}:partition:{}:subpartition:{}",
                    subpartition.index, subpartition.partition, subpartition.name
                )),
            );
            let inventory_object = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "INDEX SUBPARTITION".to_owned(),
                    subpartition.index.clone(),
                    subpartition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle index subpartition {}.{}.{}",
                    subpartition.owner, subpartition.index, subpartition.name
                ),
            )?;
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: subpartition.name.clone(),
                extension_kind: Some("oracle_index_subpartition".to_owned()),
                definition: subpartition.high_value.clone(),
                properties: oracle_index_subpartition_properties(subpartition, inventory_object),
            });
        }

        let mut lob_keys = BTreeMap::new();
        for lob in &raw.lobs {
            let parent_key = required(
                column_keys.get(&(lob.owner.clone(), lob.table.clone(), lob.column.clone())),
                format!(
                    "parent column for Oracle LOB {}.{}.{}",
                    lob.owner, lob.table, lob.column
                ),
            )?;
            let segment_inventory = required(
                inventory.get(&(
                    lob.owner.clone(),
                    "LOB".to_owned(),
                    lob.segment_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB segment {}.{}",
                    lob.owner, lob.segment_name
                ),
            )?;
            let index_inventory = required(
                inventory.get(&(
                    lob.owner.clone(),
                    "INDEX".to_owned(),
                    lob.index_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB index {}.{}",
                    lob.owner, lob.index_name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &lob.owner,
                ObjectKind::Extension,
                &lob.table,
                Some(format!("column:{}:lob:{}", lob.column, lob.segment_name)),
            );
            lob_keys.insert(
                (lob.owner.clone(), lob.table.clone(), lob.column.clone()),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key,
                parent_key: Some(parent_key.clone()),
                name: lob.segment_name.clone(),
                extension_kind: Some("oracle_lob_storage".to_owned()),
                definition: None,
                properties: oracle_lob_properties(lob, segment_inventory, index_inventory),
            });
        }

        let lobs_by_identity = raw
            .lobs
            .iter()
            .map(|lob| {
                (
                    (lob.owner.clone(), lob.table.clone(), lob.column.clone()),
                    lob,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut lob_partition_keys = BTreeMap::new();
        for partition in &raw.lob_partitions {
            let lob_identity = (
                partition.owner.clone(),
                partition.table.clone(),
                partition.column.clone(),
            );
            let lob = required(
                lobs_by_identity.get(&lob_identity),
                format!(
                    "parent LOB for Oracle partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let parent_key = required(
                lob_keys.get(&lob_identity),
                format!(
                    "parent LOB key for Oracle partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let table_partition_key = required(
                table_partition_keys.get(&(
                    partition.owner.clone(),
                    partition.table.clone(),
                    partition.table_partition.clone(),
                )),
                format!(
                    "table partition key for Oracle LOB partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let segment_inventory = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "LOB PARTITION".to_owned(),
                    partition.lob_name.clone(),
                    partition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB partition {}.{}.{}",
                    partition.owner, partition.table, partition.name
                ),
            )?;
            let index_inventory = required(
                subobject_inventory.get(&(
                    partition.owner.clone(),
                    "INDEX PARTITION".to_owned(),
                    lob.index_name.clone(),
                    partition.index_partition_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB index partition {}.{}",
                    partition.owner, partition.index_partition_name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &partition.owner,
                ObjectKind::Extension,
                &partition.table,
                Some(format!(
                    "column:{}:lob:{}:partition:{}",
                    partition.column, partition.lob_name, partition.name
                )),
            );
            lob_partition_keys.insert(
                (
                    partition.owner.clone(),
                    partition.lob_name.clone(),
                    partition.name.clone(),
                ),
                key.clone(),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent_key.clone()),
                name: partition.name.clone(),
                extension_kind: Some("oracle_lob_partition".to_owned()),
                definition: None,
                properties: oracle_lob_partition_properties(
                    partition,
                    segment_inventory,
                    index_inventory,
                ),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension(
                    "oracle_lob_partition_storage".to_owned(),
                ),
                from_key: key,
                to_key: table_partition_key.clone(),
                ordinal: Some(positive_u32(
                    partition.position,
                    "Oracle LOB partition relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
        }

        for subpartition in &raw.lob_subpartitions {
            let lob = required(
                lobs_by_identity.get(&(
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.column.clone(),
                )),
                format!(
                    "parent LOB for Oracle subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let parent_key = required(
                lob_partition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.lob_name.clone(),
                    subpartition.lob_partition_name.clone(),
                )),
                format!(
                    "parent LOB partition key for Oracle subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let table_subpartition_key = required(
                table_subpartition_keys.get(&(
                    subpartition.owner.clone(),
                    subpartition.table.clone(),
                    subpartition.table_subpartition.clone(),
                )),
                format!(
                    "table subpartition key for Oracle LOB subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let segment_inventory = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "LOB SUBPARTITION".to_owned(),
                    subpartition.lob_name.clone(),
                    subpartition.name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB subpartition {}.{}.{}",
                    subpartition.owner, subpartition.table, subpartition.name
                ),
            )?;
            let index_inventory = required(
                subobject_inventory.get(&(
                    subpartition.owner.clone(),
                    "INDEX SUBPARTITION".to_owned(),
                    lob.index_name.clone(),
                    subpartition.index_subpartition_name.clone(),
                )),
                format!(
                    "inventory row for Oracle LOB index subpartition {}.{}",
                    subpartition.owner, subpartition.index_subpartition_name
                ),
            )?;
            let key = oracle_key(
                self.connection_alias,
                &database_name,
                &subpartition.owner,
                ObjectKind::Extension,
                &subpartition.table,
                Some(format!(
                    "column:{}:lob:{}:partition:{}:subpartition:{}",
                    subpartition.column,
                    subpartition.lob_name,
                    subpartition.lob_partition_name,
                    subpartition.name
                )),
            );
            metadata.objects.push(MetadataObject {
                key: key.clone(),
                parent_key: Some(parent_key.clone()),
                name: subpartition.name.clone(),
                extension_kind: Some("oracle_lob_subpartition".to_owned()),
                definition: None,
                properties: oracle_lob_subpartition_properties(
                    subpartition,
                    segment_inventory,
                    index_inventory,
                ),
            });
            metadata.relationships.push(MetadataRelationship {
                kind: MetadataRelationshipKind::Extension(
                    "oracle_lob_subpartition_storage".to_owned(),
                ),
                from_key: key,
                to_key: table_subpartition_key.clone(),
                ordinal: Some(positive_u32(
                    subpartition.position,
                    "Oracle LOB subpartition relationship ordinal",
                )?),
                properties: BTreeMap::new(),
            });
        }

        let mut triggers = Vec::new();
        let mut trigger_keys = BTreeMap::new();
        let mut trigger_targets = BTreeMap::new();
        for trigger in &raw.triggers {
            let inventory_object = required(
                inventory.get(&(
                    trigger.owner.clone(),
                    "TRIGGER".to_owned(),
                    trigger.name.clone(),
                )),
                format!(
                    "inventory row for Oracle trigger {}.{}",
                    trigger.owner, trigger.name
                ),
            )?;
            let definition = oracle_trigger_definition(trigger)?;
            let properties = oracle_trigger_properties(trigger, inventory_object);
            match trigger.base_object_type.as_str() {
                "TABLE" | "VIEW" => {
                    let target_owner = trigger.table_owner.as_deref().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "Oracle trigger {}.{} has no target owner",
                            trigger.owner, trigger.name
                        ))
                    })?;
                    let target_name = trigger.table_name.as_deref().ok_or_else(|| {
                        CatalogError::Mapping(format!(
                            "Oracle trigger {}.{} has no target object",
                            trigger.owner, trigger.name
                        ))
                    })?;
                    let target_key = if trigger.base_object_type == "TABLE" {
                        required(
                            table_keys.get(&(target_owner.to_owned(), target_name.to_owned())),
                            format!(
                                "target table key for Oracle trigger {}.{}",
                                trigger.owner, trigger.name
                            ),
                        )?
                    } else {
                        required(
                            view_keys.get(&(target_owner.to_owned(), target_name.to_owned())),
                            format!(
                                "target view key for Oracle trigger {}.{}",
                                trigger.owner, trigger.name
                            ),
                        )?
                    };
                    let key = oracle_key(
                        self.connection_alias,
                        &database_name,
                        target_owner,
                        ObjectKind::Trigger,
                        target_name,
                        Some(trigger.name.clone()),
                    );
                    trigger_keys.insert((trigger.owner.clone(), trigger.name.clone()), key.clone());
                    trigger_targets.insert(
                        (trigger.owner.clone(), trigger.name.clone()),
                        (
                            target_owner.to_owned(),
                            target_name.to_owned(),
                            trigger.base_object_type.clone(),
                        ),
                    );
                    triggers.push(TriggerObject {
                        key: key.clone(),
                        table_key: target_key.clone(),
                        name: trigger.name.clone(),
                        timing: Some(oracle_trigger_timing(&trigger.trigger_type)?),
                        events: oracle_trigger_events(&trigger.triggering_event)?,
                        definition: Some(definition),
                        executes_routine_key: None,
                    });
                    metadata.annotations.push(ObjectAnnotation {
                        object_key: key,
                        definition: None,
                        properties,
                    });
                }
                "SCHEMA" | "DATABASE" => {
                    let (parent_key, target_name) = if trigger.base_object_type == "SCHEMA" {
                        (
                            required(
                                schema_keys.get(&trigger.owner),
                                format!(
                                    "schema key for Oracle trigger {}.{}",
                                    trigger.owner, trigger.name
                                ),
                            )?,
                            trigger.owner.as_str(),
                        )
                    } else {
                        (&database_key, database_name.as_str())
                    };
                    let key = oracle_key(
                        self.connection_alias,
                        &database_name,
                        &trigger.owner,
                        ObjectKind::Trigger,
                        target_name,
                        Some(trigger.name.clone()),
                    );
                    trigger_keys.insert((trigger.owner.clone(), trigger.name.clone()), key.clone());
                    metadata.objects.push(MetadataObject {
                        key,
                        parent_key: Some(parent_key.clone()),
                        name: trigger.name.clone(),
                        extension_kind: None,
                        definition: Some(definition),
                        properties,
                    });
                }
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle trigger target kind '{other}'"
                    )));
                }
            }
        }
        for dependency in raw
            .dependencies
            .iter()
            .filter(|dependency| dependency.object_type == "TRIGGER")
            .filter(|dependency| !dependency.referenced_owner_oracle_maintained)
        {
            if let Some(target) =
                trigger_targets.get(&(dependency.owner.clone(), dependency.name.clone()))
            {
                if dependency.referenced_owner == target.0
                    && dependency.referenced_name == target.1
                    && dependency.referenced_type == target.2
                {
                    continue;
                }
            }
            let source_key = required(
                trigger_keys.get(&(dependency.owner.clone(), dependency.name.clone())),
                format!(
                    "source key for Oracle trigger dependency {}.{}",
                    dependency.owner, dependency.name
                ),
            )?;
            let (target_key, relationship_kind) = match dependency.referenced_type.as_str() {
                "TABLE" => (
                    match materialized_view_keys.get(&(
                        dependency.referenced_owner.clone(),
                        dependency.referenced_name.clone(),
                    )) {
                        Some(key) => key,
                        None => required(
                            table_keys.get(&(
                                dependency.referenced_owner.clone(),
                                dependency.referenced_name.clone(),
                            )),
                            format!(
                                "table target for Oracle trigger dependency {}.{}",
                                dependency.referenced_owner, dependency.referenced_name
                            ),
                        )?,
                    },
                    MetadataRelationshipKind::DependsOn,
                ),
                "VIEW" => (
                    required(
                        view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "view target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "MATERIALIZED VIEW" => (
                    required(
                        materialized_view_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "materialized-view target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "SEQUENCE" => (
                    required(
                        sequence_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "sequence target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "FUNCTION" | "PROCEDURE" => (
                    required(
                        routine_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                            dependency.referenced_type.clone(),
                        )),
                        format!(
                            "routine target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::Invokes,
                ),
                "PACKAGE" => (
                    required(
                        package_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "package target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                "TYPE" => (
                    required(
                        type_keys.get(&(
                            dependency.referenced_owner.clone(),
                            dependency.referenced_name.clone(),
                        )),
                        format!(
                            "type target for Oracle trigger dependency {}.{}",
                            dependency.referenced_owner, dependency.referenced_name
                        ),
                    )?,
                    MetadataRelationshipKind::DependsOn,
                ),
                other => {
                    return Err(CatalogError::Mapping(format!(
                        "unmapped Oracle trigger dependency target type '{other}'"
                    )));
                }
            };
            metadata.relationships.push(MetadataRelationship {
                kind: relationship_kind,
                from_key: source_key.clone(),
                to_key: target_key.clone(),
                ordinal: None,
                properties: BTreeMap::from([(
                    "oracle_dependency_type".to_owned(),
                    MetadataValue::String(dependency.dependency_type.clone()),
                )]),
            });
        }

        let snapshot = CanonicalSchemaSnapshot {
            schema: SchemaSnapshot {
                source_kind: ORACLE_SOURCE.to_owned(),
                connection_alias: self.connection_alias.to_owned(),
                database,
                schemas,
                tables,
                columns,
                constraints,
                indexes,
                views,
                triggers,
                routines,
                capabilities: oracle_complete_capabilities(&self.scope),
            },
            metadata,
        };
        let discovered_counts = discovery_counts_from_catalog(&raw, &self.scope);
        let server_version = format!("{} ({})", self.facts.version, self.facts.release);
        Ok(CatalogDiscovery {
            snapshot,
            adapter: AdapterIdentity {
                name: "database-memory-oracle-catalog".to_owned(),
                version: ORACLE_ADAPTER_VERSION.to_owned(),
            },
            server: ServerIdentity {
                product: "Oracle Database".to_owned(),
                version: server_version,
            },
            scope: IntrospectionScope {
                catalogs: vec![database_name],
                schemas: self.scope.owners.clone(),
            },
            discovered_counts,
            capability_checks: vec![
                CapabilityCheck {
                    name: "supported_server_version".to_owned(),
                    evidence: format!(
                        "server release '{}' maps to live-certified strategy {}",
                        self.facts.release,
                        self.strategy.strategy_name()
                    ),
                },
                CapabilityCheck {
                    name: "single_container_scope".to_owned(),
                    evidence: format!(
                        "connected container={} con_id={} database={} and root aggregation was rejected",
                        self.facts.container, self.facts.container_id, self.facts.database
                    ),
                },
                CapabilityCheck {
                    name: "dictionary_scope".to_owned(),
                    evidence: format!(
                        "{} covered {} owner(s): {}",
                        self.scope.mode.label(),
                        self.scope.owners.len(),
                        self.scope.owners.join(", ")
                    ),
                },
                CapabilityCheck {
                    name: "stable_read_only_catalog".to_owned(),
                    evidence: "SET TRANSACTION READ ONLY succeeded and two complete dictionary reads were identical"
                        .to_owned(),
                },
                CapabilityCheck {
                    name: "independent_inventory_reconciliation".to_owned(),
                    evidence: format!(
                        "{} non-secondary USER/DBA_OBJECTS rows reconciled against table, index, partition, LOB storage, sequence, view, materialized-view, synonym, type, trigger, routine, and package detail catalogs",
                        raw.inventory.iter().filter(|object| !object.secondary).count()
                    ),
                },
                CapabilityCheck {
                    name: "metadata_only_catalog_queries".to_owned(),
                    evidence: "adapter queried Oracle data dictionary and session metadata only; no application table appears in a FROM clause"
                        .to_owned(),
                },
                CapabilityCheck {
                    name: "dependency_coverage".to_owned(),
                    evidence: format!(
                        "{} unique USER/DBA_DEPENDENCIES row(s) were resolved; {} Oracle-maintained target row(s) were explicitly collapsed",
                        raw.dependencies.len(),
                        raw.dependencies
                            .iter()
                            .filter(|dependency| dependency.referenced_owner_oracle_maintained)
                            .count()
                    ),
                },
                CapabilityCheck {
                    name: "principal_context".to_owned(),
                    evidence: format!(
                        "session_user={} current_schema={} and {} selected principal row(s) were readable",
                        self.facts.session_user,
                        self.facts.current_schema,
                        self.scope.principals.len()
                    ),
                },
            ],
        })
    }
}

trait NamedCatalogColumn {
    fn name(&self) -> &str;
}

