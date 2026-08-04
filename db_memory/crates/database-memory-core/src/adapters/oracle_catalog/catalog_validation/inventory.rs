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
