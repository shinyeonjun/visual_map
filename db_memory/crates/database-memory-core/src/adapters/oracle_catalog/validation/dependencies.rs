fn normalize_partition_high_value(
    owner: &str,
    object: &str,
    partition: &str,
    length: i64,
    value: Option<String>,
) -> Result<Option<String>, CatalogError> {
    if length < 0 {
        return Err(CatalogError::Mapping(format!(
            "Oracle partition {owner}.{object}.{partition} has negative high-value length"
        )));
    }
    if length > MAX_DEFINITION_BYTES as i64 {
        return Err(CatalogError::UnsupportedMetadata(format!(
            "Oracle partition boundary exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {owner}.{object}.{partition}"
        )));
    }
    normalize_definition(value)
}

fn read_dependencies(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawDependency>, CatalogError> {
    let mut dependencies = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       D.NAME,
                       D.TYPE,
                       D.REFERENCED_OWNER,
                       D.REFERENCED_NAME,
                       D.REFERENCED_TYPE,
                       D.REFERENCED_LINK_NAME,
                       D.DEPENDENCY_TYPE,
                       U.ORACLE_MAINTAINED
                FROM USER_DEPENDENCIES D
                LEFT JOIN ALL_USERS U ON U.USERNAME = D.REFERENCED_OWNER
                ORDER BY D.NAME, D.TYPE, D.REFERENCED_OWNER, D.REFERENCED_NAME, D.REFERENCED_TYPE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT D.OWNER,
                       D.NAME,
                       D.TYPE,
                       D.REFERENCED_OWNER,
                       D.REFERENCED_NAME,
                       D.REFERENCED_TYPE,
                       D.REFERENCED_LINK_NAME,
                       D.DEPENDENCY_TYPE,
                       U.ORACLE_MAINTAINED
                FROM DBA_DEPENDENCIES D
                LEFT JOIN DBA_USERS U ON U.USERNAME = D.REFERENCED_OWNER
                WHERE D.OWNER = :1
                ORDER BY D.OWNER, D.NAME, D.TYPE, D.REFERENCED_OWNER, D.REFERENCED_NAME, D.REFERENCED_TYPE
                "
            }
        };
        let rows = connection.query_as::<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        )>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                object_type,
                referenced_owner,
                referenced_name,
                referenced_type,
                referenced_link,
                dependency_type,
                referenced_owner_oracle_maintained,
            ) = row?;
            let referenced_owner_oracle_maintained = match referenced_owner_oracle_maintained
                .as_deref()
            {
                Some("Y") => true,
                Some("N") => false,
                value => {
                    return Err(CatalogError::Mapping(format!(
                        "Oracle dependency target owner {referenced_owner} has unprovable ORACLE_MAINTAINED state '{}'",
                        value.unwrap_or("missing")
                    )));
                }
            };
            dependencies.push(RawDependency {
                owner,
                name,
                object_type,
                referenced_owner,
                referenced_name,
                referenced_type,
                referenced_link,
                dependency_type,
                referenced_owner_oracle_maintained,
            });
        }
    }
    dependencies.sort_by(|left, right| {
        (
            &left.owner,
            &left.name,
            &left.object_type,
            &left.referenced_owner,
            &left.referenced_name,
            &left.referenced_type,
        )
            .cmp(&(
                &right.owner,
                &right.name,
                &right.object_type,
                &right.referenced_owner,
                &right.referenced_name,
                &right.referenced_type,
            ))
    });
    dependencies.dedup();
    Ok(dependencies)
}

fn oracle_package_dependency_groups(
    dependencies: &[RawDependency],
) -> BTreeMap<CollapsedDependencyIdentity, CollapsedDependencyEvidence> {
    let mut groups = BTreeMap::<CollapsedDependencyIdentity, CollapsedDependencyEvidence>::new();
    for dependency in dependencies.iter().filter(|dependency| {
        matches!(dependency.object_type.as_str(), "PACKAGE" | "PACKAGE BODY")
            && !dependency.referenced_owner_oracle_maintained
            && !(dependency.object_type == "PACKAGE BODY"
                && dependency.referenced_type == "PACKAGE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name)
    }) {
        let evidence = groups
            .entry((
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
                dependency.referenced_type.clone(),
            ))
            .or_default();
        evidence
            .source_object_types
            .insert(dependency.object_type.clone());
        evidence
            .dependency_types
            .insert(dependency.dependency_type.clone());
    }
    groups
}

fn oracle_type_dependency_groups(
    dependencies: &[RawDependency],
) -> BTreeMap<CollapsedDependencyIdentity, CollapsedDependencyEvidence> {
    let mut groups = BTreeMap::<CollapsedDependencyIdentity, CollapsedDependencyEvidence>::new();
    for dependency in dependencies.iter().filter(|dependency| {
        matches!(dependency.object_type.as_str(), "TYPE" | "TYPE BODY")
            && !dependency.referenced_owner_oracle_maintained
            && !(dependency.object_type == "TYPE BODY"
                && dependency.referenced_type == "TYPE"
                && dependency.owner == dependency.referenced_owner
                && dependency.name == dependency.referenced_name)
    }) {
        let evidence = groups
            .entry((
                dependency.owner.clone(),
                dependency.name.clone(),
                dependency.referenced_owner.clone(),
                dependency.referenced_name.clone(),
                dependency.referenced_type.clone(),
            ))
            .or_default();
        evidence
            .source_object_types
            .insert(dependency.object_type.clone());
        evidence
            .dependency_types
            .insert(dependency.dependency_type.clone());
    }
    groups
}
