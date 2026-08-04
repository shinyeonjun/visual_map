fn read_packages(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPackage>, CatalogError> {
    let mut packages = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AUTHID,
                       PROCEDURE_NAME
                FROM USER_PROCEDURES
                WHERE PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OBJECT_NAME
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       OBJECT_TYPE,
                       AUTHID,
                       PROCEDURE_NAME
                FROM DBA_PROCEDURES
                WHERE OWNER = :1
                  AND PROCEDURE_NAME IS NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OWNER, OBJECT_NAME
                "
            }
        };
        let rows = connection.query_as::<(
            String,
            String,
            i64,
            i64,
            Option<String>,
            String,
            String,
            Option<String>,
        )>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                name,
                object_id,
                subprogram_id,
                overload,
                object_type,
                authid,
                procedure_name,
            ) = row?;
            if subprogram_id != 0
                || overload.is_some()
                || object_type.trim() != "PACKAGE"
                || procedure_name.is_some()
            {
                return Err(CatalogError::Mapping(format!(
                    "Oracle package header metadata is malformed for {}.{}",
                    owner, name
                )));
            }
            packages.push(RawPackage {
                owner,
                name,
                object_id,
                authid: authid.trim().to_owned(),
                specification: None,
                body: None,
            });
        }
    }
    packages.sort_by(|left, right| (&left.owner, &left.name).cmp(&(&right.owner, &right.name)));
    Ok(packages)
}

fn attach_package_sources(
    connection: &Connection,
    scope: &DictionaryScope,
    packages: &mut [RawPackage],
    deadline: Instant,
) -> Result<(), CatalogError> {
    let positions = packages
        .iter()
        .enumerate()
        .map(|(position, package)| ((package.owner.clone(), package.name.clone()), position))
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeMap::<(usize, String), String>::new();
    let mut last_lines = BTreeMap::<(usize, String), i64>::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1, NAME, TYPE, LINE, TEXT
                FROM USER_SOURCE
                WHERE TYPE IN ('PACKAGE', 'PACKAGE BODY')
                ORDER BY NAME, TYPE, LINE
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER, NAME, TYPE, LINE, TEXT
                FROM DBA_SOURCE
                WHERE OWNER = :1
                  AND TYPE IN ('PACKAGE', 'PACKAGE BODY')
                ORDER BY OWNER, NAME, TYPE, LINE
                "
            }
        };
        let rows =
            connection.query_as::<(String, String, String, i64, Option<String>)>(sql, &[owner])?;
        for row in rows {
            let (source_owner, name, object_type, line, text) = row?;
            let position = positions
                .get(&(source_owner.clone(), name.clone()))
                .copied()
                .ok_or_else(|| {
                    CatalogError::Mapping(format!(
                        "Oracle package source {}.{} ({object_type}) has no package header",
                        source_owner, name
                    ))
                })?;
            let source_key = (position, object_type.clone());
            let expected_line = last_lines.get(&source_key).copied().unwrap_or(0) + 1;
            if line != expected_line {
                return Err(CatalogError::Mapping(format!(
                    "Oracle package source {}.{} ({object_type}) expected line {expected_line}, found {line}",
                    source_owner, name
                )));
            }
            last_lines.insert(source_key.clone(), line);
            let source = sources.entry(source_key).or_default();
            source.push_str(text.as_deref().unwrap_or_default());
            if source.len() > MAX_DEFINITION_BYTES {
                return Err(CatalogError::UnsupportedMetadata(format!(
                    "Oracle package definition exceeds the {MAX_DEFINITION_BYTES}-byte safety limit for {}.{} ({object_type})",
                    source_owner, name
                )));
            }
        }
    }
    for (position, package) in packages.iter_mut().enumerate() {
        package.specification =
            normalize_definition(sources.remove(&(position, "PACKAGE".to_owned())))?;
        package.body =
            normalize_definition(sources.remove(&(position, "PACKAGE BODY".to_owned())))?;
        if package.specification.is_none() {
            return Err(CatalogError::Mapping(format!(
                "Oracle package {}.{} has no complete specification",
                package.owner, package.name
            )));
        }
    }
    Ok(())
}

fn read_package_routines(
    connection: &Connection,
    scope: &DictionaryScope,
    deadline: Instant,
) -> Result<Vec<RawPackageRoutine>, CatalogError> {
    type PackageRoutineTuple = (
        String,
        String,
        String,
        i64,
        i64,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );
    let mut routines = Vec::new();
    for owner in &scope.owners {
        prepare_call(connection, deadline)?;
        let sql = match scope.mode {
            DictionaryScopeMode::User => {
                "
                SELECT :1,
                       OBJECT_NAME,
                       PROCEDURE_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC
                FROM USER_PROCEDURES
                WHERE PROCEDURE_NAME IS NOT NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OBJECT_NAME, SUBPROGRAM_ID
                "
            }
            DictionaryScopeMode::Dba => {
                "
                SELECT OWNER,
                       OBJECT_NAME,
                       PROCEDURE_NAME,
                       OBJECT_ID,
                       SUBPROGRAM_ID,
                       OVERLOAD,
                       AGGREGATE,
                       PIPELINED,
                       PARALLEL,
                       INTERFACE,
                       DETERMINISTIC,
                       AUTHID,
                       POLYMORPHIC
                FROM DBA_PROCEDURES
                WHERE OWNER = :1
                  AND PROCEDURE_NAME IS NOT NULL
                  AND OBJECT_TYPE = 'PACKAGE'
                ORDER BY OWNER, OBJECT_NAME, SUBPROGRAM_ID
                "
            }
        };
        let rows = connection.query_as::<PackageRoutineTuple>(sql, &[owner])?;
        for row in rows {
            let (
                owner,
                package,
                name,
                object_id,
                subprogram_id,
                overload,
                aggregate,
                pipelined,
                parallel,
                interface,
                deterministic,
                authid,
                polymorphic,
            ) = row?;
            routines.push(RawPackageRoutine {
                owner,
                package,
                name,
                object_id,
                subprogram_id,
                overload: normalize_optional_token(overload),
                aggregate: aggregate.trim() == "YES",
                pipelined: pipelined.trim() == "YES",
                parallel: parallel.trim() == "YES",
                interface: interface.trim() == "YES",
                deterministic: deterministic.trim() == "YES",
                authid: authid.trim().to_owned(),
                polymorphic: match polymorphic.trim() {
                    "" | "NULL" => None,
                    value => Some(value.to_owned()),
                },
            });
        }
    }
    routines.sort_by(|left, right| {
        (&left.owner, &left.package, left.subprogram_id).cmp(&(
            &right.owner,
            &right.package,
            right.subprogram_id,
        ))
    });
    Ok(routines)
}
